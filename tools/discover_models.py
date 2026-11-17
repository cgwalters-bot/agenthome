"""Opt-in model bumps; offline checks use no network."""

import argparse
import copy
from contextlib import contextmanager
from decimal import Decimal
from http.client import HTTPException
import json
import math
import os
from pathlib import Path
import re
import signal
import sys
import urllib.error
from urllib.parse import urlsplit
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
CATALOG = "https://models.dev/api.json"
OPENAI = "https://developers.openai.com/api/docs/models"
ANTHROPIC = "https://platform.claude.com/docs/en/models/overview"
MAX_BYTES = 16 * 1024 * 1024
MAX_PROBES = 8
MAX_RUNTIME_SECONDS = 600
FILES = ("prompt-tiers.json", "opencode/opencode.json",
         "opencode/opencode-runner.json", "roles/opencode/eval-tiered.json",
         "tests/check_config.py")
EVIDENCE = "tools/model-sources.json"
POOL = "praxis"
BROKER_ENV = ("BROKER_BASE_URL", "BROKER_API_KEY")
PROBE_FALLBACK_STATUSES = (404, 405)
JSONC_COMMENT = r"(?m)^[^\S\n]*//[^\n]*"


class ConflictingSuccessor(ValueError):
    """A successor cannot be added without losing existing settings or evidence."""


class UnconfirmedMetadata(ValueError):
    """Sources disagree or are invalid; the bump fails."""


class BrokerSettingsError(ValueError):
    """Only setting names, never their values, are safe to report."""


class DiscoveryDeadline(RuntimeError):
    """Run-wide deadline, not an OSError urllib can wrap as a failed request."""


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON object key")
        result[key] = value
    return result


def jsonc(text):
    return json.loads(re.sub(JSONC_COMMENT, "", text), object_pairs_hook=unique_object)


class NoAuthenticatedRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, *args, **kwargs):
        # A broker login redirect must not send the run token to another origin.
        raise ValueError("authenticated broker redirects are not allowed")


@contextmanager
def deadline(seconds):
    """Bound the Linux CLI even if an SSE peer keeps its socket alive."""
    def expired(signum, frame):
        raise DiscoveryDeadline("model discovery exceeded its runtime budget")

    previous = signal.signal(signal.SIGALRM, expired)
    signal.setitimer(signal.ITIMER_REAL, seconds)
    try:
        yield
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous)


def fetch(url, token=None, payload=None, *, direct=False):
    headers = {"User-Agent": "agenthome-model-discovery/1"}
    if token:
        headers["Authorization"] = "Bearer " + token
    data = None
    if payload is not None:
        headers["Content-Type"] = "application/json"
        data = json.dumps(payload).encode()
    request = urllib.request.Request(url, headers=headers, data=data)
    if token:
        handlers = [NoAuthenticatedRedirect()]
        # Deployed pairs go direct so a proxy never sees the run's token;
        # explicit callers keep their configured proxy route, HTTP or HTTPS.
        if direct:
            handlers.append(urllib.request.ProxyHandler({}))
        opener = urllib.request.build_opener(*handlers)
    else:
        opener = urllib.request.build_opener()
    with opener.open(request, timeout=60) as response:
        body = response.read(MAX_BYTES + 1)
    if len(body) > MAX_BYTES:
        raise ValueError("source exceeds 16 MiB")
    return body.decode()


def identity(model):
    """Only stable, numbered families; no previews, dated aliases or specials."""
    match = re.fullmatch(r"gpt-(\d+(?:\.\d+)?)-(astra|sol|luna)", model)
    if match:
        version = tuple(map(int, match[1].split(".")))
        return "openai", match[2], version if len(version) == 2 else version + (0,)
    match = re.fullmatch(r"claude-(fable|opus|sonnet|haiku)-(\d+)-(\d+)", model)
    if match:
        return "anthropic", match[1], (int(match[2]), int(match[3]))
    raise ValueError(f"unsupported model family: {model}")


def selected_models(tiers):
    return dict.fromkeys(entry["model"].removeprefix(f"{POOL}/")
                         for entry in tiers["pools"][POOL].values() if entry["model"] is not None)


def candidates(tiers, catalog):
    result = []
    for old in selected_models(tiers):
        provider, family, version = identity(old)
        for model in catalog[provider]["models"]:
            try:
                p, f, v = identity(model)
            except ValueError:
                continue
            if (p, f) == (provider, family) and v > version:
                result.append((old, model))
    # Newest first; bounded probes prevent an expanding catalog causing unbounded spend.
    return sorted(set(result), key=lambda pair: (identity(pair[1])[2], pair[1]), reverse=True)


