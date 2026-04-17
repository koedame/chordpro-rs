---
name: ADR 0002 — AUR install command is exempt from readme-smoke coverage
description: Captures why `yay -S chordsketch` has no end-to-end smoke job and what instead verifies AUR package correctness.
type: adr
---

# 0002. AUR install command is exempt from readme-smoke coverage

- **Status**: Accepted
- **Date**: 2026-04-18

## Context

`README.md` documents `yay -S chordsketch` as the AUR install path, and
`.github/snapshots/readme-commands.txt` records the command (line 9).
`.claude/rules/readme-sync.md` states that every install command in
`README.md` MUST be exercised end-to-end by
`.github/workflows/readme-smoke.yml` via the `cli-render-smoke` composite
action; snapshot updates without corresponding CI coverage "defeat the
purpose of this rule."

PR #1774 added the AUR snapshot entry but did not add a CI job. The
workflow contains a comment at the point the job would live:

> AUR smoke is intentionally omitted. GitHub-hosted runners do not have
> yay/paru or an Arch Linux base, and installing an AUR helper in CI
> would require bootstrapping pacman on Ubuntu — fragile and slow.
> AUR package correctness is verified by the PKGBUILD template in
> packaging/aur/ and the post-release.yml update-aur job.

Issue #1775 asked whether this is a rule violation and listed three
possible resolutions: add the job anyway, write an ADR / rule update
permitting the omission, or mark the command coverage-exempt in the
snapshot/extractor.

## Decision

Treat the AUR install command as **exempt from readme-smoke coverage**
and document the exemption in this ADR. Do not add an AUR smoke job.

## Rationale

1. **No supported runner base.** GitHub-hosted runners use
   Ubuntu / macOS / Windows. `yay` and `paru` are AUR helpers that
   require a functioning Arch Linux pacman environment. There is no
   Arch-based GitHub-hosted runner, so a smoke job would need to run
   inside an `archlinux` container, install a non-root user, bootstrap
   an AUR helper from source, and then exercise the install. That
   bootstrap has historically been fragile (makepkg relies on `sudo`
   prompts that container defaults disable, and AUR-helper upstream
   releases shift frequently).
2. **The install path is already covered by a different guarantee.**
   `post-release.yml` runs the `update-aur` job that pushes the
   generated PKGBUILD to the `aur@aur.archlinux.org` repo using the
   `AUR_SSH_KEY` secret, and `ci/release-channels.toml` marks the
   `aur` channel as part of the release-verify rollup. If the PKGBUILD
   is malformed, the publish push fails and the rollup row goes red.
   The install-side defect surface (the PKGBUILD builds but the
   installed binary is broken) is caught by the PKGBUILD's own
   `check()` stage running on an actual Arch environment during
   submission review, not by CI.
3. **The cost-benefit ratio is adverse.** The other smoke jobs each
   take ~1-2 minutes. An Arch bootstrap would take longer and produce
   false-positive failures whenever AUR-helper upstream changed, at the
   exact moment a release is being cut. The time would be better spent
   on other release verifications.

## Consequences

Positive:

- `readme-smoke.yml` remains fast and stable.
- `readme-sync.md`'s guarantee ("every install command is exercised
  end-to-end") now has exactly one durably-documented exception.

Negative:

- A future defect in the AUR install path that is NOT caught by the
  `update-aur` publish push and not caught during PKGBUILD review
  could reach users. Mitigation: the `packaging/aur/` PKGBUILD template
  is reviewed whenever `README.md` install instructions change, and
  the post-release rollup would still show the AUR channel row so a
  publish-side regression (the most likely failure mode) is visible.

## Alternatives considered

- **Add an `archlinux` container-based smoke job.** Rejected because of
  the bootstrap fragility described in Rationale §1 and because the
  install-side surface is already covered by §2. A job that takes four
  times longer than its siblings and fails semi-regularly for reasons
  unrelated to ChordSketch is a net negative.
- **Amend `.github/snapshots/readme-commands.txt` or the extractor
  script to mark AUR commands coverage-exempt.** Rejected because a
  file-level exemption hides the rationale inside tooling; a future
  contributor scanning the snapshot would not see why the AUR entry
  escapes the rule. An ADR surfaces the rationale at the point a
  reviewer looks (`docs/adr/README.md`) and is linked from the
  workflow comment so the omission is durable.
- **Remove the AUR install command from `README.md`.** Rejected
  because AUR is a real, documented distribution channel for Arch
  users; pulling the command to satisfy a CI rule would harm actual
  users to protect a meta-rule.

## References

- Issue #1775 (this ADR resolves it)
- PR #1774 (added AUR to README and snapshot)
- `.claude/rules/readme-sync.md`
- `.github/workflows/readme-smoke.yml` (comment at the AUR slot cross-
  references this ADR)
- `.github/workflows/post-release.yml` `update-aur` job
- `ci/release-channels.toml` (`aur` channel, release-verify rollup)

Watch signals that should prompt revisiting this decision:

- GitHub adding an Arch-based hosted runner family.
- An `archlinux` container image with a pre-built AUR helper that is
  maintained upstream and stable across releases.
- An AUR install-side regression that publish-push did not catch (which
  would indicate the current coverage model has a real gap).
