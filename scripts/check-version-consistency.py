#!/usr/bin/env python3
"""Version consistency CI check.

Enforces that every versioned manifest across the repo stays in lockstep with
the workspace crate version, unless the drift is explicitly declared in
`ci/version-skew-allowlist.toml`. Runs on every PR via `.github/workflows/ci.yml`
and fails loud on any unallowed drift.

The canonical version is the mode (most common value) across
`crates/*/Cargo.toml` — if even the crates disagree among themselves, that is
a structural bug and the script fails with a dedicated error.

Sources checked:

  1. Every crate's `package.version` in `crates/*/Cargo.toml`
  2. `packages/npm/package.json` `version`
  3. `packages/vscode-extension/package.json` `version`
  4. `crates/napi/package.json` `version`
  4b. `packages/tree-sitter-chordpro/package.json` `version`
  5. `.github/workflows/readme-smoke.yml` — the two hardcoded pins:
       a. L~204: `npm install '@chordsketch/wasm@<version>'`
       b. L~450–451: `chordsketch-core = "<caret>"` and
          `chordsketch-render-text = "<caret>"` (matched by both)

Each source is a (file, field, current_value) triple. The allowlist file has
the same (file, field, current_value) shape plus a mandatory `tracking_issue`
field — an entry without `tracking_issue` fails validation, because skips
must never be forgotten (see #1506 and the user's explicit requirement).

Exit codes:
  0 — every source matches canonical (after allowlist suppression)
  1 — one or more unallowed drifts detected, OR the allowlist is structurally
      invalid, OR a stale allowlist entry does not correspond to any source

stdlib only — no external deps.
"""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ALLOWLIST_PATH = REPO_ROOT / "ci" / "version-skew-allowlist.toml"


@dataclass(frozen=True)
class Source:
    """One version-bearing location in the repo."""

    file: str  # repo-relative path
    field: str  # human-readable label within the file
    value: str  # literal string extracted from the file


@dataclass(frozen=True)
class AllowlistEntry:
    file: str
    field: str
    current_value: str
    reason: str
    expires_at: str
    tracking_issue: str  # non-empty; stored as string for forward compat


@dataclass(frozen=True)
class Drift:
    source: Source
    canonical: str
    detail: str


# ---------------------------------------------------------------- extract sources


def load_crate_versions(repo_root: Path) -> list[Source]:
    """Collect `package.version` from every `crates/*/Cargo.toml`."""
    sources: list[Source] = []
    for cargo_toml in sorted((repo_root / "crates").glob("*/Cargo.toml")):
        try:
            data = tomllib.loads(cargo_toml.read_text(encoding="utf-8"))
        except tomllib.TOMLDecodeError as exc:
            raise SystemExit(f"{cargo_toml}: invalid TOML: {exc}")
        version = data.get("package", {}).get("version")
        if not isinstance(version, str):
            raise SystemExit(f"{cargo_toml}: package.version is missing or not a string")
        rel = cargo_toml.relative_to(repo_root).as_posix()
        sources.append(Source(file=rel, field="package.version", value=version))
    if not sources:
        raise SystemExit("no crates/*/Cargo.toml found — run from the repo root")
    return sources


def load_package_json_version(repo_root: Path, relative: str) -> Source:
    """Naive but deterministic JSON `version` extractor.

    Uses a literal regex rather than `json.load` so the check survives a
    temporarily-malformed package.json (e.g. during a merge conflict) and
    reports the actual field, not a parse error. The regex matches the first
    top-level `"version": "<value>"` line, which is the convention used by
    every package.json in this repo.
    """
    path = repo_root / relative
    if not path.is_file():
        raise SystemExit(f"{relative}: file not found")
    text = path.read_text(encoding="utf-8")
    match = re.search(r'"version"\s*:\s*"([^"]+)"', text)
    if match is None:
        raise SystemExit(f"{relative}: no version field found")
    return Source(file=relative, field="version", value=match.group(1))


