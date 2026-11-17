"""Opt-in model bumps; stdlib only. No network is used by offline checks."""

import argparse
import copy
from contextlib import contextmanager
import http.client
import ipaddress
import json
import math
import os
from pathlib import Path
import re
import signal
import sys
import urllib.error
import urllib.parse
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
# The checked-in placeholder address, which a run replaces with its proxy's.
PLACEHOLDER_URL = "http://inference.invalid/v1"


class MissingCredentials(Exception):
    """Names what is missing; never carries a credential value."""


class DiscoveryDeadline(Exception):
    """Overall deadline; not an OSError, so urllib won't wrap it as URLError."""


def jsonc(text):
    return json.loads("\n".join(x for x in text.splitlines()
                              if not x.lstrip().startswith("//")))


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
    handlers = [NoAuthenticatedRedirect()] if token else []
    if direct:
        handlers.append(urllib.request.ProxyHandler({}))
    opener = urllib.request.build_opener(*handlers)
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


def candidates(tiers, catalog):
    result = []
    for entry in tiers["pools"]["praxis"].values():
        if entry["model"] is None:
            continue
        old = entry["model"].removeprefix("praxis/")
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


def opencode_config():
    home = os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config"
    return Path(home) / "opencode/opencode.json"


def loopback(host):
    try:
        return ipaddress.ip_address(host).is_loopback
    except ValueError:
        return host == "localhost"


def broker():
    """Return the broker URL, token, and whether to require direct routing.

    agentic-job replaces the checked-in placeholders in the deployed
    opencode.json with its proxy's address and run token; never this checkout's.
    """
    names = ("BROKER_BASE_URL", "BROKER_API_KEY")
    hint = f"set {' and '.join(names)}, or run where {opencode_config()} defines provider.{POOL}.options"
    values = [os.environ.get(name) for name in names]
    deployed = not any(values)
    if not deployed:
        missing = [name for name, value in zip(names, values) if not value]
        if missing:
            raise MissingCredentials("set " + " and ".join(missing))
        base, token = values
    else:
        path = opencode_config()
        try:
            options = jsonc(path.read_text())["provider"][POOL]["options"]
            if not isinstance(options, dict):
                raise ValueError("options must be an object")
            base, token = options.get("baseURL"), options.get("apiKey")
        except (OSError, ValueError, KeyError, TypeError):
            raise MissingCredentials(hint) from None
    if not all(isinstance(v, str) and v.strip() for v in (base, token)):
        raise MissingCredentials(hint + " with string baseURL and apiKey values")
    if any("{env:" in v or "{file:" in v for v in (base, token)):
        raise MissingCredentials(hint + "; references are not resolved, set both broker variables")
    try:
        url = urllib.parse.urlsplit(base)
        valid = (url.scheme in ("http", "https") and url.hostname
                 and not url.username and not url.password and not url.query and not url.fragment)
        url.port  # Validate the port without echoing a credential-bearing URL.
    except ValueError:
        valid = False
    if not valid:
        raise MissingCredentials(hint + "; the broker address is not an http(s) URL without userinfo, query or fragment")
    host = url.hostname.lower().rstrip(".")
    if host == "invalid" or host.endswith(".invalid") or token == "unused":
        raise MissingCredentials(hint + " with a real baseURL and apiKey, not the placeholders")
    if deployed and url.scheme == "http" and not loopback(host):
        # Reading a stored token is implicit; only send it in cleartext to this host.
        raise MissingCredentials(hint + "; a deployed plain-HTTP baseURL must be a loopback "
                                 "address, set both broker variables to use another")
    # The deployed broker is already the run's inference proxy. Do not let a
    # caller's ambient forward proxy receive its token, especially over HTTP.
    return base.rstrip("/"), token, deployed


def load_evidence(root):
    """Only an absent history starts empty; malformed history must not be lost."""
    try:
        evidence = json.loads((root / EVIDENCE).read_text())
    except FileNotFoundError:
        return {}
    if not isinstance(evidence, dict):
        raise ValueError("model evidence must be an object")
    return evidence


def tracked(tiers, evidence):
    """Tier models whose published facts discovery recorded."""
    models = (entry["model"] for entry in tiers["pools"][POOL].values() if entry["model"])
    return [m for m in (m.removeprefix(POOL + "/") for m in models) if m in evidence]


