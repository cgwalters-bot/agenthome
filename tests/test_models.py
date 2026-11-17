"""Offline model discovery with recorded source excerpts; never contacts inference."""

import copy
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import unittest
from unittest import mock
import urllib.error

import check_config

ROOT = Path(__file__).resolve().parent.parent
sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("discover_models", ROOT / "tools/discover_models.py")
models = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(models)


class DiscoveryTests(unittest.TestCase):
    def setUp(self):
        self.sources = json.loads((ROOT / "tests/fixtures/model-discovery.json").read_text())
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        baseline = json.loads((ROOT / "tests/fixtures/model-baseline.json").read_text())
        for name, text in baseline.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        (self.root / "tools").mkdir()

    def write(self, updates):
        for name, text in updates.items():
            (self.root / name).write_text(text)

    def test_recorded_bump_and_noop(self):
        before = models.jsonc((self.root / "opencode/opencode.json").read_text())["provider"]["praxis"]["models"]
        updates = models.changes(self.root, self.sources)
        self.assertEqual(set(updates), set(models.FILES) | {"tools/model-sources.json"})
        config = models.jsonc(updates["opencode/opencode.json"])
        self.assertEqual(config["model"], "praxis/gpt-6.2-sol")
        defined = config["provider"]["praxis"]["models"]
        astra = defined["gpt-6.2-astra"]
        self.assertEqual(astra["cost"], {"input": 8, "output": 40, "cache_read": 0.4, "cache_write": 8})
        self.assertEqual(astra["limit"], {"context": 200000, "input": 200000, "output": 128000})
        # Replaced models stay defined, unchanged, for callers that still name them.
        for old in ("gpt-6.1-sol", "gpt-6-astra"):
            self.assertEqual(defined[old], before[old], old)
        evidence = json.loads(updates["tools/model-sources.json"])
        self.assertEqual(evidence["gpt-6.2-astra"]["cost"]["cache_write"], 10)
        self.assertEqual(evidence["gpt-6.2-sol"]["cost"]["cache_write"], 2.5)
        for name, text in updates.items():
            (self.root / name).write_text(text)
        self.assertEqual(models.changes(self.root, self.sources), {})

    def test_deployed_broker_pair_reaches_live_requests(self):
        config = self.root / ".config/opencode/opencode.json"
        config.parent.mkdir(parents=True)
        base, token = "https://broker.example/openai", "fixture-token"
        config.write_text(json.dumps({"provider": {"praxis": {"options": {
            "baseURL": base + "/", "apiKey": token}}}}))
        served = self.sources["broker"]["models"]
        pages_requested = []

        def fetch(url, credential=None, payload=None, *, direct=False):
            self.assertIsNone(payload)
            if url == base + "/models":
                self.assertEqual(credential, token)
                self.assertTrue(direct)
                return json.dumps({"data": [{"id": model} for model in served]})
            self.assertIsNone(credential)
            self.assertFalse(direct)
            if url == models.CATALOG:
                return json.dumps(self.sources["catalog"])
            if url == models.OPENAI + ".md":
                return self.sources["docs"]["openai"]
            if url == models.ANTHROPIC:
                return self.sources["docs"]["anthropic"]
            pages_requested.append(url)
            return self.sources["pages"][url.rsplit("/", 1)[1].removesuffix(".md")]

        names = (*models.FILES, models.EVIDENCE)
        before = {name: (self.root / name).read_bytes() for name in models.FILES}
        with (mock.patch.dict("os.environ", {}, clear=True), mock.patch.object(Path, "home", return_value=self.root),
              mock.patch.object(models, "ROOT", self.root), mock.patch.object(models, "fetch", side_effect=fetch),
              mock.patch.object(sys, "argv", ["discover-models"]), mock.patch("sys.stdout", new_callable=io.StringIO) as out):
            self.assertEqual(models.main(), 0)
        self.assertEqual(out.getvalue(), "Would update: " + ", ".join(names) + "\n")
        self.assertCountEqual(pages_requested, [f"{models.OPENAI}/{model}.md" for model in served])
        self.assertEqual({name: (self.root / name).read_bytes() for name in models.FILES}, before)
        self.assertFalse((self.root / models.EVIDENCE).exists())

    def test_missing_broker_settings_fail_before_fetch_without_leaking(self):
        config = self.root / ".config/opencode/opencode.json"
        config.parent.mkdir(parents=True)
        for env, names in (({}, ("baseURL", "apiKey")),
                           ({"BROKER_BASE_URL": "https://broker.example/v1"}, ("BROKER_API_KEY",)),
                           ({"BROKER_API_KEY": "fixture-token"}, ("BROKER_BASE_URL",))):
            config.write_text(json.dumps({"provider": {"praxis": {"options": {
                "baseURL": "https://deployed.example/v1" if env else "https://placeholder.invalid/v1",
                "apiKey": "deployed-fixture" if env else "unused"}}}}))
            with (self.subTest(names=names),
                    mock.patch.dict("os.environ", env, clear=True), mock.patch.object(Path, "home", return_value=self.root),
                    mock.patch.object(models, "ROOT", self.root), mock.patch.object(models, "fetch") as fetch,
                    mock.patch.object(sys, "argv", ["discover-models"]), mock.patch("sys.stderr", new_callable=io.StringIO) as err):
                self.assertEqual(models.main(), 1)
                for name in names:
                    self.assertIn(name, err.getvalue())
                self.assertNotIn("broker.example", err.getvalue())
                self.assertNotIn("fixture-token", err.getvalue())
                fetch.assert_not_called()

    def test_unserved_and_preview_models_are_not_added(self):
        self.sources["broker"]["models"] = []
        self.sources["catalog"]["openai"]["models"]["gpt-9-sol-preview"] = {}
        self.assertEqual(models.changes(self.root, self.sources), {})

    def test_two_distinct_models_cannot_inherit_into_one_successor(self):
        self.write(models.changes(self.root, self.sources))
        path = self.root / "prompt-tiers.json"
        tiers = models.jsonc(path.read_text())
        tiers["pools"]["praxis"]["extra"] = {"model": "praxis/gpt-6.1-sol"}
        path.write_text(json.dumps(tiers))
        sources = copy.deepcopy(self.sources)
        sources["catalog"]["openai"]["models"]["gpt-6.3-sol"] = sources["catalog"]["openai"]["models"]["gpt-6.2-sol"]
        sources["docs"]["openai"] = "gpt-6.3-sol"
        sources["pages"]["gpt-6.3-sol"] = sources["pages"]["gpt-6.2-sol"]
        sources["broker"]["models"] = ["gpt-6.3-sol"]
        fixture = self.root / "sources.json"
        fixture.write_text(json.dumps(sources))
        names = (*models.FILES, models.EVIDENCE)
        before = {name: (self.root / name).read_bytes() for name in names}
        with mock.patch.object(models, "ROOT", self.root), mock.patch.object(sys, "argv", ["discover-models", "--fixtures", str(fixture), "--write"]), mock.patch("sys.stderr", new_callable=io.StringIO) as err:
            self.assertEqual(models.main(), 1)
        for model in ("gpt-6.1-sol", "gpt-6.2-sol", "gpt-6.3-sol"):
            self.assertIn(model, err.getvalue())
        self.assertEqual({name: (self.root / name).read_bytes() for name in names}, before)

    def test_existing_evidence_is_not_overwritten_when_definition_is_missing(self):
        evidence = self.root / models.EVIDENCE
        evidence.write_text('{"gpt-6.2-sol": {"broker": "earlier proof"}}\n')
        with self.assertRaisesRegex(models.ConflictingSuccessor, r"gpt-6\.2-sol already has discovery evidence"):
            models.changes(self.root, self.sources)

    def test_duplicate_evidence_is_rejected_before_a_bump(self):
        evidence = self.root / models.EVIDENCE
        evidence.write_text('{"gpt-6.1-sol": {"broker": "earlier proof"}, "gpt-6.1-sol": {}}\n')
        with self.assertRaisesRegex(ValueError, "duplicate JSON object key"):
            models.changes(self.root, self.sources)

    def test_unrepresentable_evidence_numbers_are_rejected_before_a_bump(self):
        evidence = self.root / models.EVIDENCE
        for number in ("1e999", "0.1234567890123456789", "NaN", "Infinity"):
            with self.subTest(number=number):
                evidence.write_text(f'{{"gpt-6.1-sol": {{"input": {number}}}}}\n')
                with self.assertRaisesRegex(ValueError, "model evidence number cannot be preserved"):
                    models.changes(self.root, self.sources)

    def test_successor_is_inserted_only_in_the_provider_models(self):
        old, new = "gpt-6.1-sol", "gpt-6.2-sol"
        path = self.root / "opencode/opencode.json"
        config = models.jsonc(path.read_text())
        config.setdefault("agent", {})[old] = {"model": f"praxis/{old}"}
        defined = config["provider"]["praxis"]["models"]
        defined[old]["id"] = old
        text = json.dumps(config, indent=2).replace(f'"{old}": {{', f'"{old}":\n  {{', 1)
        # A whole-line comment accepted by the parser must not alter edit paths.
        text = text.replace('  "model":', '\u00a0// }\n  "model":', 1)
        path.write_text(text)

        updates = models.changes(self.root, self.sources)
        proposed = models.jsonc(updates["opencode/opencode.json"])
        self.assertEqual(proposed["model"], f"praxis/{new}")
        self.assertIn("\u00a0// }", updates["opencode/opencode.json"])
        self.assertNotIn(new, proposed["agent"])
        self.assertEqual(proposed["agent"][old]["model"], f"praxis/{new}")
        result = proposed["provider"]["praxis"]["models"]
        self.assertEqual(result[old], defined[old])
        self.assertEqual(result[new]["id"], new)

    def test_only_whole_model_references_are_repointed(self):
        old, new, unrelated = "claude-fable-5-2", "claude-fable-5-3", "claude-fable-5-20"
        for name in models.FILES:
            path = self.root / name
            path.write_text(path.read_text().replace("gpt-6.1-sol", old))
        path = self.root / "opencode/opencode.json"
        text = path.read_text()
        definition = next(line for line in text.splitlines() if f'"{old}":' in line)
        path.write_text(text.replace(definition, definition + "\n" + definition.replace(old, unrelated)))
        path = self.root / "prompt-tiers.json"
        tiers = models.jsonc(path.read_text())
        tiers["pools"]["praxis"]["other"] = {"model": f"praxis/{unrelated}"}
        path.write_text(json.dumps(tiers))
        catalog = self.sources["catalog"]["anthropic"]["models"]
        catalog[new] = catalog[old]
        self.sources["docs"]["anthropic"] = new
        self.sources["broker"]["models"] = [new]
        updates = models.changes(self.root, self.sources)
        tiers = models.jsonc(updates["prompt-tiers.json"])["pools"]["praxis"]
        self.assertEqual(tiers["sol"]["model"], f"praxis/{new}")
        self.assertEqual(tiers["other"]["model"], f"praxis/{unrelated}")
        defined = models.jsonc(updates["opencode/opencode.json"])["provider"]["praxis"]["models"]
        self.assertTrue({old, new, unrelated} <= set(defined))

    def test_luna_bump_passes_configuration_validator(self):
        for name in models.FILES:
            (self.root / name).write_text((ROOT / name).read_text())
        path = self.root / "prompt-tiers.json"
        config = check_config.load()
        old = config["tiers"]["pools"]["praxis"]["luna"]["model"]
        defined = config["catalogue"]["provider"]["praxis"]["models"]
        # Exercise the checked-in config even after a real bump or rollback.
        latest = max(models.identity(model)[2][0] for model in defined if model.endswith("-luna"))
        new = f"gpt-{latest + 1}-luna"
        text = json.dumps(config["tiers"], indent=2)
        path.write_text(text.replace(f'"model": "{old}"', f'"model":\n        // Keep this comment.\n        "{old}"'))
        catalog = self.sources["catalog"]["openai"]["models"]
        catalog[new] = catalog["gpt-6.2-sol"]
        self.sources["pages"][new] = self.sources["pages"]["gpt-6.2-sol"]
        self.sources["docs"]["openai"] += f" {new}"
        self.sources["broker"]["models"] = [new]
        fixture = self.root / "sources.json"
        fixture.write_text(json.dumps(self.sources))
        with mock.patch.object(models, "ROOT", self.root), mock.patch.object(sys, "argv", ["discover-models", "--fixtures", str(fixture), "--write"]), mock.patch("sys.stdout", new_callable=io.StringIO):
            self.assertEqual(models.main(), 0)
        config["tiers"] = models.jsonc(path.read_text())
        config["catalogue"] = models.jsonc((self.root / "opencode/opencode.json").read_text())
        config["runner"] = models.jsonc((self.root / "opencode/opencode-runner.json").read_text())
        config["profile"] = (self.root / "roles/opencode/eval-tiered.json").read_text()
        self.assertIn("// Keep this comment.", path.read_text())
        self.assertEqual(config["catalogue"]["small_model"], f"praxis/{new}")
        check_config.check(config)
        self.assertEqual(models.changes(self.root, self.sources), {})

    def test_bump_does_not_change_tier_keys_or_other_pool_pins(self):
        old, new = "praxis/gpt-6.1-sol", "praxis/gpt-6.2-sol"
        path = self.root / "prompt-tiers.json"
        config = models.jsonc(path.read_text())
        pools = config["pools"]
        pools["other"] = {"pinned": {"model": old, "fallback": None}}
        pools["praxis"][old] = {"model": old, "fallback": None}
        pools["praxis"][new] = {"model": None, "fallback": "sol"}
        path.write_text(json.dumps(config))
        updates = models.changes(self.root, self.sources)
        proposed = models.jsonc(updates["prompt-tiers.json"])["pools"]
        self.assertEqual(proposed["other"], pools["other"])
        self.assertEqual(proposed["praxis"][old]["model"], new)
        self.assertEqual(proposed["praxis"][new], pools["praxis"][new])

    def test_subsequent_bump_is_independent_of_recorded_generation(self):
        self.write(models.changes(self.root, self.sources))
        sources = copy.deepcopy(self.sources)
        sources["catalog"]["openai"]["models"]["gpt-6.3-sol"] = sources["catalog"]["openai"]["models"].pop("gpt-6.2-sol")
        sources["broker"]["models"] = ["gpt-6.3-sol"]
        sources["docs"]["openai"] = "gpt-6.3-sol"
        sources["pages"]["gpt-6.3-sol"] = sources["pages"]["gpt-6.2-sol"].replace("gpt-6.2-sol", "gpt-6.3-sol")
        updates = models.changes(self.root, sources)
        self.assertEqual(models.jsonc(updates["prompt-tiers.json"])["pools"]["praxis"]["sol"]["model"], "praxis/gpt-6.3-sol")
        defined = models.jsonc(updates["opencode/opencode.json"])["provider"]["praxis"]["models"]
        self.assertTrue({"gpt-6.1-sol", "gpt-6.2-sol", "gpt-6.3-sol"} <= set(defined))
        self.assertIn("gpt-6.2-sol", json.loads(updates["tools/model-sources.json"]))
        self.write(updates)
        self.assertEqual(models.changes(self.root, sources), {})

    def test_retained_target_is_skipped_without_blocking_other_tiers(self):
        self.write(models.changes(self.root, self.sources))
        path = self.root / "prompt-tiers.json"
        path.write_text(path.read_text().replace("gpt-6.2-astra", "gpt-6-astra"))
        sources = copy.deepcopy(self.sources)
        sources["catalog"]["openai"]["models"]["gpt-6.3-sol"] = copy.deepcopy(sources["catalog"]["openai"]["models"]["gpt-6.2-sol"])
        sources["catalog"]["openai"]["models"]["gpt-6.1-astra"] = copy.deepcopy(sources["catalog"]["openai"]["models"]["gpt-6.2-astra"])
        sources["docs"]["openai"] += " gpt-6.3-sol gpt-6.1-astra"
        sources["pages"]["gpt-6.3-sol"] = sources["pages"]["gpt-6.2-sol"]
        sources["pages"]["gpt-6.1-astra"] = sources["pages"]["gpt-6.2-astra"]
        sources["broker"]["models"].extend(["gpt-6.3-sol", "gpt-6.1-astra"])
        sources["broker"]["method"] = "GET /models"
        diagnostics = []
        updates = models.changes(self.root, sources, diagnostics)
        self.assertIn("repoint", " ".join(diagnostics))
        tiers = models.jsonc(updates["prompt-tiers.json"])["pools"]["praxis"]
        self.assertEqual(tiers["astra"]["model"], "praxis/gpt-6-astra")
        self.assertEqual(tiers["sol"]["model"], "praxis/gpt-6.3-sol")
        before = models.jsonc((self.root / "opencode/opencode.json").read_text())
        after = models.jsonc(updates["opencode/opencode.json"])
        self.assertEqual(before["provider"]["praxis"]["models"]["gpt-6.2-astra"], after["provider"]["praxis"]["models"]["gpt-6.2-astra"])
        with mock.patch.dict("os.environ", {"BROKER_BASE_URL": "https://broker.example/v1", "BROKER_API_KEY": "fixture-token"}):
            def fetch(url, token=None, payload=None, *, direct=False):
                if url == models.CATALOG:
                    return json.dumps(sources["catalog"])
                if url.endswith("/models"):
                    return json.dumps({"data": [{"id": m} for m in sources["broker"]["models"]]})
                if url == models.OPENAI + ".md":
                    return sources["docs"]["openai"]
                if url == models.ANTHROPIC:
                    return sources["docs"]["anthropic"]
                self.assertNotIn("gpt-6.1-astra", url)
                return sources["pages"][url.rsplit("/", 1)[1].removesuffix(".md")]

            with mock.patch.object(models, "fetch", side_effect=fetch):
                live = models.live(models.jsonc(path.read_text()), before["provider"]["praxis"]["models"])
                self.assertEqual(models.changes(self.root, live), updates)

    def test_deployed_request_bypasses_proxy_and_refuses_redirect(self):
        seen = []

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                seen.append((self.server, self.path, self.headers.get("Authorization")))
                self.send_response(302)
                self.send_header("Location", "/redirected")
                self.end_headers()

            def log_message(self, *args):
                pass

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        proxy = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        proxy_thread = threading.Thread(target=proxy.serve_forever, daemon=True)
        thread.start()
        proxy_thread.start()
        try:
            with (mock.patch.dict("os.environ", {"http_proxy": f"http://127.0.0.1:{proxy.server_port}"}, clear=True),
                  self.assertRaisesRegex(ValueError, "redirects are not allowed")):
                models.fetch(f"http://127.0.0.1:{server.server_port}/models", "fixture-token", direct=True)
            self.assertEqual(seen, [(server, "/models", "Bearer fixture-token")])
        finally:
            for listener, worker in ((server, thread), (proxy, proxy_thread)):
                listener.shutdown()
                worker.join()
                listener.server_close()

    def test_explicit_broker_keeps_caller_proxy(self):
        proxy = "http://proxy.example:3128"
        for scheme in ("http", "https"):
            with (self.subTest(scheme=scheme),
                  mock.patch.dict("os.environ", {f"{scheme}_proxy": proxy}, clear=True)):
                opener = models.urllib.request.OpenerDirector
                with mock.patch.object(opener, "open", autospec=True) as open_request:
                    open_request.return_value.__enter__.return_value.read.return_value = b"{}"
                    models.fetch(f"{scheme}://broker.example/v1/models", "fixture-token")
                director, request = open_request.call_args.args
                proxy_handlers = [handler for handler in director.handlers
                                  if isinstance(handler, models.urllib.request.ProxyHandler)]
                self.assertEqual([handler.proxies for handler in proxy_handlers], [{scheme: proxy}])
                self.assertTrue(any(isinstance(handler, models.NoAuthenticatedRedirect)
                                    for handler in director.handlers))
                self.assertEqual(request.get_header("Authorization"), "Bearer fixture-token")

    def test_runtime_deadline_interrupts_blocking_work(self):
        with self.assertRaises(models.DiscoveryDeadline):
            with models.deadline(0.02):
                time.sleep(1)

    def test_unconfirmed_and_invalid_metadata_fail_without_writing(self):
        before = (self.root / "opencode/opencode.json").read_text()
        for field, value in (("input", -1), ("output", float("nan")), ("cache_read", 7)):
            with self.subTest(field=field):
                sources = copy.deepcopy(self.sources)
                sources["catalog"]["openai"]["models"]["gpt-6.2-sol"]["cost"][field] = value
                with self.assertRaises(ValueError):
                    models.changes(self.root, sources)
                self.assertEqual((self.root / "opencode/opencode.json").read_text(), before)

    def test_numeric_version_order_and_same_family_only(self):
        tiers = {"pools": {"praxis": {"sol": {"model": "praxis/gpt-6.2-sol"}}}}
        catalog = {"openai": {"models": {"gpt-6.10-sol": {}, "gpt-6.3-sol": {}, "gpt-7-astra": {}}}}
        self.assertEqual(models.candidates(tiers, catalog), [("gpt-6.2-sol", "gpt-6.10-sol"), ("gpt-6.2-sol", "gpt-6.3-sol")])

    def test_claude_family_mapping_without_inventing_tiers(self):
        self.assertEqual(models.identity("claude-fable-5-2"), ("anthropic", "fable", (5, 2)))
        facts = models.metadata("claude-fable-5-2", self.sources)
        self.assertEqual(facts["provider_limit"]["input"], 872000)
        tiers = {"pools": {"praxis": {"fable": {"model": "praxis/claude-fable-5-1"}}}}
        self.assertEqual(models.candidates(tiers, self.sources["catalog"]), [("claude-fable-5-1", "claude-fable-5-2")])

    def test_probe_requires_completed_matching_model(self):
        for status, model, expected in (("completed", "gpt-6.2-sol", True), ("failed", "gpt-6.2-sol", False), ("completed", "other", False)):
            event = {"type": "response.completed", "response": {"model": model, "status": status}}
            with mock.patch.object(models, "fetch", return_value="data: " + json.dumps(event)):
                self.assertEqual(models.probe("http://inference.invalid/v1", "unused", "gpt-6.2-sol"), expected)

    def test_listing_and_probe_fallback(self):
        tiers = models.jsonc((self.root / "prompt-tiers.json").read_text())

        def fetch(url, token=None, payload=None, *, direct=False):
            self.assertFalse(direct)
            self.assertEqual(token, "fixture-token" if url.endswith(("/models", "/responses")) else None)
            if url == models.CATALOG:
                return json.dumps(self.sources["catalog"])
            if url.endswith("/models"):
                error = urllib.error.HTTPError(url, 404, "missing", {}, None)
                self.addCleanup(error.close)
                raise error
            if url.endswith("/responses"):
                return "data: " + json.dumps({"type": "response.completed", "response": {"status": "completed", "model": payload["model"]}})
            if url == models.OPENAI + ".md":
                return self.sources["docs"]["openai"]
            if url == models.ANTHROPIC:
                return self.sources["docs"]["anthropic"]
            return self.sources["pages"][url.rsplit("/", 1)[1].removesuffix(".md")]

        with mock.patch.dict("os.environ", {"BROKER_BASE_URL": "https://broker.example/v1", "BROKER_API_KEY": "fixture-token"}), mock.patch.object(models, "fetch", side_effect=fetch):
            self.assertEqual(set(models.live(tiers, {})["broker"]["models"]), {"gpt-6.2-astra", "gpt-6.2-sol"})

    def test_probe_fallback_keeps_existing_targets_out_of_the_probe_budget(self):
        self.write(models.changes(self.root, self.sources))
        tiers_path = self.root / models.FILES[0]
        # Keep both Sol versions selected. A rollback must not add an older
        # intermediate successor when availability of the kept target is unknown.
        pinned, current, lower, luna = "gpt-5-sol", "gpt-6.2-sol", "gpt-6-sol", "gpt-20-luna"
        tiers = models.jsonc(tiers_path.read_text())
        tiers["pools"]["praxis"]["older-sol"] = {"model": f"praxis/{pinned}"}
        tiers_path.write_text(json.dumps(tiers))
        config_path = self.root / models.FILES[1]
        config = models.jsonc(config_path.read_text())
        defined = config["provider"]["praxis"]["models"]
        defined[pinned] = copy.deepcopy(defined[current])
        catalog = self.sources["catalog"]["openai"]["models"]
        for model in (lower, luna):
            catalog[model] = catalog[current]
        self.sources["docs"]["openai"] += f" {luna}"
        self.sources["pages"][luna] = self.sources["pages"][current]

        for status, last_minor in ((404, 2), (405, 10)):
            with self.subTest(status=status):
                latest = f"gpt-6.{last_minor}-sol"
                for minor in range(3, last_minor + 1):
                    model = f"gpt-6.{minor}-sol"
                    defined[model] = copy.deepcopy(defined[current])
                    catalog[model] = catalog[current]
                config_path.write_text(json.dumps(config))
                probed = []

                def fetch(url, token=None, payload=None, *, direct=False):
                    if url == models.CATALOG:
                        return json.dumps(self.sources["catalog"])
                    if url.endswith("/models"):
                        raise urllib.error.HTTPError(url, status, "missing", {}, None)
                    if url.endswith("/responses"):
                        model = payload["model"]
                        probed.append(model)
                        if model != luna:
                            error = urllib.error.HTTPError(url, 429, "throttled", {}, None)
                            self.addCleanup(error.close)
                            raise error
                        return "data: " + json.dumps({"type": "response.completed",
                                                     "response": {"status": "completed", "model": model}})
                    if url == models.OPENAI + ".md":
                        return self.sources["docs"]["openai"]
                    if url == models.ANTHROPIC:
                        return self.sources["docs"]["anthropic"]
                    self.assertEqual(url, f"{models.OPENAI}/{luna}.md")
                    return self.sources["pages"][luna]

                with (mock.patch.dict("os.environ", {"BROKER_BASE_URL": "https://broker.example/v1", "BROKER_API_KEY": "fixture-token"}),
                      mock.patch.object(models, "fetch", side_effect=fetch)):
                    sources = models.live(tiers, defined)
                self.assertEqual(probed, [luna])
                self.assertEqual(sources["broker"]["models"], [luna])
                diagnostics = []
                updates = models.changes(self.root, sources, diagnostics)
                self.assertTrue(any(latest in note and "repoint" in note for note in diagnostics))
                proposed = models.jsonc(updates[models.FILES[0]])["pools"]["praxis"]
                self.assertEqual(proposed["older-sol"]["model"], f"praxis/{pinned}")
                self.assertEqual(proposed["sol"]["model"], f"praxis/{current}")
                self.assertEqual(proposed["luna"]["model"], f"praxis/{luna}")

    def test_listing_does_not_probe_and_auth_errors_fail(self):
        tiers = models.jsonc((self.root / "prompt-tiers.json").read_text())
        for code in (None, 401):
            with self.subTest(code=code):
                def fetch(url, token=None, payload=None, *, direct=False):
                    self.assertIsNone(payload)
                    if url == models.CATALOG:
                        return json.dumps(self.sources["catalog"])
                    if url.endswith("/models"):
                        if code:
                            error = urllib.error.HTTPError(url, code, "denied", {}, None)
                            self.addCleanup(error.close)
                            raise error
                        return '{"data": []}'
                    return "overview"

                with mock.patch.dict("os.environ", {"BROKER_BASE_URL": "https://broker.example/v1", "BROKER_API_KEY": "fixture-token"}), mock.patch.object(models, "fetch", side_effect=fetch):
                    if code:
                        with self.assertRaises(urllib.error.HTTPError):
                            models.live(tiers, {})
                    else:
                        self.assertEqual(models.live(tiers, {})["broker"]["models"], [])

    def test_missing_overview_or_limit_disagreement_fails(self):
        for mutate in (lambda s: s["docs"].update(openai=""),
                       lambda s: s["catalog"]["openai"]["models"]["gpt-6.2-sol"]["limit"].update(output=42)):
            sources = copy.deepcopy(self.sources)
            mutate(sources)
            with self.assertRaises(ValueError):
                models.changes(self.root, sources)


if __name__ == "__main__":
    unittest.main()
