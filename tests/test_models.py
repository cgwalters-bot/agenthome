"""Offline model discovery with recorded source excerpts; never contacts inference."""

import copy
import importlib.util
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

    def test_recorded_bump_and_noop(self):
        updates = models.changes(self.root, self.sources)
        self.assertEqual(set(updates), set(models.FILES) | {"tools/model-sources.json"})
        config = models.jsonc(updates["opencode/opencode.json"])
        astra = config["provider"]["praxis"]["models"]["gpt-6.2-astra"]
        self.assertEqual(astra["cost"], {"input": 8, "output": 40, "cache_read": 0.4, "cache_write": 8})
        self.assertEqual(astra["limit"], {"context": 200000, "input": 200000, "output": 128000})
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
        for name, text in models.changes(self.root, self.sources).items():
            (self.root / name).write_text(text)
        sources = copy.deepcopy(self.sources)
        sources["catalog"]["openai"]["models"]["gpt-6.3-sol"] = sources["catalog"]["openai"]["models"].pop("gpt-6.2-sol")
        sources["broker"]["models"] = ["gpt-6.3-sol"]
        sources["docs"]["openai"] = "gpt-6.3-sol"
        sources["pages"]["gpt-6.3-sol"] = sources["pages"]["gpt-6.2-sol"].replace("gpt-6.2-sol", "gpt-6.3-sol")
        updates = models.changes(self.root, sources)
        self.assertEqual(models.jsonc(updates["prompt-tiers.json"])["pools"]["praxis"]["sol"]["model"], "praxis/gpt-6.3-sol")
        for name, text in updates.items():
            (self.root / name).write_text(text)
        self.assertEqual(models.changes(self.root, sources), {})

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

    def test_runtime_deadline_interrupts_blocking_work(self):
        with self.assertRaises(TimeoutError):
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

        def fetch(url, token=None, payload=None):
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

        with mock.patch.dict("os.environ", {"BROKER_BASE_URL": "http://inference.invalid/v1", "BROKER_API_KEY": "unused"}), mock.patch.object(models, "fetch", side_effect=fetch):
            self.assertEqual(set(models.live(tiers)["broker"]["models"]), {"gpt-6.2-astra", "gpt-6.2-sol"})

    def test_listing_does_not_probe_and_auth_errors_fail(self):
        tiers = models.jsonc((self.root / "prompt-tiers.json").read_text())
        for code in (None, 401):
            with self.subTest(code=code):
                def fetch(url, token=None, payload=None):
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

                with mock.patch.dict("os.environ", {"BROKER_BASE_URL": "http://inference.invalid/v1", "BROKER_API_KEY": "unused"}), mock.patch.object(models, "fetch", side_effect=fetch):
                    if code:
                        with self.assertRaises(urllib.error.HTTPError):
                            models.live(tiers)
                    else:
                        self.assertEqual(models.live(tiers)["broker"]["models"], [])

    def test_missing_overview_or_limit_disagreement_fails(self):
        for mutate in (lambda s: s["docs"].update(openai=""),
                       lambda s: s["catalog"]["openai"]["models"]["gpt-6.2-sol"]["limit"].update(output=42)):
            sources = copy.deepcopy(self.sources)
            mutate(sources)
            with self.assertRaises(ValueError):
                models.changes(self.root, sources)


if __name__ == "__main__":
    unittest.main()
