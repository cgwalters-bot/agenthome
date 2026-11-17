"""Opt-in model bumps; stdlib only. No network is used by offline checks."""

import argparse
import copy
from contextlib import contextmanager
import json
import math
import os
from pathlib import Path
import re
import signal
import sys
import urllib.error
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
        raise TimeoutError("model discovery exceeded its runtime budget")

    previous = signal.signal(signal.SIGALRM, expired)
    signal.setitimer(signal.ITIMER_REAL, seconds)
    try:
        yield
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous)


def fetch(url, token=None, payload=None):
    headers = {"User-Agent": "agenthome-model-discovery/1"}
    if token:
        headers["Authorization"] = "Bearer " + token
    data = None
    if payload is not None:
        headers["Content-Type"] = "application/json"
        data = json.dumps(payload).encode()
    request = urllib.request.Request(url, headers=headers, data=data)
    opener = urllib.request.build_opener(NoAuthenticatedRedirect()) if token else urllib.request.build_opener()
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


def probe(base, token, model):
    body = fetch(base + "/responses", token, {
        "model": model, "input": [{"role": "user", "content": "Reply OK."}],
        "store": False, "stream": True, "max_output_tokens": 16,
        "reasoning": {"effort": "low"},
    })
    for line in body.splitlines():
        if line.startswith("data: {"):
            event = json.loads(line[6:])
            response = event.get("response", {})
            if (event.get("type") == "response.completed"
                    and response.get("status") == "completed"
                    and response.get("model") == model):
                return True
    return False


def live(tiers):
    # Explicit environment settings, never a checked-in endpoint or credential.
    base = os.environ["BROKER_BASE_URL"].rstrip("/")
    token = os.environ["BROKER_API_KEY"]
    catalog = json.loads(fetch(CATALOG))
    docs = {"openai": fetch(OPENAI + ".md"), "anthropic": fetch(ANTHROPIC)}
    pairs = candidates(tiers, catalog)
    served = []
    method = "GET /models"
    try:
        listing = json.loads(fetch(base + "/models", token))
        served = [entry["id"] for entry in listing["data"]]
    except urllib.error.HTTPError as error:
        if error.code not in (404, 405):
            raise
        method = "POST /responses: completed, matching model ID"
        if len(pairs) > MAX_PROBES:
            raise ValueError("more than 8 candidates; inspect catalog before probing")
        for _, model in pairs:
            try:
                if probe(base, token, model):
                    served.append(model)
            except urllib.error.HTTPError as error:
                if error.code not in (400, 404):
                    raise
                print(f"unavailable: {model} (HTTP {error.code})", file=sys.stderr)
    pages = {}
    for _, model in pairs:
        if model in served and identity(model)[0] == "openai":
            pages[model] = fetch(OPENAI + "/" + model + ".md")
    return {"catalog": catalog, "docs": docs, "pages": pages,
            "broker": {"method": method, "models": served}}


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


def changes(root, sources):
    texts = {name: (root / name).read_text() for name in FILES}
    tiers = jsonc(texts[FILES[0]])
    config = jsonc(texts[FILES[1]])
    evidence_path = root / "tools/model-sources.json"
    evidence = json.loads(evidence_path.read_text()) if evidence_path.exists() else {}
    replacements = {}
    for old, new in candidates(tiers, sources["catalog"]):
        if old in replacements or new not in sources["broker"]["models"]:
            continue
        facts = metadata(new, sources)
        previous = config["provider"]["praxis"]["models"][old]
        entry = copy.deepcopy(previous)
        entry["cost"] = copy.deepcopy(facts["cost"])
        # Preserve intentional local caps rather than enabling expensive long contexts.
        entry["limit"] = {k: min(v, facts["provider_limit"][k]) for k, v in previous["limit"].items()}
        # The broker meters cache creation as plain input, not provider cache writes.
        entry["cost"]["cache_write"] = entry["cost"]["input"]
        facts["configured_limit"] = entry["limit"]
        facts["cache_write_policy"] = "broker meters cache creation as plain input"
        facts["previous"] = old
        evidence.pop(old, None)
        evidence[new] = facts
        replacements[old] = new
        pattern = r'(?m)^(\s*)"' + re.escape(old) + r'": \{.*$'
        texts[FILES[1]], count = re.subn(pattern, lambda m: m[1] + json.dumps(old) + ": " + json.dumps(entry) + ",", texts[FILES[1]])
        if count != 1:
            raise ValueError(f"expected one single-line model definition for {old}")
    if not replacements:
        return {}
    for name, text in texts.items():
        for old, new in replacements.items():
            text = text.replace(old, new)
        texts[name] = text
    # Model entries except the final one carry commas; preserve valid JSONC.
    texts[FILES[1]] = re.sub(r",(\s*\n\s*})", r"\1", texts[FILES[1]])
    jsonc(texts[FILES[1]])
    texts["tools/model-sources.json"] = json.dumps(evidence, indent=2, sort_keys=True) + "\n"
    return {name: text for name, text in texts.items()
            if not (root / name).exists() or (root / name).read_text() != text}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="write proposed bumps (default: preview)")
    parser.add_argument("--fixtures", type=Path, help="offline recorded source bundle instead of network")
    args = parser.parse_args()
    try:
        with deadline(MAX_RUNTIME_SECONDS):
            tiers = jsonc((ROOT / FILES[0]).read_text())
            sources = json.loads(args.fixtures.read_text()) if args.fixtures else live(tiers)
            updates = changes(ROOT, sources)
        for name, text in updates.items():
            if args.write:
                (ROOT / name).write_text(text)
        print(("Updated: " if args.write else "Would update: ") + ", ".join(updates) if updates else "No model bumps.")
    except (ValueError, KeyError, OSError) as error:
        # Do not print request objects, credential-bearing URLs or response bodies.
        print(f"Model discovery failed ({type(error).__name__}); check source schema and broker environment.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