def fallback_candidates(pairs, defined):
    """Keep the first defined target as a manual stop, not a probe target."""
    stopped = set()
    for old, new in pairs:
        if old not in stopped:
            yield old, new
            if new in defined:
                # Without a listing we cannot rule this target out. Do not
                # automatically route the tier to an older intermediate release.
                stopped.add(old)


def select_replacements(pairs, broker, defined, diagnostics):
    """Select at most one successor per old ID and one old ID per new definition."""
    served = set(broker["models"])
    fallback = broker.get("listing_status") in PROBE_FALLBACK_STATUSES
    if fallback:
        pairs = fallback_candidates(pairs, defined)
    considered = set()
    owners = {}
    for old, new in pairs:
        if old in considered or (new not in served and not (fallback and new in defined)):
            continue
        considered.add(old)
        if new in defined:
            action = ("verify availability and repoint its tiers by hand to use it" if fallback
                      else "repoint its tiers by hand to use it")
            diagnostics.append(f"{old}: {new} is already defined; {action}")
            continue
        if new in owners:
            raise ConflictingSuccessor(f"{new} would inherit settings from both {owners[new]} and {old}; resolve the tier selections by hand")
        owners[new] = old
    return {old: new for new, old in owners.items()}


def probe(base, token, model, *, direct=False):
    body = fetch(base + "/responses", token, {
        "model": model, "input": [{"role": "user", "content": "Reply OK."}],
        "store": False, "stream": True, "max_output_tokens": 16,
        "reasoning": {"effort": "low"},
    }, direct=direct)
    for line in body.splitlines():
        if line.startswith("data: {"):
            event = json.loads(line[6:])
            response = event.get("response", {})
            if (event.get("type") == "response.completed"
                    and response.get("status") == "completed"
                    and response.get("model") == model):
                return True
    return False


def evidence_float(text):
    # The ledger is rewritten on each bump, so reject numbers json.dumps
    # cannot emit with their original value (including non-JSON constants).
    value = float(text)
    if not math.isfinite(value) or Decimal(text) != Decimal(str(value)):
        raise ValueError("model evidence number cannot be preserved")
    return value


def load_evidence(root):
    """Only an absent history starts empty; malformed history must not be lost."""
    try:
        evidence = json.loads((root / EVIDENCE).read_text(), object_pairs_hook=unique_object,
                              parse_float=evidence_float, parse_constant=evidence_float)
    except FileNotFoundError:
        return {}
    if not isinstance(evidence, dict):
        raise ValueError("model evidence must be an object")
    return evidence


def broker_settings():
    # The launcher provisions the home config outside the checkout. Do not merge
    # from the project, or send its token to a URL supplied by a partial override.
    names = BROKER_ENV
    explicit = any(name in os.environ for name in names)
    if explicit:
        base, token = (os.environ.get(name) for name in names)
        source = "the caller environment"
    else:
        names = ("baseURL", "apiKey")
        source = "the deployed ~/.config/opencode/opencode.json praxis provider"
        try:
            config = jsonc((Path.home() / ".config/opencode/opencode.json").read_text())
            options = config["provider"][POOL]["options"]
        except (KeyError, TypeError, OSError, ValueError, RuntimeError):
            options = {}
        if not isinstance(options, dict):
            options = {}
        base, token = (options.get(name) for name in names)

    invalid = []
    try:
        url = urlsplit(base if isinstance(base, str) else "")
        host = (url.hostname or "").rstrip(".")
        port = url.port
        if (url.scheme not in ("http", "https") or not host or host == "invalid"
                or host.endswith(".invalid") or port == 0
                or url.username is not None or url.query or url.fragment
                or any(c.isspace() for c in base)):
            invalid.append(names[0])
    except ValueError:
        invalid.append(names[0])
    if (not isinstance(token, str) or not token or token == "unused" or not token.isascii()
            or not token.isprintable() or any(c.isspace() for c in token) or token.startswith("{")):
        invalid.append(names[1])
    if invalid:
        raise BrokerSettingsError(f"missing or invalid {', '.join(invalid)} in {source}; set both {' and '.join(BROKER_ENV)}")
    return base.rstrip("/"), token, not explicit


def listed_record(model, catalog, docs):
    provider = identity(model)[0]
    # The OpenAI overview can concatenate the label and ID as `Model IDgpt-…`.
    prefix = r"(?:(?<![\w.-])|(?<=\bModel ID))" if provider == "openai" else r"(?<![\w.-])"
    if not re.search(prefix + re.escape(model) + r"(?![\w-]|\.\w)", docs[provider]):
        raise UnconfirmedMetadata(f"{model}: absent from provider model overview")
    records = catalog[provider]["models"]
    if model not in records:
        raise UnconfirmedMetadata(f"{model}: absent from catalog")
    return records[model]