# Line 204 smoke-test pin: `npm install '@chordsketch/wasm@<version>'`
_SMOKE_NPM_PIN_RE = re.compile(
    r"""npm\s+install\s+['"]@chordsketch/wasm@([0-9][^'"]*)['"]"""
)
# Line ~450 smoke-test caret constraint (matches the chordsketch-core entry,
# which is representative of the library-smoke mode's paired pins).
_SMOKE_CARET_RE = re.compile(
    r"""chordsketch-core\s*=\s*['"]\^([0-9]+\.[0-9]+)['"]"""
)


def load_readme_smoke_pins(repo_root: Path) -> list[Source]:
    """Extract the two hardcoded pins from `.github/workflows/readme-smoke.yml`.

    These pins cannot be in `allowlist` form because they live inside shell
    script bodies inside YAML; regex is the pragmatic extractor. If the YAML
    is restructured and either regex stops matching, this function raises
    `SystemExit` with a clear message so the maintainer knows to update it.
    """
    relative = ".github/workflows/readme-smoke.yml"
    path = repo_root / relative
    if not path.is_file():
        raise SystemExit(f"{relative}: file not found")
    text = path.read_text(encoding="utf-8")

    sources: list[Source] = []

    npm_match = _SMOKE_NPM_PIN_RE.search(text)
    if npm_match is None:
        raise SystemExit(
            f"{relative}: could not find `npm install @chordsketch/wasm@<version>`. "
            f"If the smoke job was restructured, update _SMOKE_NPM_PIN_RE in this script."
        )
    sources.append(
        Source(
            file=relative,
            field="npm install @chordsketch/wasm pin",
            value=npm_match.group(1),
        )
    )

    caret_match = _SMOKE_CARET_RE.search(text)
    if caret_match is None:
        raise SystemExit(
            f"{relative}: could not find `chordsketch-core = \"^X.Y\"`. "
            f"If the library-smoke job was restructured, update _SMOKE_CARET_RE."
        )
    sources.append(
        Source(
            file=relative,
            field="library-smoke caret constraint (^major.minor)",
            value=caret_match.group(1),
        )
    )

    return sources


def load_napi_platform_package_versions(repo_root: Path) -> list[Source]:
    """Collect versions from every `crates/napi/npm/*/package.json`.

    These are the per-platform prebuilt-binary packages published alongside
    the main `@chordsketch/node` resolver. They must all share the same
    version as the main package or `optionalDependencies` resolution breaks.
    Globbed rather than enumerated so adding a new target triple only
    requires dropping a new `npm/<triple>/package.json` — the check picks
    it up automatically.
    """
    sources: list[Source] = []
    base = repo_root / "crates" / "napi" / "npm"
    if not base.is_dir():
        return sources
    for pkg_json in sorted(base.glob("*/package.json")):
        rel = pkg_json.relative_to(repo_root).as_posix()
        sources.append(load_package_json_version(repo_root, rel))
    return sources


def load_all_sources(repo_root: Path) -> list[Source]:
    sources: list[Source] = []
    sources.extend(load_crate_versions(repo_root))
    sources.append(load_package_json_version(repo_root, "packages/npm/package.json"))
    sources.append(
        load_package_json_version(repo_root, "packages/vscode-extension/package.json")
    )
    sources.append(load_package_json_version(repo_root, "crates/napi/package.json"))
    sources.append(
        load_package_json_version(
            repo_root, "packages/tree-sitter-chordpro/package.json"
        )
    )
    sources.extend(load_napi_platform_package_versions(repo_root))
    sources.extend(load_readme_smoke_pins(repo_root))
    return sources


# ---------------------------------------------------------------- allowlist