def live(tiers, evidence):
    # Explicit settings, never a checked-in endpoint or credential.
    base, token, direct = broker()
    catalog = json.loads(fetch(CATALOG))
    docs = {"openai": fetch(OPENAI + ".md"), "anthropic": fetch(ANTHROPIC)}
    pairs = candidates(tiers, catalog)
    served = []
    method = "GET /models"
    try:
        listing = json.loads(fetch(base + "/models", token, direct=direct))
        served = [entry["id"] for entry in listing["data"]]
    except urllib.error.HTTPError as error:
        if error.code not in (404, 405):
            raise
        error.close()
        method = "POST /responses: completed, matching model ID"
        if len(pairs) > MAX_PROBES:
            raise ValueError("more than 8 candidates; inspect catalog before probing")
        for _, model in pairs:
            try:
                if probe(base, token, model, direct=direct):
                    served.append(model)
            except urllib.error.HTTPError as error:
                if error.code not in (400, 404):
                    raise
                print(f"unavailable: {model} (HTTP {error.code})", file=sys.stderr)
    sources = {"catalog": catalog, "docs": docs, "pages": {}, "diagnostics": [],
               "broker": {"method": method, "models": served}}
    config = jsonc((ROOT / FILES[1]).read_text())
    defined = config["provider"][POOL]["models"]

    def page(model):
        if model not in sources["pages"] and identity(model)[0] == "openai":
            sources["pages"][model] = fetch(OPENAI + "/" + model + ".md")

    # Confirm successors first. Retired outgoing pages are no longer evidence
    # needed for these bumps and must not prevent an independent tier's update.
    replaced = set()
    skipped = set()
    for old, new in pairs:
        if old in replaced or old in skipped or new not in served:
            continue
        if new in defined:
            skipped.add(old)
            continue
        page(new)
        metadata(new, sources)
        replaced.add(old)
    for model in tracked(tiers, evidence):
        if model in replaced:
            continue
        provider = identity(model)[0]
        if model not in docs[provider] or model not in catalog[provider]["models"]:
            continue  # changes() reports missing corroboration without writing it.
        try:
            page(model)
        except (OSError, ValueError, http.client.HTTPException):
            sources["diagnostics"].append(f"{model}: could not fetch published page; keeping unchanged")
    return sources


def metadata(model, sources):
    provider = identity(model)[0]
    if model not in sources["docs"][provider]:
        raise ValueError(f"{model}: absent from provider model overview")
    record = sources["catalog"][provider]["models"][model]
    cost = {key: record["cost"][key] for key in ("input", "output", "cache_read", "cache_write")}
    limits = {key: record["limit"][key] for key in ("context", "output")}
    limits["input"] = record["limit"].get("input", limits["context"] - limits["output"])
    for value in cost.values():
        if type(value) not in (int, float) or not math.isfinite(value) or value <= 0:
            raise ValueError(f"{model}: invalid price")
    if not all(type(v) is int and v > 0 for v in limits.values()):
        raise ValueError(f"{model}: invalid limits")
    source = CATALOG
    if provider == "openai":
        page = sources["pages"][model]
        for label, key in (("Input", "input"), ("Output", "output"),
                           ("Cached input", "cache_read"), ("Cache writes", "cache_write")):
            match = re.search(r"\| " + label + r" \| \$([\d.]+) \|", page)
            if not match or float(match[1]) != cost[key]:
                raise ValueError(f"{model}: unconfirmed {key} price")
        for pattern, key in ((r"([\d,]+) context window", "context"),
                             (r"Maximum input tokens: ([\d,]+)", "input"),
                             (r"([\d,]+) max output tokens", "output")):
            match = re.search(pattern, page)
            if not match or int(match[1].replace(",", "")) != limits[key]:
                raise ValueError(f"{model}: unconfirmed {key} limit")
        source = OPENAI + "/" + model + ".md"
    return {"cost": cost, "provider_limit": limits, "source": source,
            "cross_check": CATALOG, "broker": sources["broker"]["method"]}


