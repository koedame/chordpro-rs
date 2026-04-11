# 0001. Kotlin Maven Central publishing credentials

- **Status**: Accepted
- **Date**: 2026-04-11

## Context

ChordSketch publishes binding packages to multiple language registries from
GitHub Actions. The security goal across all of these channels is to
eliminate long-lived publishing credentials in favour of GitHub OIDC
trusted publishing, where each release run obtains a short-lived token
scoped to a specific repository, workflow, and environment.

This goal has already been achieved for two of the three current channels:

- **PyPI** — `python.yml` uses `pypa/gh-action-pypi-publish` with OIDC
  trusted publishing.
- **RubyGems** — `ruby.yml` uses `rubygems/configure-rubygems-credentials`
  with OIDC trusted publishing (see lines 247–328).

The third channel, **Maven Central** (Kotlin), still authenticates with
long-lived `CENTRAL_PORTAL_USERNAME` / `CENTRAL_PORTAL_PASSWORD` user
tokens stored in the `maven-central` GitHub environment, as configured in
`.github/workflows/kotlin.yml` lines 173–245.

#1124 proposed migrating Kotlin to OIDC to bring it in line with the other
two channels and remove the last long-lived publishing credentials from
the repository.

A research pass on **2026-04-11** confirmed the migration is **blocked
upstream**:

1. **Sonatype Central Portal exposes no OIDC token-exchange endpoint.**
   The URL `https://central.sonatype.com/publishing/trusted-publishers`
   resolves but serves only an empty single-page-application shell — the
   route exists but no feature is wired behind it. Sonatype's documented
   OIDC support applies to Nexus Repository Cloud / Pro 3.86+, not the
   Central Portal that publishes to Maven Central.
2. **The vanniktech `gradle-maven-publish-plugin` has no OIDC code path.**
   Latest release 0.36.0 (2025-01-18) still requires
   `mavenCentralUsername` / `mavenCentralPassword`. Release notes through
   0.33–0.36 cover the OSSRH sunset and Central Portal migration but
   contain zero mentions of OIDC, trusted publishing, or `id-token`.
3. **No official Sonatype GitHub Action exists** for Central Portal
   publishing. The `sonatype/actions` organisation publishes only
   Lifecycle / IQ Server tooling (Evaluate, Fetch SBOM, Setup Sonatype
   CLI, Run Sonatype CLI), none of which authenticate to Maven Central.
4. **No community GitHub Action implements OIDC ↔ Central Portal token
   exchange.** Searches surface only guides that use long-lived user
   tokens (`teamlead/java-maven-sonatype-starter`, `davidcarboni/releaser`,
   sbt-ci-release, etc.).

The migration cannot be implemented today without writing a custom OIDC
bridge whose counter-party (the Sonatype OIDC endpoint) does not exist.

## Decision