def load_allowlist(path: Path) -> list[AllowlistEntry]:
    if not path.is_file():
        # A missing allowlist is a valid (empty) state. The check still runs
        # and will fail on any drift.
        return []
    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except tomllib.TOMLDecodeError as exc:
        raise SystemExit(f"{path}: invalid TOML: {exc}")

    raw_rows = data.get("allowed_skews", [])
    if not isinstance(raw_rows, list):
        raise SystemExit(f"{path}: `allowed_skews` must be an array")

    entries: list[AllowlistEntry] = []
    for index, row in enumerate(raw_rows):
        if not isinstance(row, dict):
            raise SystemExit(f"{path}: allowed_skews[{index}] is not a table")
        try:
            file = str(row["file"])
            field = str(row["field"])
            current_value = str(row["current_value"])
            reason = str(row["reason"]).strip()
            expires_at = str(row["expires_at"])
            tracking_issue = str(row["tracking_issue"]).strip()
        except KeyError as exc:
            raise SystemExit(
                f"{path}: allowed_skews[{index}] missing required field: {exc.args[0]}"
            )
        if not tracking_issue:
            raise SystemExit(
                f"{path}: allowed_skews[{index}] ({file} / {field}) has an empty "
                f"tracking_issue. Every skew MUST reference a GitHub issue so it "
                f"cannot be forgotten. See the rule comment at the top of the file."
            )
        if not reason:
            raise SystemExit(
                f"{path}: allowed_skews[{index}] ({file} / {field}) has an empty reason"
            )
        entries.append(
            AllowlistEntry(
                file=file,
                field=field,
                current_value=current_value,
                reason=reason,
                expires_at=expires_at,
                tracking_issue=tracking_issue,
            )
        )
    return entries


def _key(entry: AllowlistEntry) -> tuple[str, str]:
    return (entry.file, entry.field)


def _source_key(source: Source) -> tuple[str, str]:
    return (source.file, source.field)


# ---------------------------------------------------------------- core check


# The caret-constraint source stores a bare `<major>.<minor>` value (extracted
# from `^<major>.<minor>`), which cannot be compared against canonical
# `<major>.<minor>.<patch>` directly. Every source has its own "expected value
# given the canonical crate version" function. The default is identity.
_CARET_FIELD_LABEL = "library-smoke caret constraint (^major.minor)"


def _expected_for(source: Source, canonical: str) -> str:
    if source.field == _CARET_FIELD_LABEL:
        parts = canonical.split(".")
        if len(parts) < 2:
            return canonical
        return f"{parts[0]}.{parts[1]}"
    return canonical


def compute_canonical(crate_sources: list[Source]) -> str:
    """The canonical version is the unanimous crate version.

    If crates disagree, there is no single canonical — this is itself a drift
    that the caller must surface as an error. We could pick a mode, but
    silently accepting crate-level drift would defeat the purpose of the check.
    """
    values = {source.value for source in crate_sources}
    if len(values) == 1:
        return next(iter(values))
    raise SystemExit(
        "crates/*/Cargo.toml versions disagree — cannot derive canonical version.\n"
        + "\n".join(f"  {s.file}: {s.value}" for s in crate_sources)
    )