def changes(root, sources, diagnostics=None):
    if diagnostics is None:
        diagnostics = []
    texts = {name: (root / name).read_text() for name in FILES}
    tiers = jsonc(texts[FILES[0]])
    config = jsonc(texts[FILES[1]])
    evidence = load_evidence(root)
    models = config["provider"][POOL]["models"]

    def configured(model, facts):
        entry = copy.deepcopy(models[model])
        entry["cost"] = copy.deepcopy(facts["cost"])
        # Preserve intentional local caps rather than enabling expensive long contexts.
        entry["limit"] = {k: min(v, facts["provider_limit"][k]) for k, v in entry["limit"].items()}
        # The broker meters cache creation as plain input, not provider cache writes.
        entry["cost"]["cache_write"] = entry["cost"]["input"]
        facts["configured_limit"] = entry["limit"]
        facts["cache_write_policy"] = "broker meters cache creation as plain input"
        return entry

    def rewrite(model, replace):
        pattern = r'(?m)^([ \t]*)"' + re.escape(model) + r'": \{.*$'
        texts[FILES[1]], count = re.subn(pattern, replace, texts[FILES[1]])
        if count != 1:
            raise ValueError(f"expected one single-line model definition for {model}")

    replacements = {}
    skipped = set()
    for old, new in candidates(tiers, sources["catalog"]):
        if old in replacements or old in skipped or new not in sources["broker"]["models"]:
            continue
        if new in models:
            diagnostics.append(f"{old}: {new} is already defined; repoint its tiers by hand to use it; keeping unchanged")
            skipped.add(old)
            continue
        facts = metadata(new, sources)
        entry = configured(old, facts)
        facts["previous"] = old
        evidence[new] = facts
        replacements[old] = new
        # Keep the old definition: callers outside this repository (tracker's
        # pools.json, actions' deployment.json, AGENT_MODEL) may still name it.
        rewrite(old, lambda m: m[1] + json.dumps(new) + ": " + json.dumps(entry) + ",\n"
                + m[1] + f"// Superseded by {new}; kept while callers still name it.\n" + m[0])
    same_id = []
    for model in tracked(tiers, evidence):
        if model in replacements:
            continue
        try:
            facts = metadata(model, sources)
        except (ValueError, KeyError):
            diagnostics.append(f"{model}: cannot corroborate published metadata; keeping unchanged")
            continue
        recorded = evidence[model]
        if (facts["cost"], facts["provider_limit"]) == (recorded["cost"], recorded["provider_limit"]):
            continue
        entry = configured(model, facts)
        # The broker was not asked again; it served this ID before.
        facts["broker"] = recorded["broker"]
        if "previous" in recorded:
            facts["previous"] = recorded["previous"]
        evidence[model] = facts
        same_id.append(model)
        rewrite(model, lambda m: m[1] + json.dumps(model) + ": " + json.dumps(entry) + ",")
    if not replacements and not same_id:
        return {}
    for name, text in texts.items():
        for old, new in replacements.items():
            # opencode.json keeps the old definition; repoint only its references.
            if name == FILES[1]:
                text = text.replace(f'"{POOL}/{old}"', f'"{POOL}/{new}"')
            else:
                text = text.replace(old, new)
        texts[name] = text
    # Model entries except the final one carry commas; preserve valid JSONC.
    texts[FILES[1]] = re.sub(r",(\s*\n\s*})", r"\1", texts[FILES[1]])
    jsonc(texts[FILES[1]])
    texts[EVIDENCE] = json.dumps(evidence, indent=2, sort_keys=True) + "\n"
    return {name: text for name, text in texts.items()
            if not (root / name).exists() or (root / name).read_text() != text}


def preview(root, updates):
    """Structured old/new JSON values, including model definitions and evidence."""
    result = []

    def compare(name, path, old, new):
        if isinstance(old, dict) and isinstance(new, dict):
            for key in sorted(old.keys() | new.keys()):
                compare(name, path + [key], old.get(key), new.get(key))
        elif old != new:
            result.append({"file": name, "path": path, "old": old, "new": new})

    for name, text in updates.items():
        if name.endswith(".json"):
            path = root / name
            old = jsonc(path.read_text()) if path.exists() else {}
            compare(name, [], old, jsonc(text))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="write proposed bumps (default: preview)")
    parser.add_argument("--fixtures", type=Path, help="offline recorded source bundle instead of network")
    parser.add_argument("--json", action="store_true", help="emit structured preview with old/new values and diagnostics")
    args = parser.parse_args()
    try:
        with deadline(MAX_RUNTIME_SECONDS):
            tiers = jsonc((ROOT / FILES[0]).read_text())
            if args.fixtures:
                sources = json.loads(args.fixtures.read_text())
            else:
                evidence = load_evidence(ROOT)
                sources = live(tiers, evidence)
            diagnostics = list(sources.get("diagnostics", []))
            updates = changes(ROOT, sources, diagnostics)
            details = preview(ROOT, updates)
        for name, text in updates.items():
            if args.write:
                (ROOT / name).write_text(text)
        if args.json:
            print(json.dumps({"applied": args.write, "files": list(updates),
                              "changes": details, "diagnostics": diagnostics}, indent=2))
        else:
            print(("Updated: " if args.write else "Would update: ") + ", ".join(updates) if updates else "No model changes.")
            for item in details:
                print(f"{item['file']} {'.'.join(item['path'])}: "
                      f"{json.dumps(item['old'], sort_keys=True)} -> {json.dumps(item['new'], sort_keys=True)}")
        for diagnostic in diagnostics:
            print(diagnostic, file=sys.stderr)
    except MissingCredentials as error:
        print(f"Model discovery needs broker credentials: {error}.", file=sys.stderr)
        return 1
    except DiscoveryDeadline as error:
        print(f"Model discovery failed: {error}.", file=sys.stderr)
        return 1
    except (ValueError, KeyError, OSError) as error:
        # Do not print request objects, credential-bearing URLs or response bodies.
        print(f"Model discovery failed ({type(error).__name__}); check source schema and broker environment.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