def live(tiers, defined):
    base, token, direct = broker_settings()
    catalog = json.loads(fetch(CATALOG))
    docs = {"openai": fetch(OPENAI + ".md"), "anthropic": fetch(ANTHROPIC)}
    pairs = candidates(tiers, catalog)
    served = []
    method = "GET /models"
    listing_status = 200
    try:
        listing = json.loads(fetch(base + "/models", token, direct=direct))
        served = [entry["id"] for entry in listing["data"]]
    except urllib.error.HTTPError as error:
        if error.code not in PROBE_FALLBACK_STATUSES:
            raise
        listing_status = error.code
        error.close()
        method = "POST /responses: completed, matching model ID"
        targets = dict.fromkeys(model for _, model in fallback_candidates(pairs, defined)
                                if model not in defined)
        if len(targets) > MAX_PROBES:
            raise ValueError("more than 8 candidates; inspect catalog before probing")
        for model in targets:
            try:
                if probe(base, token, model, direct=direct):
                    served.append(model)
            except urllib.error.HTTPError as error:
                if error.code not in (400, 404):
                    raise
                print(f"unavailable: {model} (HTTP {error.code})", file=sys.stderr)
    broker = {"method": method, "models": served, "listing_status": listing_status}
    replacements = select_replacements(pairs, broker, defined, [])
    pages = {}
    for model in sorted(replacements.values()):
        if identity(model)[0] == "openai":
            pages[model] = fetch(OPENAI + "/" + model + ".md")
    return {"catalog": catalog, "docs": docs, "pages": pages, "broker": broker}


def metadata(model, sources):
    provider = identity(model)[0]
    record = listed_record(model, sources["catalog"], sources["docs"])
    try:
        cost = {key: record["cost"][key] for key in ("input", "output", "cache_read", "cache_write")}
        limits = {key: record["limit"][key] for key in ("context", "output")}
        limits["input"] = record["limit"].get("input", limits["context"] - limits["output"])
    except (KeyError, TypeError, AttributeError):
        raise UnconfirmedMetadata(f"{model}: incomplete catalog record") from None
    for key, value in cost.items():
        try:
            # Published cache writes are ledger-only; configured writes use the input rate.
            valid = (type(value) in (int, float) and math.isfinite(value)
                     and (value > 0 or (key == "cache_write" and value == 0)))
        except OverflowError:
            valid = False
        if not valid:
            raise UnconfirmedMetadata(f"{model}: invalid price")
    if not all(type(v) is int and v > 0 for v in limits.values()):
        raise UnconfirmedMetadata(f"{model}: invalid limits")
    source = CATALOG
    if provider == "openai":
        page = sources["pages"].get(model)
        if page is None:
            raise UnconfirmedMetadata(f"{model}: no provider page")
        for label, key in (("Input", "input"), ("Output", "output"),
                           ("Cached input", "cache_read"), ("Cache writes", "cache_write")):
            match = re.search(r"\| " + label + r" \| \$([\d.]+) \|", page)
            try:
                value = float(match[1]) if match else None
            except ValueError:
                value = None
            if value != cost[key]:
                raise UnconfirmedMetadata(f"{model}: unconfirmed {key} price")
        for pattern, key in ((r"([\d,]+) context window", "context"),
                             (r"Maximum input tokens: ([\d,]+)", "input"),
                             (r"([\d,]+) max output tokens", "output")):
            match = re.search(pattern, page)
            try:
                value = int(match[1].replace(",", "")) if match else None
            except ValueError:
                value = None
            if value != limits[key]:
                raise UnconfirmedMetadata(f"{model}: unconfirmed {key} limit")
        source = OPENAI + "/" + model + ".md"
    return {"cost": cost, "provider_limit": limits, "source": source,
            "cross_check": CATALOG, "broker": sources["broker"]["method"]}


def edit_jsonc(text, references, *, tiers, additions):
    """Edit managed routes and add model definitions without reformatting other settings."""
    # The input is checked as JSONC by changes(). Track structural tokens and
    # use source spans to edit values without reformatting the file.
    tokens = [token for token in re.finditer(JSONC_COMMENT + r'|"(?:\\.|[^"\\])*"|[{}\[\]:,]', text)
              if not token[0].lstrip().startswith("//")]
    parents, chunks = [], []
    key, end = None, 0
    for index, token in enumerate(tokens):
        word = token[0]
        if word in ("{", "["):
            parents.append(key)
            key = None
        elif word in ("}", "]"):
            parents.pop()
            key = None
        elif word == ",":
            key = None
        elif word.startswith('"'):
            value = json.loads(word)
            if index + 1 < len(tokens) and tokens[index + 1][0] == ":":
                key = value
                if parents[1:] == ["provider", POOL, "models"] and key in additions:
                    model, definition = additions[key]
                    indent = text[text.rfind("\n", 0, token.start()) + 1:token.start()]
                    separator = " " if indent.strip() else "\n" + indent
                    chunks.extend((text[end:token.start()], json.dumps(model), ": ",
                                   json.dumps(definition), ",", separator))
                    end = token.start()
                continue
            path = (*parents[1:], key)
            if tiers:
                managed = len(path) == 4 and path[:2] == ("pools", POOL) and key == "model"
            else:
                managed = path in (("model",), ("small_model",)) or (
                    len(path) == 3 and path[0] == "agent" and path[1] is not None and key == "model")
            if managed and value in references:
                chunks.extend((text[end:token.start()], json.dumps(references[value])))
                end = token.end()
    chunks.append(text[end:])
    return "".join(chunks)


