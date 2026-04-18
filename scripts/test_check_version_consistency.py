#!/usr/bin/env python3
"""Tests for `check-version-consistency.py`.

Rather than read the real repo (which would make the test self-referential
and brittle to every future version bump), each test assembles a synthetic
repo root in a tempdir with exactly the files the script looks for. This
isolates the check logic from the real state of the repository.

Tests cover:
  1. Happy path — all versions consistent, no allowlist needed
  2. Unallowlisted drift fails
  3. Allowlisted drift passes
  4. Wrong current_value in an allowlist entry fails
  5. Stale allowlist entry (no matching source) fails
  6. Allowlist with empty tracking_issue fails hard validation
  7. Allowlist with empty reason fails hard validation
  8. Crates disagreeing among themselves fails
  9. readme-smoke pins are detected correctly (regex integration smoke test)
"""

from __future__ import annotations

import importlib.util
import sys
import textwrap
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))

_spec = importlib.util.spec_from_file_location(
    "check_version_consistency", SCRIPTS_DIR / "check-version-consistency.py"
)
assert _spec is not None and _spec.loader is not None
check_version_consistency = importlib.util.module_from_spec(_spec)
sys.modules["check_version_consistency"] = check_version_consistency
_spec.loader.exec_module(check_version_consistency)


# ---------------------------------------------------------------- fixture builder