Continue to authenticate the Kotlin Maven Central publish job with the
long-lived `CENTRAL_PORTAL_USERNAME` / `CENTRAL_PORTAL_PASSWORD` secrets
held in the `maven-central` GitHub environment. Do not introduce a custom
OIDC bridge or speculative tooling. Re-evaluate when **any** of the watch
signals listed in [References](#references) flips.

## Rationale

The four upstream gaps above are joint blockers — fixing any one of them
on our side would still leave the migration non-functional because the
others are missing. Specifically:

- Adding `id-token: write` permission to the publish job is harmless but
  meaningless without an action that consumes the resulting token.
- Forking the vanniktech plugin to add an OIDC code path would still need
  a Sonatype-side endpoint to exchange the token against.
- Writing a custom token-exchange action has no Sonatype URL to POST to.

The honest engineering answer is to wait. A workaround built today would
have to be ripped out the moment Sonatype ships the real feature, and
maintaining the workaround would consume the engineering attention that
should instead go toward monitoring the watch signals.

The empty SPA shell at `central.sonatype.com/publishing/trusted-publishers`
is the only weak signal that Sonatype is building this feature — it is
not yet usable but is worth checking periodically.

## Consequences

**Positive**

- No engineering time is spent building a bridge that would have to be
  removed later.
- The Kotlin publish flow stays simple and matches the patterns documented
  for the vanniktech plugin and Central Portal.
- The repository carries an honest, discoverable record of *why* one
  channel is asymmetric with the others, so a future contributor (or
  Claude session) does not re-propose the migration without the upstream
  context.

**Negative**

- Long-lived `CENTRAL_PORTAL_*` secrets remain in the `maven-central`
  environment. They require manual rotation per Sonatype's user-token
  policy and represent an exfiltration window the other channels do not
  have.
- Incident response for a credential leak must remember that Maven
  Central is the asymmetric channel and follow the user-token revocation
  flow at https://central.sonatype.com/account.

**Mitigations**

- The secrets are scoped to the `maven-central` environment, not the
  repository (see `kotlin.yml` lines 196–198), so they are not exposed to
  pull-request workflow runs or to unrelated jobs.
- The `maven-central` environment can have required reviewers added if
  the threat model demands it; this ADR does not mandate that change but
  notes it as an available lever.

## Alternatives considered

- **Build a custom OIDC bridge action** — rejected. There is no Sonatype
  endpoint to exchange the GitHub token against, so the action would be
  a no-op shell. Implementation cost: small. Value delivered: zero until
  Sonatype ships an endpoint, at which point the official path will
  presumably work better.
- **Fork the vanniktech plugin to add OIDC support** — rejected for the
  same reason. An OIDC code path in the plugin still needs a server-side
  counterpart.
- **Move to a different Maven Central client (e.g. JReleaser)** — not
  pursued. JReleaser also targets the existing Central Portal API and
  would face the same upstream block. Switching clients is a much larger
  change than the OIDC migration would have been.
- **Close #1124 silently without an ADR** — rejected. The rationale would
  be buried in issue history and the decision would be invisible to
  anyone scanning the repository for "why is Kotlin asymmetric?". An ADR
  is the lowest-cost way to make the reasoning permanent and
  discoverable.
- **Leave #1124 open as a passive reminder** — rejected by the
  maintainer. The issue cannot be acted on, has no clear deadline, and
  fragments the work-in-progress signal in the issue tracker. A watch
  signal in an ADR is a better fit than a perpetually-open issue.

## References

**GitHub**

- Issue: koedame/chordsketch#1124 (closed by this ADR)
- Tracking issue for this ADR: koedame/chordsketch#1501

**Sibling implementations to mirror once unblocked**

- `.github/workflows/ruby.yml` lines 247–328 — RubyGems OIDC pattern
  using `rubygems/configure-rubygems-credentials` and an
  `ACTIONS_ID_TOKEN_REQUEST_URL` availability probe.
- `.github/workflows/python.yml` — PyPI OIDC pattern using
  `pypa/gh-action-pypi-publish`.

**Current Kotlin publish job**

- `.github/workflows/kotlin.yml` lines 173–245 — the publish job that
  this ADR governs.

**Watch signals — re-open the migration when any of these flip**

1. The vanniktech `gradle-maven-publish-plugin`
   ([releases](https://github.com/vanniktech/gradle-maven-publish-plugin/releases))
   ships a release whose notes mention `oidc`, `trusted publish`, or
   `id-token`.
2. `https://central.sonatype.com/publishing/trusted-publishers` returns
   substantive content rather than an empty SPA shell.
3. An official `sonatype/*` GitHub Action for Central Portal publishing
   appears in the [sonatype organisation](https://github.com/sonatype).
4. `https://central.sonatype.org/publish/publish-portal-gradle/`
   documents an OIDC-based authentication flow.

When any of these flips, file a fresh issue referencing this ADR and the
specific signal that changed, and follow the normal PR workflow for the
migration.