def updated_definition(definition, facts, *, successor):
    entry = copy.deepcopy(definition)
    if "id" in entry:
        entry["id"] = successor
    entry["cost"] = entry["cost"] | facts["cost"]
    # Keep intentional local caps. Cache creation is metered as plain input.
    entry["limit"] = {k: min(v, facts["provider_limit"][k]) for k, v in entry["limit"].items()}
    entry["cost"]["cache_write"] = entry["cost"]["input"]
    facts["configured_limit"] = entry["limit"]
    facts["cache_write_policy"] = "broker meters cache creation as plain input"
    return entry


def changes(root, sources, diagnostics=None):
    if diagnostics is None:
        diagnostics = []
    texts = {name: (root / name).read_text() for name in FILES}
    parsed = {name: jsonc(texts[name]) for name in FILES[:-1]}
    tiers, config = parsed[FILES[0]], parsed[FILES[1]]
    evidence = load_evidence(root)
    models = config["provider"][POOL]["models"]

    replacements = select_replacements(candidates(tiers, sources["catalog"]),
                                       sources["broker"], models, diagnostics)
    additions = {}
    for old, new in replacements.items():
        if new in evidence:
            raise ConflictingSuccessor(f"{new} already has discovery evidence; archive that record before regenerating its definition")
        facts = metadata(new, sources)
        entry = updated_definition(models[old], facts, successor=new)
        facts["previous"] = old
        evidence[new] = facts
        # Keep the old definition: callers outside this repository (tracker's
        # pools.json, actions' deployment.json, AGENT_MODEL) may still name it.
        additions[old] = (new, entry)
    if not additions:
        return {}
    ids = "|".join(map(re.escape, replacements))
    references = {f"{POOL}/{old}": f"{POOL}/{new}" for old, new in replacements.items()}
    for name, text in texts.items():
        if name == FILES[-1]:  # Regression cases contain both bare IDs and references.
            if replacements:
                texts[name] = re.sub(rf"(?<![\w.-])(?:{ids})(?![\w.-])", lambda m: replacements[m[0]], text)
            continue
        texts[name] = edit_jsonc(text, references, tiers=name == FILES[0],
                                 additions=additions if name == FILES[1] else {})
        proposed = jsonc(texts[name])
        if name == FILES[1] and proposed["provider"][POOL]["models"] != (models | dict(additions.values())):
            raise ValueError("proposed models must preserve definitions outside the planned updates")
    texts[EVIDENCE] = json.dumps(evidence, indent=2, sort_keys=True, allow_nan=False) + "\n"
    return {name: text for name, text in texts.items()
            if not (root / name).exists() or (root / name).read_text() != text}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="write proposed updates (default: preview)")
    parser.add_argument("--fixtures", type=Path, help="offline recorded source bundle instead of network")
    args = parser.parse_args()
    try:
        with deadline(MAX_RUNTIME_SECONDS):
            tiers = jsonc((ROOT / FILES[0]).read_text())
            if args.fixtures:
                sources = json.loads(args.fixtures.read_text())
            else:
                config = jsonc((ROOT / FILES[1]).read_text())
                sources = live(tiers, config["provider"][POOL]["models"])
            diagnostics = []
            updates = changes(ROOT, sources, diagnostics)
        for name, text in updates.items():
            if args.write:
                (ROOT / name).write_text(text)
        print(("Updated: " if args.write else "Would update: ") + ", ".join(updates) if updates else "No model updates.")
        for diagnostic in diagnostics:
            print(diagnostic, file=sys.stderr)
    except (BrokerSettingsError, ConflictingSuccessor, DiscoveryDeadline) as error:
        print(f"Model discovery failed: {error}.", file=sys.stderr)
        return 1
    except (ValueError, KeyError, OSError, HTTPException) as error:
        # Do not print request objects, credential-bearing URLs or response bodies.
        print(f"Model discovery failed ({type(error).__name__}); check source schema and broker environment.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
