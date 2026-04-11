#!/usr/bin/env python3
"""Tests for `_release_channels.load_channels` and
`check-release-channels.verify_channel`.

Uses only `unittest` and `unittest.mock` from stdlib so CI does not need a
`pip install` step to run. Live registry lookups are NOT exercised here —
those are best tested by running the real workflow against a real release
tag. This file covers the behaviors most likely to break under refactoring:

  1. Manifest validation (missing field, duplicate id, skip without reason,
     unknown kind)
  2. `verify_channel` on a skip channel returns OK without touching the
     registry
  3. `verify_channel` with `force_stale=True` returns a synthetic red
     result with the expected diagnostic string
  4. Per-kind dispatch mocks: one test per registry kind that asserts the
     HTTP URL the checker requests and the tag/observed comparison logic,
     so a silent refactor of URL construction cannot go unnoticed.
"""

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))

from _release_channels import Channel, ManifestError, load_channels  # noqa: E402

# Load the hyphenated script as a module so tests can import its symbols.
# The module must be registered in sys.modules BEFORE exec_module so that
# dataclasses' type introspection can find it (otherwise @dataclass fails
# with "NoneType has no attribute __dict__" during class definition).
_spec = importlib.util.spec_from_file_location(
    "check_release_channels", SCRIPTS_DIR / "check-release-channels.py"
)
assert _spec is not None and _spec.loader is not None
check_release_channels = importlib.util.module_from_spec(_spec)
sys.modules["check_release_channels"] = check_release_channels
_spec.loader.exec_module(check_release_channels)


# ---------------------------------------------------------------- helpers


def _write_manifest(tmp: Path, body: str) -> Path:
    path = tmp / "release-channels.toml"
    path.write_text(body, encoding="utf-8")
    return path


def _fake_channel(
    *,
    channel_id: str = "fake-channel",
    kind: str = "crates-io",
    package: str = "fake-pkg",
    expected_version: str = "tag",
    skip_reason: str = "",
) -> Channel:
    return Channel(
        id=channel_id,
        display=channel_id,
        kind=kind,
        package=package,
        expected_version=expected_version,
        required_secrets=(),
        skip_reason=skip_reason,
        notes="",
    )


# ---------------------------------------------------------------- manifest validation


class ManifestValidationTests(unittest.TestCase):
    def test_happy_path(self) -> None:
        """The real manifest in ci/release-channels.toml MUST parse."""
        channels = load_channels()
        self.assertGreater(len(channels), 0, "manifest should have at least one channel")
        # Every channel id must be unique (load_channels enforces this, but
        # we assert it here too as a regression guard).
        ids = [c.id for c in channels]
        self.assertEqual(len(ids), len(set(ids)), "channel ids must be unique")

    def test_missing_required_field(self) -> None:
        from tempfile import TemporaryDirectory

        with TemporaryDirectory() as td:
            path = _write_manifest(
                Path(td),
                '[[channels]]\nid = "x"\ndisplay = "X"\nkind = "crates-io"\n',
            )
            with self.assertRaises(ManifestError) as ctx:
                load_channels(path)
            self.assertIn("expected_version", str(ctx.exception))

    def test_duplicate_id(self) -> None:
        from tempfile import TemporaryDirectory

        with TemporaryDirectory() as td:
            path = _write_manifest(
                Path(td),
                """
[[channels]]
id = "dup"
display = "A"
kind = "crates-io"
package = "a"
expected_version = "tag"

[[channels]]
id = "dup"
display = "B"
kind = "crates-io"
package = "b"
expected_version = "tag"
""",
            )
            with self.assertRaises(ManifestError) as ctx:
                load_channels(path)
            self.assertIn("duplicate", str(ctx.exception))

    def test_unknown_kind(self) -> None:
        from tempfile import TemporaryDirectory

        with TemporaryDirectory() as td:
            path = _write_manifest(
                Path(td),
                """
[[channels]]
id = "x"
display = "X"
kind = "nonexistent-kind"
package = "p"
expected_version = "tag"
""",
            )
            with self.assertRaises(ManifestError) as ctx:
                load_channels(path)
            self.assertIn("unknown kind", str(ctx.exception))

    def test_skip_without_reason(self) -> None:
        from tempfile import TemporaryDirectory

        with TemporaryDirectory() as td:
            path = _write_manifest(
                Path(td),
                """
[[channels]]
id = "x"
display = "X"
kind = "manual"
expected_version = "skip"
""",
            )
            with self.assertRaises(ManifestError) as ctx:
                load_channels(path)
            self.assertIn("skip_reason", str(ctx.exception))