def _build_repo(
    root: Path,
    *,
    crate_versions: dict[str, str] | None = None,
    npm_version: str = "0.2.0",
    vscode_version: str = "0.2.0",
    napi_version: str = "0.2.0",
    tree_sitter_version: str = "0.2.0",
    smoke_npm_pin: str = "0.2.0",
    smoke_caret: str = "0.2",
    macports_version: str = "0.2.0",
    nix_version: str = "0.2.0",
    winget_version: str = "0.2.0",
) -> None:
    """Build a minimal repo layout under `root` that satisfies every extractor.

    Defaults produce an all-consistent repo at version 0.2.0. Override any
    kwarg to inject drift.
    """
    crate_versions = crate_versions or {
        "core": "0.2.0",
        "cli": "0.2.0",
    }

    # crates/*/Cargo.toml
    for name, version in crate_versions.items():
        d = root / "crates" / name
        d.mkdir(parents=True, exist_ok=True)
        (d / "Cargo.toml").write_text(
            textwrap.dedent(
                f"""
                [package]
                name = "chordsketch-{name}"
                version = "{version}"
                """
            ).strip()
            + "\n",
            encoding="utf-8",
        )

    # crates/napi/package.json — the check script explicitly looks at this
    napi_dir = root / "crates" / "napi"
    napi_dir.mkdir(parents=True, exist_ok=True)
    (napi_dir / "Cargo.toml").write_text(
        textwrap.dedent(
            f"""
            [package]
            name = "chordsketch-napi"
            version = "{crate_versions.get('napi', '0.2.0')}"
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )
    (napi_dir / "package.json").write_text(
        f'{{\n  "name": "@chordsketch/node",\n  "version": "{napi_version}"\n}}\n',
        encoding="utf-8",
    )

    # packages/npm/package.json
    npm_dir = root / "packages" / "npm"
    npm_dir.mkdir(parents=True, exist_ok=True)
    (npm_dir / "package.json").write_text(
        f'{{\n  "name": "@chordsketch/wasm",\n  "version": "{npm_version}"\n}}\n',
        encoding="utf-8",
    )

    # packages/tree-sitter-chordpro/package.json
    ts_dir = root / "packages" / "tree-sitter-chordpro"
    ts_dir.mkdir(parents=True, exist_ok=True)
    (ts_dir / "package.json").write_text(
        f'{{\n  "name": "tree-sitter-chordpro",\n  "version": "{tree_sitter_version}"\n}}\n',
        encoding="utf-8",
    )

    # packages/vscode-extension/package.json
    vscode_dir = root / "packages" / "vscode-extension"
    vscode_dir.mkdir(parents=True, exist_ok=True)
    (vscode_dir / "package.json").write_text(
        f'{{\n  "name": "chordsketch",\n  "version": "{vscode_version}"\n}}\n',
        encoding="utf-8",
    )

    # .github/workflows/readme-smoke.yml
    workflows_dir = root / ".github" / "workflows"
    workflows_dir.mkdir(parents=True, exist_ok=True)
    (workflows_dir / "readme-smoke.yml").write_text(
        textwrap.dedent(
            f"""
            # synthetic fixture
            jobs:
              wasm-smoke:
                steps:
                  - run: |
                      npm install '@chordsketch/wasm@{smoke_npm_pin}'
              library-smoke:
                steps:
                  - run: |
                      CORE_DEP='chordsketch-core = "^{smoke_caret}"'
                      RENDER_DEP='chordsketch-render-text = "^{smoke_caret}"'
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )

    # packaging/macports/Portfile
    macports_dir = root / "packaging" / "macports"
    macports_dir.mkdir(parents=True, exist_ok=True)
    (macports_dir / "Portfile").write_text(
        textwrap.dedent(
            f"""
            # Reference Portfile for MacPorts submission.
            PortSystem          1.0
            PortGroup           github 1.0
            github.setup        koedame chordsketch {macports_version} v
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )

    # packaging/nix/package.nix
    nix_dir = root / "packaging" / "nix"
    nix_dir.mkdir(parents=True, exist_ok=True)
    (nix_dir / "package.nix").write_text(
        textwrap.dedent(
            f"""
            {{ lib, rustPlatform, fetchFromGitHub }}:
            rustPlatform.buildRustPackage rec {{
              pname = "chordsketch";
              version = "{nix_version}";
            }}
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )

    # packaging/winget/*.yaml
    winget_dir = root / "packaging" / "winget"
    winget_dir.mkdir(parents=True, exist_ok=True)
    for manifest in (
        "koedame.chordsketch.yaml",
        "koedame.chordsketch.installer.yaml",
        "koedame.chordsketch.locale.en-US.yaml",
    ):
        (winget_dir / manifest).write_text(
            textwrap.dedent(
                f"""
                PackageIdentifier: koedame.chordsketch
                PackageVersion: {winget_version}
                """
            ).strip()
            + "\n",
            encoding="utf-8",
        )


def _write_allowlist(path: Path, entries: list[dict]) -> None:
    lines = []
    for e in entries:
        lines.append("[[allowed_skews]]")
        for k, v in e.items():
            if isinstance(v, str) and "\n" in v:
                lines.append(f'{k} = """\n{v}\n"""')
            else:
                lines.append(f'{k} = "{v}"')
        lines.append("")
    path.write_text("\n".join(lines), encoding="utf-8")


# ---------------------------------------------------------------- tests


class CheckRunTests(unittest.TestCase):
    def test_happy_path(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            _build_repo(root)
            rc = check_version_consistency.run(root, root / "nonexistent.toml")
            self.assertEqual(rc, 0)

    def test_unallowlisted_drift_fails(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            _build_repo(root, npm_version="0.1.5")
            rc = check_version_consistency.run(root, root / "nonexistent.toml")
            self.assertEqual(rc, 1)

    def test_tree_sitter_drift_detected(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            _build_repo(root, tree_sitter_version="0.1.0")
            rc = check_version_consistency.run(root, root / "nonexistent.toml")
            self.assertEqual(rc, 1)

    def test_allowlisted_drift_passes(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            _build_repo(root, npm_version="0.1.5")
            allowlist_path = root / "version-skew-allowlist.toml"
            _write_allowlist(
                allowlist_path,
                [
                    {
                        "file": "packages/npm/package.json",
                        "field": "version",
                        "current_value": "0.1.5",
                        "reason": "test fixture",
                        "expires_at": "never",
                        "tracking_issue": "9999",
                    }
                ],
            )
            rc = check_version_consistency.run(root, allowlist_path)
            self.assertEqual(rc, 0)

    def test_wrong_current_value_in_allowlist_fails(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            _build_repo(root, npm_version="0.1.5")
            allowlist_path = root / "version-skew-allowlist.toml"
            _write_allowlist(
                allowlist_path,
                [
                    {
                        "file": "packages/npm/package.json",
                        "field": "version",
                        "current_value": "0.1.4",  # wrong!
                        "reason": "stale",
                        "expires_at": "never",
                        "tracking_issue": "9999",
                    }
                ],
            )
            rc = check_version_consistency.run(root, allowlist_path)
            self.assertEqual(rc, 1)

    def test_wrong_current_value_is_not_reported_as_stale(self) -> None:
        """Regression test for #1513.

        When an allowlist entry has a wrong `current_value` but the source
        is still drifting, the entry is load-bearing (just inaccurate) —
        it must be reported as a drift but NOT also as a stale entry. The
        misleading "no matching source found — the drift has been
        resolved" message would have told maintainers to remove the
        entry (wrong action) when they should update `current_value`
        (right action).
        """
        from io import StringIO
        from contextlib import redirect_stdout

        with TemporaryDirectory() as td:
            root = Path(td)
            _build_repo(root, npm_version="0.1.5")
            allowlist_path = root / "version-skew-allowlist.toml"
            _write_allowlist(
                allowlist_path,
                [
                    {
                        "file": "packages/npm/package.json",
                        "field": "version",
                        "current_value": "0.1.4",  # wrong — source is 0.1.5
                        "reason": "test fixture",
                        "expires_at": "never",
                        "tracking_issue": "9999",
                    }
                ],
            )
            buf = StringIO()
            with redirect_stdout(buf):
                rc = check_version_consistency.run(root, allowlist_path)
            output = buf.getvalue()
            self.assertEqual(rc, 1)
            # The drift message must fire.
            self.assertIn("says current_value='0.1.4' but actual value is '0.1.5'", output)
            # The stale-entry message must NOT fire — the entry is still
            # load-bearing.
            self.assertNotIn("stale allowlist entry", output)
            self.assertNotIn("drift has been resolved", output)

    def test_stale_allowlist_entry_fails(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            _build_repo(root)  # all 0.2.0, no drift
            allowlist_path = root / "version-skew-allowlist.toml"
            _write_allowlist(
                allowlist_path,
                [
                    {
                        "file": "packages/npm/package.json",
                        "field": "version",
                        "current_value": "0.1.5",
                        "reason": "test",
                        "expires_at": "never",
                        "tracking_issue": "9999",
                    }
                ],
            )
            rc = check_version_consistency.run(root, allowlist_path)
            self.assertEqual(rc, 1)

    def test_missing_tracking_issue_fails_hard(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            _build_repo(root, npm_version="0.1.5")
            allowlist_path = root / "version-skew-allowlist.toml"
            _write_allowlist(
                allowlist_path,
                [
                    {
                        "file": "packages/npm/package.json",
                        "field": "version",
                        "current_value": "0.1.5",
                        "reason": "test",
                        "expires_at": "never",
                        "tracking_issue": "",  # empty!
                    }
                ],
            )
            with self.assertRaises(SystemExit) as ctx:
                check_version_consistency.run(root, allowlist_path)
            self.assertIn("tracking_issue", str(ctx.exception))

    def test_missing_reason_fails_hard(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            _build_repo(root, npm_version="0.1.5")
            allowlist_path = root / "version-skew-allowlist.toml"
            _write_allowlist(
                allowlist_path,
                [
                    {
                        "file": "packages/npm/package.json",
                        "field": "version",
                        "current_value": "0.1.5",
                        "reason": "",
                        "expires_at": "never",
                        "tracking_issue": "9999",
                    }
                ],
            )
            with self.assertRaises(SystemExit) as ctx:
                check_version_consistency.run(root, allowlist_path)
            self.assertIn("empty reason", str(ctx.exception))

    def test_crates_disagree_fails_hard(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            _build_repo(
                root,
                crate_versions={"core": "0.2.0", "cli": "0.1.9"},
            )
            with self.assertRaises(SystemExit) as ctx:
                check_version_consistency.run(root, root / "nonexistent.toml")
            self.assertIn("disagree", str(ctx.exception))

    def test_readme_smoke_pin_drift_detected(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            _build_repo(root, smoke_npm_pin="0.1.1", smoke_caret="0.1")
            rc = check_version_consistency.run(root, root / "nonexistent.toml")
            self.assertEqual(rc, 1)

    # -- packaging/<channel>/ drift detection (#1864) --------------------

    def test_macports_portfile_drift_detected(self) -> None:
        # Regression guard: #1864 observed that packaging/macports/Portfile
        # silently stayed at 0.2.0 across two releases. The extractor must
        # detect the drift instead of the human-audit-at-release path.
        with TemporaryDirectory() as td:
            root = Path(td)
            _build_repo(root, macports_version="0.1.0")
            rc = check_version_consistency.run(root, root / "nonexistent.toml")
            self.assertEqual(rc, 1)

    def test_nix_package_drift_detected(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            _build_repo(root, nix_version="0.1.0")
            rc = check_version_consistency.run(root, root / "nonexistent.toml")
            self.assertEqual(rc, 1)

    def test_winget_manifest_drift_detected(self) -> None:
        with TemporaryDirectory() as td:
            root = Path(td)
            _build_repo(root, winget_version="0.1.0")
            rc = check_version_consistency.run(root, root / "nonexistent.toml")
            self.assertEqual(rc, 1)

    def test_winget_manifest_without_package_version_fails_hard(self) -> None:
        # A winget yaml that omits `PackageVersion:` is a structural
        # defect (every manifest type requires it per the schema). The
        # check must fail hard so the author registers it, rather than
        # silently skipping the file and letting CI pass.
        with TemporaryDirectory() as td:
            root = Path(td)
            _build_repo(root)
            # Replace one manifest with one that lacks PackageVersion.
            (root / "packaging" / "winget" / "koedame.chordsketch.yaml").write_text(
                "PackageIdentifier: koedame.chordsketch\n",
                encoding="utf-8",
            )
            with self.assertRaises(SystemExit) as ctx:
                check_version_consistency.run(root, root / "nonexistent.toml")
            self.assertIn("PackageVersion", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
