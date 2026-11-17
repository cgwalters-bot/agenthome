"""Offline model discovery with recorded source excerpts; never contacts inference."""

import copy
import http.client
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

    def test_unserved_and_preview_models_are_not_added(self):
        self.sources["broker"]["models"] = []
        self.sources["catalog"]["openai"]["models"]["gpt-9-sol-preview"] = {}
        self.assertEqual(models.changes(self.root, self.sources), {})

    def test_subsequent_bump_is_independent_of_recorded_generation(self):
        self.write(models.changes(self.root, self.sources))
        sources = copy.deepcopy(self.sources)
        sources["catalog"]["openai"]["models"]["gpt-6.3-sol"] = sources["catalog"]["openai"]["models"].pop("gpt-6.2-sol")
        sources["broker"]["models"] = ["gpt-6.3-sol"]
        # The installed astra must still be confirmed from the overview.
        sources["docs"]["openai"] = "gpt-6.3-sol gpt-6.2-astra"
        sources["pages"]["gpt-6.3-sol"] = sources["pages"]["gpt-6.2-sol"].replace("gpt-6.2-sol", "gpt-6.3-sol")
        updates = models.changes(self.root, sources)
        self.assertEqual(models.jsonc(updates["prompt-tiers.json"])["pools"]["praxis"]["sol"]["model"], "praxis/gpt-6.3-sol")
        defined = models.jsonc(updates["opencode/opencode.json"])["provider"]["praxis"]["models"]
        self.assertTrue({"gpt-6.1-sol", "gpt-6.2-sol", "gpt-6.3-sol"} <= set(defined))
        self.assertIn("gpt-6.2-sol", json.loads(updates["tools/model-sources.json"]))
        self.write(updates)
        self.assertEqual(models.changes(self.root, sources), {})

    def test_same_id_price_change_is_reported(self):
        self.write(models.changes(self.root, self.sources))
        sources = copy.deepcopy(self.sources)
        sources["catalog"]["openai"]["models"]["gpt-6.2-sol"]["cost"]["input"] = 3
        sources["pages"]["gpt-6.2-sol"] = sources["pages"]["gpt-6.2-sol"].replace("| Input | $2 |", "| Input | $3 |")
        updates = models.changes(self.root, sources)
        self.assertEqual(set(updates), {"opencode/opencode.json", "tools/model-sources.json"})
        sol = models.jsonc(updates["opencode/opencode.json"])["provider"]["praxis"]["models"]["gpt-6.2-sol"]
        self.assertEqual(sol["cost"], {"input": 3, "output": 10, "cache_read": 0.1, "cache_write": 3})
        evidence = json.loads(updates["tools/model-sources.json"])["gpt-6.2-sol"]
        self.assertEqual((evidence["cost"]["input"], evidence["previous"]), (3, "gpt-6.1-sol"))
        self.write(updates)
        self.assertEqual(models.changes(self.root, sources), {})

    def test_same_id_limit_change_updates_evidence_and_keeps_caps(self):
        self.write(models.changes(self.root, self.sources))
        sources = copy.deepcopy(self.sources)
        sources["catalog"]["openai"]["models"]["gpt-6.2-sol"]["limit"]["context"] = 2000000
        sources["pages"]["gpt-6.2-sol"] = sources["pages"]["gpt-6.2-sol"].replace("1,050,000 context", "2,000,000 context")
        updates = models.changes(self.root, sources)
        self.assertEqual(set(updates), {"tools/model-sources.json"})
        self.assertEqual(json.loads(updates["tools/model-sources.json"])["gpt-6.2-sol"]["provider_limit"]["context"], 2000000)

    def test_live_retired_pages_do_not_block_verified_replacement(self):
        root = mock.patch.object(models, "ROOT", self.root)
        root.start()
        self.addCleanup(root.stop)
        self.write(models.changes(self.root, self.sources))
        evidence = models.load_evidence(self.root)
        tiers = models.jsonc((self.root / "prompt-tiers.json").read_text())
        sources = copy.deepcopy(self.sources)
        sources["catalog"]["openai"]["models"]["gpt-6.3-sol"] = copy.deepcopy(sources["catalog"]["openai"]["models"]["gpt-6.2-sol"])
        sources["pages"]["gpt-6.3-sol"] = sources["pages"]["gpt-6.2-sol"]
        sources["docs"]["openai"] = "gpt-6.3-sol"
        seen = []

        def fetch(url, token=None, payload=None, *, direct=False):
            seen.append(url)
            if url == models.CATALOG:
                return json.dumps(sources["catalog"])
            if url.endswith("/models"):
                return '{"data": [{"id": "gpt-6.3-sol"}]}'
            if url == models.OPENAI + ".md":
                return sources["docs"]["openai"]
            if url == models.ANTHROPIC:
                return sources["docs"]["anthropic"]
            model = url.rsplit("/", 1)[1].removesuffix(".md")
            if model == "gpt-6.2-astra" and failure == "truncated chunk":
                sock = mock.Mock()
                sock.makefile.return_value = io.BytesIO(
                    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n10\r\nshort"
                )
                response = http.client.HTTPResponse(sock)
                response.begin()
                with mock.patch.object(models.urllib.request, "build_opener") as opener:
                    opener.return_value.open.return_value = response
                    return real_fetch(url)
            if model == "gpt-6.2-astra" and failure is not None:
                raise failure
            if model in ("gpt-6.2-sol", "gpt-6.2-astra"):
                error = urllib.error.HTTPError(url, 404, "retired", {}, None)
                self.addCleanup(error.close)
                raise error
            return sources["pages"][model]

        # An uncorroborated unrelated model must not block the bump.
        real_fetch = models.fetch
        for overview, failure in (("gpt-6.3-sol", None),
                                  ("gpt-6.3-sol gpt-6.2-astra", None),
                                  ("gpt-6.3-sol gpt-6.2-astra", TimeoutError("source timed out")),
                                  ("gpt-6.3-sol gpt-6.2-astra", ValueError("source exceeds 16 MiB")),
                                  ("gpt-6.3-sol gpt-6.2-astra", UnicodeDecodeError("utf-8", b"\xff", 0, 1, "invalid start byte")),
                                  ("gpt-6.3-sol gpt-6.2-astra", "truncated chunk"),
                                  ("gpt-6.3-sol gpt-6.2-astra", models.DiscoveryDeadline("overall deadline"))):
            sources["docs"]["openai"] = overview
            with self.subTest(overview=overview, failure=failure), mock.patch.object(models, "broker", return_value=("http://proxy.test/v1", "k", False)), mock.patch.object(models, "fetch", side_effect=fetch):
                if isinstance(failure, models.DiscoveryDeadline):
                    with self.assertRaises(models.DiscoveryDeadline):
                        models.live(tiers, evidence)
                    continue
                live = models.live(tiers, evidence)
                diagnostics = list(live.get("diagnostics", []))
                updates = models.changes(self.root, live, diagnostics)
                self.assertNotIn(models.OPENAI + "/gpt-6.2-sol.md", seen)
                self.assertEqual(models.jsonc(updates["prompt-tiers.json"])["pools"]["praxis"]["sol"]["model"], "praxis/gpt-6.3-sol")
                self.assertEqual(json.loads(updates[models.EVIDENCE])["gpt-6.2-astra"], evidence["gpt-6.2-astra"])
                self.assertTrue(any("gpt-6.2-astra" in d for d in diagnostics))

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
        with mock.patch.object(models, "ROOT", self.root), mock.patch.object(models, "broker", return_value=("http://proxy.test/v1", "k", False)):
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
                live = models.live(models.jsonc(path.read_text()), models.load_evidence(self.root))
                self.assertEqual(models.changes(self.root, live), updates)

    def test_live_cli_without_evidence_and_json_preview(self):
        with mock.patch.object(models, "ROOT", self.root), mock.patch.object(models, "live", return_value=self.sources) as live, mock.patch.object(sys, "argv", ["discover-models", "--json"]), mock.patch("sys.stdout", new_callable=io.StringIO) as out:
            self.assertEqual(models.main(), 0)
            self.assertEqual(live.call_args.args[1], {})
            preview = json.loads(out.getvalue())
            self.assertFalse(preview["applied"])
            self.assertTrue(any(p["old"] == "praxis/gpt-6.1-sol" and p["new"] == "praxis/gpt-6.2-sol" for p in preview["changes"]))
            self.assertTrue(any("gpt-6.2-sol" in p["path"] and "cost" in p["new"] for p in preview["changes"] if isinstance(p["new"], dict)))
        self.assertFalse((self.root / models.EVIDENCE).exists())
        (self.root / models.EVIDENCE).write_text("{")
        with self.assertRaises(ValueError):
            models.load_evidence(self.root)
        (self.root / models.EVIDENCE).write_text("[]")
        with self.assertRaises(ValueError):
            models.load_evidence(self.root)

    def test_text_preview_shows_same_id_old_and_new_prices(self):
        self.write(models.changes(self.root, self.sources))
        self.sources["catalog"]["openai"]["models"]["gpt-6.2-sol"]["cost"]["input"] = 3
        self.sources["pages"]["gpt-6.2-sol"] = self.sources["pages"]["gpt-6.2-sol"].replace("| Input | $2 |", "| Input | $3 |")
        with mock.patch.object(models, "ROOT", self.root), mock.patch.object(models, "live", return_value=self.sources), mock.patch.object(sys, "argv", ["discover-models"]), mock.patch("sys.stdout", new_callable=io.StringIO) as out:
            self.assertEqual(models.main(), 0)
            self.assertRegex(out.getvalue(), r"gpt-6.2-sol.*cost.*input.*2 -> 3")

    def test_cli_missing_credentials_is_actionable_and_does_not_fetch(self):
        with mock.patch.object(models, "ROOT", self.root), mock.patch.dict("os.environ", {"XDG_CONFIG_HOME": str(self.root)}, clear=True), mock.patch.object(models, "fetch") as fetch, mock.patch.object(sys, "argv", ["discover-models"]), mock.patch("sys.stderr", new_callable=io.StringIO) as err:
            self.assertEqual(models.main(), 1)
            fetch.assert_not_called()
            self.assertIn("BROKER_BASE_URL and BROKER_API_KEY", err.getvalue())

    def test_broker_credentials_from_environment_or_opencode_config(self):
        config = Path(self.temp.name) / "xdg"
        path = config / "opencode/opencode.json"
        path.parent.mkdir(parents=True)

        def provider(base, key):
            return json.dumps({"provider": {"praxis": {"options": {"baseURL": base, "apiKey": key}}}})

        url = "http://proxy.test/v1"
        for env, text, expected in (
            ({"BROKER_BASE_URL": url + "/", "BROKER_API_KEY": "k"}, None, (url, "k", False)),
            ({"BROKER_BASE_URL": url}, None, "set BROKER_API_KEY$"),
            ({}, provider(url, "k"), "deployed plain-HTTP baseURL must be a loopback"),
            ({}, provider("https://proxy.test/v1", "k"), ("https://proxy.test/v1", "k", True)),
            ({}, provider("http://127.0.0.1:8080/v1", "k"), ("http://127.0.0.1:8080/v1", "k", True)),
            ({}, provider("http://[::1]/v1", "k"), ("http://[::1]/v1", "k", True)),
            ({}, provider("http://localhost/v1", "k"), ("http://localhost/v1", "k", True)),
            ({}, provider(models.PLACEHOLDER_URL, "unused"), "not the placeholders"),
            ({}, provider("{env:X}", "k"), "BROKER_BASE_URL and BROKER_API_KEY"),
            ({}, provider(url, "{env:K}"), "BROKER_BASE_URL and BROKER_API_KEY"),
            ({}, provider(models.PLACEHOLDER_URL + "/", "k"), "not the placeholders"),
            ({}, provider("http://other.invalid/v1", "k"), "not the placeholders"),
            ({}, provider(42, "k"), "BROKER_BASE_URL and BROKER_API_KEY"),
            ({}, provider(url, 42), "BROKER_BASE_URL and BROKER_API_KEY"),
            ({}, '{"provider":{"praxis":{"options":null}}}', "BROKER_BASE_URL and BROKER_API_KEY"),
            ({}, None, "BROKER_BASE_URL and BROKER_API_KEY, or run where .*opencode.json"),
        ):
            with self.subTest(env=sorted(env), text=text):
                path.unlink(missing_ok=True)
                if text:
                    path.write_text(text)
                with mock.patch.dict("os.environ", {**env, "XDG_CONFIG_HOME": str(config)}, clear=True):
                    if isinstance(expected, tuple):
                        self.assertEqual(models.broker(), expected)
                    else:
                        with self.assertRaisesRegex(models.MissingCredentials, expected):
                            models.broker()

    def test_authenticated_redirect_is_rejected_before_following(self):
        seen = []

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                seen.append(self.path)
                self.send_response(302)
                self.send_header("Location", "/redirected")
                self.end_headers()

            def log_message(self, *args):
                pass

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with self.assertRaisesRegex(ValueError, "redirects are not allowed"):
                models.fetch(f"http://127.0.0.1:{server.server_port}/models", "fixture-token")
            self.assertEqual(seen, ["/models"])
        finally:
            server.shutdown()
            thread.join()
            server.server_close()

    def test_deployed_broker_bypasses_ambient_proxy(self):
        seen = []
        catalog = "http://public.test/catalog"
        openai = "http://public.test/openai"
        anthropic = "http://public.test/anthropic"
        public = {catalog: json.dumps(self.sources["catalog"]), openai + ".md": "", anthropic: ""}

        class Handler(BaseHTTPRequestHandler):
            def respond(self):
                seen.append((self.command, self.path, self.headers.get("Authorization")))
                # /models is unavailable, so discovery must probe /responses too.
                if self.path.endswith("/models"):
                    self.send_response(404)
                    self.end_headers()
                    return
                if self.command == "POST":
                    self.rfile.read(int(self.headers["Content-Length"]))
                self.send_response(200)
                self.end_headers()
                self.wfile.write(public.get(self.path, "").encode())

            do_GET = do_POST = respond

            def log_message(self, *args):
                pass

        # The loopback server doubles as a forward proxy. urllib sends it an
        # absolute URI in proxy mode, and only a path in direct mode.
        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            address = f"http://127.0.0.1:{server.server_port}"
            path = self.root / "opencode/opencode.json"
            config = models.jsonc(path.read_text())
            config["provider"]["praxis"]["options"] = {"baseURL": address + "/v1", "apiKey": "fixture-token"}
            path.write_text(json.dumps(config))
            tiers = models.jsonc((self.root / "prompt-tiers.json").read_text())
            env = {"XDG_CONFIG_HOME": str(self.root), "http_proxy": address}
            with mock.patch.dict("os.environ", env, clear=True), mock.patch.multiple(
                    models, ROOT=self.root, CATALOG=catalog, OPENAI=openai, ANTHROPIC=anthropic):
                models.live(tiers, {})
            self.assertEqual(seen, [("GET", url, None) for url in public] + [
                ("GET", "/v1/models", "Bearer fixture-token"),
                ("POST", "/v1/responses", "Bearer fixture-token"),
                ("POST", "/v1/responses", "Bearer fixture-token"),
            ])
        finally:
            server.shutdown()
            thread.join()
            server.server_close()

    def test_runtime_deadline_interrupts_blocking_work(self):
        with mock.patch.object(http.client.HTTPConnection, "request", side_effect=lambda *a, **kw: time.sleep(1)):
            with self.assertRaises(models.DiscoveryDeadline), models.deadline(0.02):
                models.fetch("http://127.0.0.1/models", direct=True)

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

        with mock.patch.dict("os.environ", {"BROKER_BASE_URL": "http://proxy.test/v1", "BROKER_API_KEY": "fixture-token"}), mock.patch.object(models, "fetch", side_effect=fetch):
            self.assertEqual(set(models.live(tiers, {})["broker"]["models"]), {"gpt-6.2-astra", "gpt-6.2-sol"})

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

                with mock.patch.dict("os.environ", {"BROKER_BASE_URL": "http://proxy.test/v1", "BROKER_API_KEY": "fixture-token"}), mock.patch.object(models, "fetch", side_effect=fetch):
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
