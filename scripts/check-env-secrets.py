#!/usr/bin/env python3
"""Fail-fast secrets check for publish jobs.

Invoked as the first step of every publish job in `.github/workflows/*.yml`.
Reads `ci/release-channels.toml`, finds the channel with the requested id,
and asserts every secret listed in `required_secrets` is present and
non-empty in the current environment.

Rationale: publish jobs were silently skipping when a GitHub environment was
missing (e.g. `vscode-marketplace` was never created, so the VS Code
Marketplace publish job has been dormant for three weeks with a green
workflow). Running this step at the top of each publish job turns that
silent-skip failure into a loud, diagnosable error.

This does NOT fix the silent-skip case where the *environment gate itself*
is missing — that case is covered by the post-release channel rollup in
`.github/workflows/release-verify.yml`. This script is the input-side half
of the same defense-in-depth pair.

Usage (from a workflow step):

    - name: Check required secrets present
      env:
        VSCE_PAT: ${{ secrets.VSCE_PAT }}
      run: python3 scripts/check-env-secrets.py --channel vscode-marketplace

Exits 0 on success, 1 on any missing secret. Prints a diagnostic to stderr
that names every missing secret so a maintainer can jump straight to the
environment settings page without inspecting the workflow.
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _release_channels import find_channel, load_channels  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.strip().splitlines()[0])
    parser.add_argument(
        "--channel",
        required=True,
        help="channel id from ci/release-channels.toml",
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        default=None,
        help="override manifest path (defaults to ci/release-channels.toml)",
    )
    parser.add_argument(
        "--soft",
        action="store_true",
        help=(
            "soft mode: on missing secret, emit `available=false` to stdout and "
            "exit 0 instead of failing the job. Use this only when a publish "
            "channel is intentionally optional (e.g. Docker Hub, which mirrors "
            "GHCR and must not fail the workflow when credentials are absent "
            "— see #1072). Hard mode (default) is correct for primary channels."
        ),
    )
    args = parser.parse_args()

    channels = load_channels(args.manifest) if args.manifest else load_channels()
    try:
        channel = find_channel(channels, args.channel)
    except Exception as exc:  # noqa: BLE001
        print(f"error: {exc}", file=sys.stderr)
        return 1

    if channel.is_skip:
        # Skip channels have no publish job, so the script should never be
        # called for one. Treat it as a maintainer mistake and fail loud.
        print(
            f"error: channel {channel.id!r} is declared as 'skip' in the manifest "
            f"but a workflow is trying to check its secrets. Remove this step "
            f"from the workflow or un-skip the channel.",
            file=sys.stderr,
        )
        return 1

    missing: list[str] = []
    for secret in channel.required_secrets:
        value = os.environ.get(secret, "")
        if not value:
            missing.append(secret)

    if missing:
        if args.soft:
            print("available=false")
            print(
                f"::warning::channel {channel.id!r} is missing secrets "
                f"({', '.join(missing)}); downstream steps will be skipped.",
                file=sys.stderr,
            )
            return 0
        print(
            f"error: channel {channel.id!r} is missing required secrets: "
            f"{', '.join(missing)}",
            file=sys.stderr,
        )
        print(
            "Fix: add the missing secrets to the GitHub environment used by "
            "this publish job (Settings → Environments → <environment name>). "
            "If this workflow does not intentionally publish this channel, "
            "remove the check-env step instead.",
            file=sys.stderr,
        )
        return 1

    if args.soft:
        print("available=true")
    if not channel.required_secrets:
        print(
            f"channel {channel.id!r} has no required_secrets — nothing to check",
            file=sys.stderr if args.soft else sys.stdout,
        )
    else:
        print(
            f"channel {channel.id!r}: all required secrets present "
            f"({', '.join(channel.required_secrets)})",
            file=sys.stderr if args.soft else sys.stdout,
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