# ---------------------------------------------------------------- verify_channel


class VerifyChannelTests(unittest.TestCase):
    def test_skip_channel_is_ok_without_http(self) -> None:
        channel = _fake_channel(
            kind="manual", expected_version="skip", skip_reason="manual PR", package=""
        )
        # Intercept any real HTTP to prove the path didn't hit the network.
        with patch("check_release_channels._http_get_json") as mock_http:
            result = check_release_channels.verify_channel(channel, "v0.2.0", force_stale=False)
        self.assertTrue(result.ok)
        self.assertEqual(result.observed, "<manual>")
        mock_http.assert_not_called()

    def test_force_stale_returns_red(self) -> None:
        channel = _fake_channel()
        result = check_release_channels.verify_channel(channel, "v0.2.0", force_stale=True)
        self.assertFalse(result.ok)
        self.assertEqual(result.observed, "<forced-stale>")
        self.assertEqual(result.expected, "0.2.0")
        self.assertIn("synthetic failure", result.detail)

    def test_crates_io_version_match(self) -> None:
        channel = _fake_channel(kind="crates-io", package="chordsketch")
        with patch(
            "check_release_channels._http_get_json",
            return_value={"crate": {"max_version": "0.2.0"}},
        ) as mock_http:
            result = check_release_channels.verify_channel(channel, "v0.2.0", force_stale=False)
        self.assertTrue(result.ok, f"expected OK, got {result}")
        self.assertEqual(result.observed, "0.2.0")
        called_url = mock_http.call_args.args[0]
        self.assertEqual(called_url, "https://crates.io/api/v1/crates/chordsketch")

    def test_crates_io_version_mismatch(self) -> None:
        channel = _fake_channel(kind="crates-io", package="chordsketch")
        with patch(
            "check_release_channels._http_get_json",
            return_value={"crate": {"max_version": "0.1.9"}},
        ):
            result = check_release_channels.verify_channel(channel, "v0.2.0", force_stale=False)
        self.assertFalse(result.ok)
        self.assertEqual(result.observed, "0.1.9")
        self.assertIn("version mismatch", result.detail)

    def test_npm_scoped_package_url_encoding(self) -> None:
        channel = _fake_channel(kind="npm", package="@chordsketch/wasm")
        with patch(
            "check_release_channels._http_get_json",
            return_value={"version": "0.2.0"},
        ) as mock_http:
            check_release_channels.verify_channel(channel, "v0.2.0", force_stale=False)
        called_url = mock_http.call_args.args[0]
        # The scoped slash MUST be percent-encoded for the registry URL, or
        # npm returns a 404 for @chordsketch/wasm while happily serving
        # %40chordsketch%2Fwasm. This has bitten projects before.
        self.assertIn("%40chordsketch%2Fwasm", called_url)

    def test_ghcr_head_ok_path(self) -> None:
        channel = _fake_channel(kind="ghcr", package="koedame/chordsketch")
        with patch(
            "check_release_channels._http_head_ok",
            return_value=True,
        ) as mock_head:
            result = check_release_channels.verify_channel(channel, "v0.2.0", force_stale=False)
        self.assertTrue(result.ok)
        # The URL must include the v-prefixed tag, matching the Docker
        # Registry v2 manifest path contract.
        called_url = mock_head.call_args.args[0]
        self.assertEqual(called_url, "https://ghcr.io/v2/koedame/chordsketch/manifests/v0.2.0")

    def test_ghcr_head_fail_path(self) -> None:
        channel = _fake_channel(kind="ghcr", package="koedame/chordsketch")
        with patch("check_release_channels._http_head_ok", return_value=False):
            result = check_release_channels.verify_channel(channel, "v0.2.0", force_stale=False)
        self.assertFalse(result.ok)
        self.assertIn("not publicly reachable", result.detail)

    def test_pypi_mismatch(self) -> None:
        channel = _fake_channel(kind="pypi", package="chordsketch")
        with patch(
            "check_release_channels._http_get_json",
            return_value={"info": {"version": "0.1.0"}},
        ):
            result = check_release_channels.verify_channel(channel, "v0.2.0", force_stale=False)
        self.assertFalse(result.ok)
        self.assertEqual(result.observed, "0.1.0")

    def test_maven_invalid_package_format(self) -> None:
        channel = _fake_channel(kind="maven-central", package="no-colon")
        result = check_release_channels.verify_channel(channel, "v0.2.0", force_stale=False)
        self.assertFalse(result.ok)
        self.assertIn("group:artifact", result.detail)

    # ----------------------------------------------------------------
    # Per-kind mocked tests for the 6 checkers that previously had no
    # unit coverage. Regression guard for silent URL-construction or
    # response-parsing breaks. See #1516.
    # ----------------------------------------------------------------

    def test_docker_hub_match_no_double_v_prefix(self) -> None:
        """Regression test for #1512: Docker Hub returns `name: v0.2.0` (with
        the `v` already), so `observed` must not add another `v`."""
        channel = _fake_channel(kind="docker-hub", package="koedame/chordsketch")
        with patch(
            "check_release_channels._http_get_json",
            return_value={"name": "v0.2.0"},
        ) as mock_http:
            result = check_release_channels.verify_channel(channel, "v0.2.0", force_stale=False)
        self.assertTrue(result.ok, f"expected OK, got {result}")
        self.assertEqual(result.observed, "v0.2.0")  # NOT "vv0.2.0"
        self.assertEqual(
            mock_http.call_args.args[0],
            "https://hub.docker.com/v2/repositories/koedame/chordsketch/tags/v0.2.0/",
        )

    def test_docker_hub_mismatch(self) -> None:
        channel = _fake_channel(kind="docker-hub", package="koedame/chordsketch")
        with patch(
            "check_release_channels._http_get_json",
            return_value={"name": "v0.1.9"},
        ):
            result = check_release_channels.verify_channel(channel, "v0.2.0", force_stale=False)
        self.assertFalse(result.ok)
        self.assertIn("tag mismatch", result.detail)

    def test_vscode_marketplace_match(self) -> None:
        channel = _fake_channel(
            kind="vscode-marketplace", package="koedame.chordsketch"
        )
        fake_payload = {
            "results": [
                {
                    "extensions": [
                        {
                            "versions": [
                                {"version": "0.2.0"},
                                {"version": "0.1.0"},
                            ]
                        }
                    ]
                }
            ]
        }

        # The Marketplace checker uses urlopen directly (POST), not
        # _http_get_json, so patch the module-level symbol.
        class _FakeResponse:
            def __init__(self, body):
                self._body = body

            def __enter__(self):
                return self

            def __exit__(self, *exc):
                return False

            def read(self):
                import json as _json

                return _json.dumps(self._body).encode("utf-8")

        with patch(
            "check_release_channels.urllib.request.urlopen",
            return_value=_FakeResponse(fake_payload),
        ):
            result = check_release_channels.verify_channel(
                channel, "v0.2.0", force_stale=False
            )
        self.assertTrue(result.ok, f"expected OK, got {result}")
        self.assertEqual(result.observed, "0.2.0")

    def test_vscode_marketplace_no_results(self) -> None:
        channel = _fake_channel(
            kind="vscode-marketplace", package="koedame.chordsketch"
        )

        class _FakeResponse:
            def __enter__(self):
                return self

            def __exit__(self, *exc):
                return False

            def read(self):
                return b'{"results": []}'

        with patch(
            "check_release_channels.urllib.request.urlopen",
            return_value=_FakeResponse(),
        ):
            result = check_release_channels.verify_channel(
                channel, "v0.2.0", force_stale=False
            )
        self.assertFalse(result.ok)
        self.assertIn("no results", result.detail)

    def test_homebrew_tap_match(self) -> None:
        channel = _fake_channel(kind="homebrew-tap", package="chordsketch")
        formula = """class Chordsketch < Formula
  desc "ChordPro tool"
  homepage "https://example.com"
  version "0.2.0"
  sha256 "abc"
end
"""
        with patch(
            "check_release_channels._http_get_text",
            return_value=formula,
        ) as mock_http:
            result = check_release_channels.verify_channel(
                channel, "v0.2.0", force_stale=False
            )
        self.assertTrue(result.ok, f"expected OK, got {result}")
        self.assertEqual(result.observed, "0.2.0")
        self.assertEqual(
            mock_http.call_args.args[0],
            "https://raw.githubusercontent.com/koedame/homebrew-tap/main/Formula/chordsketch.rb",
        )

    def test_homebrew_tap_no_version_line(self) -> None:
        channel = _fake_channel(kind="homebrew-tap", package="chordsketch")
        with patch(
            "check_release_channels._http_get_text",
            return_value="class Chordsketch < Formula\nend\n",
        ):
            result = check_release_channels.verify_channel(
                channel, "v0.2.0", force_stale=False
            )
        self.assertFalse(result.ok)
        self.assertIn("no version line", result.detail)

    def test_scoop_bucket_match(self) -> None:
        channel = _fake_channel(kind="scoop-bucket", package="chordsketch")
        manifest = '{"version": "0.2.0", "architecture": {}}'
        with patch(
            "check_release_channels._http_get_text",
            return_value=manifest,
        ) as mock_http:
            result = check_release_channels.verify_channel(
                channel, "v0.2.0", force_stale=False
            )
        self.assertTrue(result.ok, f"expected OK, got {result}")
        self.assertEqual(result.observed, "0.2.0")
        self.assertEqual(
            mock_http.call_args.args[0],
            "https://raw.githubusercontent.com/koedame/scoop-bucket/main/bucket/chordsketch.json",
        )

    def test_rubygems_match(self) -> None:
        channel = _fake_channel(kind="rubygems", package="chordsketch")
        with patch(
            "check_release_channels._http_get_json",
            return_value={"version": "0.2.0"},
        ) as mock_http:
            result = check_release_channels.verify_channel(
                channel, "v0.2.0", force_stale=False
            )
        self.assertTrue(result.ok, f"expected OK, got {result}")
        self.assertEqual(result.observed, "0.2.0")
        self.assertEqual(
            mock_http.call_args.args[0],
            "https://rubygems.org/api/v1/versions/chordsketch/latest.json",
        )

    def test_maven_central_match(self) -> None:
        channel = _fake_channel(
            kind="maven-central", package="io.github.koedame:chordsketch"
        )
        with patch(
            "check_release_channels._http_get_json",
            return_value={
                "response": {
                    "docs": [
                        {"latestVersion": "0.2.0"},
                    ]
                }
            },
        ) as mock_http:
            result = check_release_channels.verify_channel(
                channel, "v0.2.0", force_stale=False
            )
        self.assertTrue(result.ok, f"expected OK, got {result}")
        self.assertEqual(result.observed, "0.2.0")
        called_url = mock_http.call_args.args[0]
        # The solrsearch query must URL-encode the AND+group+artifact
        # expression correctly so ":" and spaces round-trip through
        # maven's search index.
        self.assertIn(
            "q=g%3Aio.github.koedame%20AND%20a%3Achordsketch",
            called_url,
        )

    def test_maven_central_not_found(self) -> None:
        channel = _fake_channel(
            kind="maven-central", package="io.github.koedame:chordsketch"
        )
        with patch(
            "check_release_channels._http_get_json",
            return_value={"response": {"docs": []}},
        ):
            result = check_release_channels.verify_channel(
                channel, "v0.2.0", force_stale=False
            )
        self.assertFalse(result.ok)
        self.assertIn("not found on Maven Central", result.detail)


if __name__ == "__main__":
    unittest.main()
