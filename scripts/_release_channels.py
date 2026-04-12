"""Shared helper for loading `ci/release-channels.toml`.

Imported by:
  - scripts/check-release-channels.py
  - scripts/check-env-secrets.py

Kept stdlib-only so every script that consumes the manifest can run from a
fresh GitHub Actions runner with no `pip install` step.

Name starts with an underscore so no one mistakes it for a directly-invocable
script. It is a private implementation module.
"""

from __future__ import annotations

import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = REPO_ROOT / "ci" / "release-channels.toml"

# The complete set of channel kinds the verifier knows how to query. Any
# manifest entry whose `kind` is not in this set triggers a structural error
# so maintainers cannot silently add unknown channel types.
KNOWN_KINDS = frozenset(
    {
        "crates-io",
        "npm",
        "ghcr",
        "docker-hub",
        "vscode-marketplace",
        "open-vsx",
        "pypi",
        "rubygems",
        "maven-central",
        "homebrew-tap",
        "scoop-bucket",
        "manual",
    }
)


@dataclass(frozen=True)
class Channel:
    """A single row from `ci/release-channels.toml`.

    All fields match the manifest schema documented at the top of the
    manifest file. `skip_reason` and `notes` are empty strings when the
    manifest did not provide them, so downstream code can treat them as
    always-present.
    """

    id: str
    display: str
    kind: str
    package: str
    expected_version: str  # "tag" | "skip" | an explicit version string
    required_secrets: tuple[str, ...]
    skip_reason: str
    notes: str

    @property
    def is_skip(self) -> bool:
        """True if this channel is intentionally skipped from the rollup."""
        return self.expected_version == "skip"


class ManifestError(Exception):
    """Raised when the manifest file is structurally invalid."""


def load_channels(path: Path = MANIFEST_PATH) -> list[Channel]:
    """Load and validate every channel entry from the manifest.

    Raises `ManifestError` on any structural problem: unknown `kind`, missing
    required field, duplicate `id`, or a `skip` entry without `skip_reason`.
    Validation is intentionally strict — the whole point of the manifest is
    to be the single source of truth, so silent drift is worse than a loud
    error at CI time.
    """

    try:
        raw = path.read_bytes()
    except FileNotFoundError as exc:
        raise ManifestError(f"manifest not found: {path}") from exc

    try:
        data = tomllib.loads(raw.decode("utf-8"))
    except tomllib.TOMLDecodeError as exc:
        raise ManifestError(f"manifest is not valid TOML: {exc}") from exc

    rows = data.get("channels")
    if not isinstance(rows, list) or not rows:
        raise ManifestError("manifest must contain a non-empty [[channels]] array")

    channels: list[Channel] = []
    seen_ids: set[str] = set()

    for index, row in enumerate(rows):
        if not isinstance(row, dict):
            raise ManifestError(f"channels[{index}] is not a table")

        try:
            channel_id = str(row["id"])
            display = str(row["display"])
            kind = str(row["kind"])
            package = str(row["package"]) if "package" in row else ""
            expected_version = str(row["expected_version"])
        except KeyError as exc:
            raise ManifestError(
                f"channels[{index}] missing required field: {exc.args[0]}"
            ) from exc

        if channel_id in seen_ids:
            raise ManifestError(f"duplicate channel id: {channel_id!r}")
        seen_ids.add(channel_id)

        if kind not in KNOWN_KINDS:
            raise ManifestError(
                f"channels[{index}] ({channel_id}): unknown kind {kind!r} — "
                f"add it to KNOWN_KINDS and implement a check function"
            )

        required_secrets_raw = row.get("required_secrets", [])
        if not isinstance(required_secrets_raw, list) or not all(
            isinstance(s, str) for s in required_secrets_raw
        ):
            raise ManifestError(
                f"channels[{index}] ({channel_id}): required_secrets must be a list of strings"
            )

        skip_reason = str(row.get("skip_reason", "")).strip()
        if expected_version == "skip" and not skip_reason:
            raise ManifestError(
                f"channels[{index}] ({channel_id}): expected_version=skip requires skip_reason"
            )
        if expected_version != "skip" and not package:
            raise ManifestError(
                f"channels[{index}] ({channel_id}): non-skip channel must have a package"
            )

        channels.append(
            Channel(
                id=channel_id,
                display=display,
                kind=kind,
                package=package,
                expected_version=expected_version,
                required_secrets=tuple(required_secrets_raw),
                skip_reason=skip_reason,
                notes=str(row.get("notes", "")).strip(),
            )
        )

    return channels


def find_channel(channels: list[Channel], channel_id: str) -> Channel:
    """Return the named channel or raise `ManifestError`."""
    for channel in channels:
        if channel.id == channel_id:
            return channel
    available = ", ".join(sorted(c.id for c in channels))
    raise ManifestError(
        f"channel {channel_id!r} not found in manifest. Available: {available}"
    )


def main() -> int:
    """Entry point for ad-hoc validation: python3 _release_channels.py."""
    try:
        channels = load_channels()
    except ManifestError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print(f"loaded {len(channels)} channels from {MANIFEST_PATH}")
    for channel in channels:
        marker = "SKIP" if channel.is_skip else "    "
        print(f"  {marker} {channel.id:<28} [{channel.kind}]")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
