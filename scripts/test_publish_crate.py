"""Offline release regression tests: never run real Cargo or contact a registry."""

import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from urllib.error import HTTPError, URLError

import publish_crate as publisher


class PublishTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.manifest = Path(self.temp.name, "Cargo.toml")
        # The directory name deliberately differs from the package name.
        self.manifest.write_text('[package]\nname = "fframes_three_d"\nversion = "1.4.1"\n')
        self.cargo = self.enterContext(patch.object(publisher.subprocess, "run"))
        self.http = self.enterContext(patch.object(publisher, "urlopen"))
        self.sleep = self.enterContext(patch.object(publisher.time, "sleep"))
        self.enterContext(patch("builtins.print"))

    def response(self, **overrides):
        info = {"crate": "fframes_three_d", "num": "1.4.1", "yanked": False, **overrides}
        return io.BytesIO(json.dumps({"version": info}).encode())

    def error(self, status, headers=None):
        return HTTPError("https://crates.io", status, "test", headers or {}, None)

    def run_publish(self):
        publisher.publish(self.manifest, "1.4.1")

    def test_existing_version_is_skipped_using_manifest_name(self):
        self.http.return_value = self.response()
        self.run_publish()
        self.cargo.assert_not_called()
        self.assertTrue(self.http.call_args.args[0].full_url.endswith('/fframes_three_d/1.4.1'))

    def test_missing_version_is_published_to_crates_io(self):
        self.http.side_effect = self.error(404)
        self.cargo.return_value = subprocess.CompletedProcess([], 0, "published\n")
        self.run_publish()
        self.assertEqual(self.cargo.call_args.args[0], [
            "cargo", "publish", "--registry", "crates-io", "--allow-dirty", "--no-verify"
        ])
        self.assertEqual(self.cargo.call_args.kwargs['cwd'], self.manifest.parent)
        self.assertEqual(self.cargo.call_args.kwargs['timeout'], 600)

    def test_timeout_after_successful_upload(self):
        self.http.side_effect = [self.error(404), self.response()]
        self.cargo.side_effect = subprocess.TimeoutExpired(["cargo"], 600, output=b"Uploaded\n")
        self.run_publish()
        self.cargo.assert_called_once()
        self.assertEqual(self.http.call_count, 2)
        self.sleep.assert_not_called()

    def test_timeout_with_missing_version_fails_without_retry(self):
        self.http.side_effect = self.error(404)
        self.cargo.side_effect = subprocess.TimeoutExpired(["cargo"], 600)
        with self.assertRaisesRegex(RuntimeError, "timed out after 600s.*fframes_three_d@1.4.1"):
            self.run_publish()
        self.cargo.assert_called_once()
        self.assertEqual(self.http.call_count, 2)
        self.sleep.assert_not_called()

    def test_timeout_registry_failure_is_not_success(self):
        self.http.side_effect = [self.error(404)] + [self.error(503)] * 5
        self.cargo.side_effect = subprocess.TimeoutExpired(["cargo"], 600)
        with self.assertRaises(HTTPError):
            self.run_publish()
        self.cargo.assert_called_once()
        self.assertEqual(self.http.call_count, 6)

    def test_timeout_preserves_partial_output(self):
        for output, expected in [(b"partial\xff", "partial\ufffd"), ("partial", "partial"), (None, "")]:
            with self.subTest(output=output), patch("builtins.print") as printed:
                self.http.side_effect = self.error(404)
                self.cargo.side_effect = subprocess.TimeoutExpired(["cargo"], 600, output=output)
                with self.assertRaisesRegex(RuntimeError, "timed out"):
                    self.run_publish()
                printed.assert_any_call(expected, end="", flush=True)

    def test_partial_release_skips_five_then_retries_sixth(self):
        root = Path(__file__).resolve().parent.parent
        script = (root / "scripts/publish.sh").read_text()
        # Exercise the real dependency order and manifests, including underscores
        # and package names that differ from their directory names.
        directories = script.split("for crate in ", 1)[1].split("; do", 1)[0]
        directories = directories.replace("\\", "").split()
        expected_names = ["webvtt-parser", "svgr-macro", "fframes-media",
                          "fframes-media-dir-macro", "fframes_three_d", "fframes",
                          "fframes-editor-controller", "fframes_skia_renderer",
                          "fframes_native_player", "fframes-dylib", "cargo-fframes"]
        with patch.object(publisher, "is_published",
                          side_effect=[True] * 5 + [False] * 8) as published:
            self.cargo.side_effect = [
                subprocess.CompletedProcess([], 101, "error (status 429 Too Many Requests)"),
            ] + [subprocess.CompletedProcess([], 0, "published\n")] * 6
            for directory in directories:
                publisher.publish(root / directory / "Cargo.toml", "1.4.1")
        names = [call.args[0] for call in published.call_args_list]
        self.assertEqual(names, expected_names[:6] + ["fframes"] * 2 + expected_names[6:])
        self.assertEqual(self.cargo.call_count, 7)
        self.sleep.assert_called_once_with(60)

    def test_retry_budget_is_finite(self):
        self.http.side_effect = self.error(404)
        self.cargo.return_value = subprocess.CompletedProcess([], 101, "status 429 Too Many Requests")
        with self.assertRaisesRegex(RuntimeError, "retries exhausted"):
            self.run_publish()
        self.assertEqual(self.cargo.call_count, 5)
        self.assertEqual([c.args[0] for c in self.sleep.call_args_list], [60, 120, 240, 480])

    def test_permanent_cargo_error_is_not_retried(self):
        self.http.side_effect = self.error(404)
        self.cargo.return_value = subprocess.CompletedProcess([], 101, "status 403 Forbidden")
        with self.assertRaisesRegex(RuntimeError, "cargo publish failed"):
            self.run_publish()
        self.cargo.assert_called_once()
        self.sleep.assert_not_called()

    def test_upload_succeeded_but_cargo_failed(self):
        self.http.side_effect = [self.error(404), self.response()]
        self.cargo.return_value = subprocess.CompletedProcess([], 101, "already uploaded")
        self.run_publish()
        self.cargo.assert_called_once()
        self.sleep.assert_not_called()

    def test_version_mismatch_fails_before_network_or_upload(self):
        with self.assertRaisesRegex(RuntimeError, "manifest version"):
            publisher.publish(self.manifest, "1.4.2")
        self.http.assert_not_called()
        self.cargo.assert_not_called()

    def test_yanked_version_fails_closed(self):
        self.http.return_value = self.response(yanked=True)
        with self.assertRaisesRegex(RuntimeError, "yanked"):
            self.run_publish()
        self.cargo.assert_not_called()

    def test_wrong_registry_version_fails_closed(self):
        self.http.return_value = self.response(num="1.4.0")
        with self.assertRaisesRegex(RuntimeError, "Unexpected registry response"):
            self.run_publish()
        self.cargo.assert_not_called()

    def test_malformed_registry_response_fails_closed(self):
        self.http.return_value = io.BytesIO(b"not json")
        with self.assertRaises(ValueError):
            self.run_publish()
        self.cargo.assert_not_called()

    def test_registry_permission_error_is_not_absence(self):
        self.http.side_effect = self.error(403)
        with self.assertRaises(HTTPError):
            self.run_publish()
        self.cargo.assert_not_called()
        self.sleep.assert_not_called()

    def test_registry_rate_limit_respects_retry_after(self):
        self.http.side_effect = [self.error(429, {"Retry-After": "180"}), self.response()]
        self.run_publish()
        self.sleep.assert_called_once_with(181)
        self.cargo.assert_not_called()

    def test_registry_transient_failures_are_bounded(self):
        for error in (self.error(503), URLError("offline"), TimeoutError()):
            with self.subTest(error=error):
                self.http.reset_mock()
                self.http.side_effect = error
                with self.assertRaises(type(error)):
                    self.run_publish()
                self.assertEqual(self.http.call_count, 5)
        self.cargo.assert_not_called()

    def test_cargo_retry_date(self):
        self.http.side_effect = self.error(404)
        self.cargo.side_effect = [
            subprocess.CompletedProcess([], 101,
                "status 429 Too Many Requests: Please try again after Mon, 02 Nov 2026 03:01:45 GMT and see docs"),
            subprocess.CompletedProcess([], 0, ""),
        ]
        with patch.object(publisher.time, "time", return_value=1793588205):
            self.run_publish()
        self.sleep.assert_called_once_with(301)

    def test_retry_after_formats_and_upper_bound(self):
        self.assertEqual(publisher.retry_delay(0, "nonsense"), 60)
        self.assertEqual(publisher.retry_delay(0, "-1"), 60)
        with self.assertRaisesRegex(RuntimeError, "one hour"):
            publisher.retry_delay(0, "7200")


if __name__ == "__main__":
    unittest.main()