def check(
    sources: list[Source],
    allowlist: list[AllowlistEntry],
    canonical: str,
) -> tuple[list[Drift], list[AllowlistEntry]]:
    """Return (drifts, stale_entries).

    A source is OK if:
      - value matches canonical, OR
      - an allowlist entry matches its (file, field) AND the entry's
        `current_value` matches the source's `value`.

    An allowlist entry is "load-bearing" if the source it covers is
    currently drifting (whether or not `current_value` is up to date —
    a wrong `current_value` is still a drift, and the entry still has a
    role to play). "Stale" means not load-bearing:

      - the (file, field) no longer exists in the repo (file renamed
        or removed), OR
      - the source exists but no longer drifts (the underlying problem
        was resolved without also retiring the entry).

    Crucially, the case where an entry exists AND the source still drifts
    but `current_value` is wrong is NOT stale — the entry is load-bearing,
    just needs updating. That case is surfaced as a drift with a message
    that tells the maintainer exactly what to do, and must not also fire
    the stale-entry message (which would tell them to remove the entry
    instead — the wrong action). See #1513.
    """
    drifts: list[Drift] = []

    by_key: dict[tuple[str, str], AllowlistEntry] = {_key(e): e for e in allowlist}

    # Keys whose source is currently drifting. An allowlist entry is
    # load-bearing iff its key is in this set — a clean, non-drifting
    # source makes any entry pointing at it stale.
    load_bearing_keys: set[tuple[str, str]] = set()

    for source in sources:
        expected = _expected_for(source, canonical)

        # Most sources should equal the canonical (or the caret-derived form).
        if source.value == expected:
            continue

        # Drifted: allowlist may suppress.
        key = _source_key(source)
        entry = by_key.get(key)
        if entry is None:
            drifts.append(
                Drift(
                    source=source,
                    canonical=canonical,
                    detail=(
                        f"{source.value!r} drifted from expected {expected!r} "
                        f"(canonical={canonical!r}) and has no allowlist entry. "
                        f"Either re-align the value or add an `[[allowed_skews]]` "
                        f"row for ({source.file!r}, {source.field!r}) with a "
                        f"reason, expires_at, and tracking_issue."
                    ),
                )
            )
            continue

        # An entry exists for a drifting source — it IS load-bearing, even
        # if `current_value` is stale. Whether the entry is accurate is a
        # separate check below.
        load_bearing_keys.add(key)

        if entry.current_value != source.value:
            drifts.append(
                Drift(
                    source=source,
                    canonical=canonical,
                    detail=(
                        f"allowlist entry for ({source.file!r}, {source.field!r}) "
                        f"says current_value={entry.current_value!r} but actual "
                        f"value is {source.value!r}. Either update the allowlist "
                        f"or re-align the source."
                    ),
                )
            )
            continue

    stale_entries = [entry for entry in allowlist if _key(entry) not in load_bearing_keys]
    return drifts, stale_entries


def run(
    repo_root: Path,
    allowlist_path: Path,
) -> int:
    sources = load_all_sources(repo_root)
    crate_sources = [s for s in sources if s.file.startswith("crates/") and s.file.endswith("/Cargo.toml")]
    canonical = compute_canonical(crate_sources)
    allowlist = load_allowlist(allowlist_path)

    drifts, stale_entries = check(sources, allowlist, canonical)

    # Report.
    print(f"canonical version (from crates/*/Cargo.toml): {canonical}")
    print(f"sources checked: {len(sources)}")
    print(f"allowlist entries: {len(allowlist)}")

    if drifts:
        print()
        print(f"ERROR: {len(drifts)} unallowlisted drift(s) detected:")
        for drift in drifts:
            print(f"  - {drift.source.file} / {drift.source.field}")
            print(f"      value={drift.source.value!r} canonical={drift.canonical!r}")
            print(f"      {drift.detail}")

    if stale_entries:
        print()
        print(f"ERROR: {len(stale_entries)} stale allowlist entry/entries:")
        for entry in stale_entries:
            print(f"  - {entry.file} / {entry.field}")
            print(f"      no matching source found — the drift has been resolved.")
            print(f"      remove this entry AND close tracking issue {entry.tracking_issue}.")

    if drifts or stale_entries:
        return 1

    print()
    print("OK: all sources are in sync (or covered by the allowlist)")
    return 0


# ---------------------------------------------------------------- CLI


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.strip().splitlines()[0])
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=REPO_ROOT,
        help="repo root (defaults to the ancestor containing this script)",
    )
    parser.add_argument(
        "--allowlist",
        type=Path,
        default=DEFAULT_ALLOWLIST_PATH,
        help="allowlist file path (defaults to ci/version-skew-allowlist.toml)",
    )
    args = parser.parse_args()
    return run(args.repo_root, args.allowlist)


if __name__ == "__main__":
    raise SystemExit(main())
