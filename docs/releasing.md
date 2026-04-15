# Versioning and Release Process

## Versioning Policy

All ten Rust crates in the workspace share the same version number and are
bumped in lockstep. This project follows [Semantic Versioning](https://semver.org/):

- **Major** (1.0.0) — breaking API changes
- **Minor** (0.2.0) — new features, backward compatible
- **Patch** (0.1.1) — bug fixes, backward compatible

While the version is below 1.0.0, minor version bumps may include breaking
changes.

The npm package `@chordsketch/wasm` *usually* tracks the workspace version,
but is allowed to be **skewed by patch versions** when a packaging-only fix is
shipped (the WASM library code itself is unchanged but the npm wrapper needs
a bump). For example, after the dual-package fix the npm package was at
`0.1.1` while all crates remained at `0.1.0`. The wasm `version()` function
returns the **Rust crate version** (`"0.1.0"`), not the npm wrapper version,
because it is compiled into the `.wasm` binary. This skew is acceptable and
re-syncs at the next workspace-wide release.

## Release Checklist

### Pre-release sanity

Before starting the bump in Step 1, verify the release-time infrastructure
is healthy. Any gap here would silently break a channel and be discovered
at post-release verification rather than before the tag is cut.

1. **Every expected secret exists.** Cross-reference
   `ci/release-channels.toml`'s `required_secrets` against the repo secret
   list:
   ```bash
   gh secret list -R koedame/chordsketch
   ```
   Every secret listed in `required_secrets` (any field) must appear.
2. **Every referenced environment exists.** The silent VS Code Marketplace
   skip happened because the `vscode-marketplace` environment was never
   created. Guard against a recurrence:
   ```bash
   gh api repos/koedame/chordsketch/environments --jq '.environments[].name'
   ```
   Every `environment:` name used in a publish job (`npm`, `docker-hub`,
   `vscode-marketplace`, `pypi`, `rubygems`, `maven-central`, `napi`) must
   appear in the output.
3. **`ci.yml` and `readme-smoke.yml` are green on the target commit.** The
   release workflow builds from that commit, so a red CI is a release
   blocker:
   ```bash
   gh run list --branch main --workflow ci.yml -R koedame/chordsketch --limit 1
   gh run list --branch main --workflow readme-smoke.yml -R koedame/chordsketch --limit 1
   ```
4. **The version-consistency check is green.** This catches any manifest
   that has drifted from the canonical workspace version without an
   explicit allowlist entry:
   ```bash
   python3 scripts/check-version-consistency.py
   ```

### Checklist

1. **Update version** in every versioned manifest:

   Workspace Cargo.toml files (all ten crates):
   - `crates/core/Cargo.toml`
   - `crates/render-text/Cargo.toml`
   - `crates/render-html/Cargo.toml`
   - `crates/render-pdf/Cargo.toml`
   - `crates/cli/Cargo.toml`
   - `crates/wasm/Cargo.toml`
   - `crates/ffi/Cargo.toml`
   - `crates/napi/Cargo.toml`
   - `crates/lsp/Cargo.toml`
   - `crates/convert-musicxml/Cargo.toml`
   - Update inter-crate dependency `version = ` fields to match.

   Non-Rust manifests:
   - `packages/npm/package.json` (unless an allowlisted patch skew applies
     — see `ci/version-skew-allowlist.toml`)
   - `packages/vscode-extension/package.json` (once the first Marketplace
     publish has succeeded and its allowlist entry has been retired)
   - `crates/napi/package.json` — both the main package and the per-platform
     manifests under `crates/napi/npm/<triple>/package.json`
   - `packages/tree-sitter-chordpro/package.json`

   Hardcoded pins in CI:
   - `.github/workflows/readme-smoke.yml` ~line 204:
     `npm install '@chordsketch/wasm@<version>'`
   - `.github/workflows/readme-smoke.yml` ~lines 450–451:
     `chordsketch-core = "^<major>.<minor>"` and the matching
     `chordsketch-render-text` pin

   Allowlist (if applicable):
   - If this release re-syncs any drift, remove the corresponding entries
     from `ci/version-skew-allowlist.toml` **and close their tracking
     issues in the same commit**. Leaving stale entries causes
     `check-version-consistency.py` to fail (which is the point — you
     can't forget).

   Sanity: run `python3 scripts/check-version-consistency.py` after the
   edit. It must exit 0 before you commit.

2. **Update CHANGELOG.md**: change `## [X.Y.Z] - Unreleased` to
   `## [X.Y.Z] - YYYY-MM-DD` and add a new `## [Unreleased]` section above.

3. **Commit** with message: `Release vX.Y.Z`

4. **Create and push tag**:
   ```bash
   git tag vX.Y.Z
   git push origin main vX.Y.Z
   ```

5. **Wait for the release workflow**: pushing the tag triggers
   `.github/workflows/release.yml`, which builds binaries for all targets and
   creates a GitHub Release with archives attached.

6. **Publish to crates.io** in dependency order:
   ```bash
   cargo publish -p chordsketch-core
   # Wait ~30 seconds for the crates.io index to update
   cargo publish -p chordsketch-render-text
   cargo publish -p chordsketch-render-html
   cargo publish -p chordsketch-render-pdf
   # Wait ~30 seconds for renderer crates to propagate
   cargo publish -p chordsketch
   ```

7. **Publish `@chordsketch/wasm` to npm**: trigger
   `.github/workflows/npm-publish.yml` via `workflow_dispatch` with the
   version input:
   ```bash
   gh workflow run npm-publish.yml -f version=X.Y.Z -R koedame/chordsketch
   ```
   ⚠️ **First publish of any new `@chordsketch/*` package needs the manual
   local fallback** — see "Known operational quirks" below. The CI publish
   path can only update *existing* packages reliably.

8. **Submit winget-pkgs PR**: see "Post-Release > winget" below. This is the
   only post-release step that involves an external repo (`microsoft/winget-pkgs`).

## crates.io Publishing Order

Crates must be published in dependency order because crates.io resolves
versions from the registry, not the local workspace. The inter-crate
dependencies specify both `path` (for local development) and `version` (for
crates.io).

After publishing each crate, wait for the crates.io index to update before
publishing dependents. This typically takes 10-30 seconds.

Publishing order:
1. `chordsketch-core` (no internal dependencies)
2. `chordsketch-render-text` (depends on `chordsketch-core`)
3. `chordsketch-render-html` (depends on `chordsketch-core`)
4. `chordsketch-render-pdf` (depends on `chordsketch-core`)
5. `chordsketch` (depends on all four above)

Steps 2-4 can be published in any order among themselves, but all must complete
before step 5.

## Distribution Channels

`koedame/chordsketch` is distributed across multiple channels. Each channel
has its own automation, secret, and verification path. The
`.github/workflows/readme-smoke.yml` workflow exercises every channel
end-to-end after each release as the single source of truth for "is the
project's promised distribution actually working right now".

This table is the **human-readable view** of `ci/release-channels.toml`.
When adding a new channel, update both.

| Channel | Identifier | Trigger | Required secret(s) | Verified by |
|---|---|---|---|---|
| crates.io | `chordsketch` (CLI) + 4 lib crates | manual `cargo publish` (Step 6) | maintainer's `~/.cargo/credentials` | `cargo-install` job |
| GitHub Releases | binary archives | `release.yml` on tag push | `GITHUB_TOKEN` | `source-build` job |
| GHCR | `ghcr.io/koedame/chordsketch` | `docker.yml` on `release: published` | `GITHUB_TOKEN` (push), org policy must allow public packages | `docker-ghcr` job |
| Docker Hub | `docker.io/koedame/chordsketch` | `docker.yml` on `release: published` | `DOCKERHUB_USERNAME`, `DOCKERHUB_TOKEN` | `docker-hub` job |
| npm (wasm) | `@chordsketch/wasm` | `npm-publish.yml` `workflow_dispatch` (Step 7) | `NPM_TOKEN` (Granular Token, see quirks) | `npm-wasm` job |
| npm (napi) | `@chordsketch/node` + 5 prebuilt platform packages | `napi.yml` on `release: published` | `NPM_TOKEN` | `napi-node` job |
| npm (tree-sitter) | `tree-sitter-chordpro` | `npm-publish-tree-sitter.yml` on `release: published` | `NPM_TOKEN` | `npm-tree-sitter` rollup entry |
| Homebrew tap | `koedame/tap/chordsketch` | `post-release.yml` on `release: published` | `TAP_GITHUB_TOKEN` | `homebrew` job |
| Scoop bucket | `koedame/scoop-bucket/chordsketch` | `post-release.yml` on `release: published` | `TAP_GITHUB_TOKEN` | `scoop` job |
| AUR | `chordsketch` | `post-release.yml` on `release: published` | `AUR_SSH_KEY` | `aur` rollup entry |
| Snap Store | `chordsketch` | `post-release.yml` on `release: published` | `SNAP_STORE_TOKEN` | `snap` rollup entry |
| nixpkgs | `pkgs.chordsketch` | manual PR to `NixOS/nixpkgs` | none | `nixpkgs` rollup entry |
| winget | `koedame.chordsketch` | manual PR to `microsoft/winget-pkgs` (Step 8) | none (uses your `gh` token to fork+push) | `winget` job |
| VS Code Marketplace | `koedame.chordsketch` | `vscode-extension.yml` on `release: published` | `VSCE_PAT` (PAT, Marketplace Publish scope) | `vscode-marketplace` rollup entry |
| PyPI | `chordsketch` | `python.yml` on tag push | none (OIDC trusted publisher) | `pypi` rollup entry |
| RubyGems | `chordsketch` | `ruby.yml` on tag push | none (OIDC trusted publisher) | `rubygems` rollup entry |
| Maven Central | `io.github.koedame:chordsketch` | `kotlin.yml` on tag push | `MAVEN_CENTRAL_USERNAME`, `MAVEN_CENTRAL_PASSWORD`, `SIGNING_KEY`, `SIGNING_PASSWORD` | `maven-central` rollup entry |
| CocoaPods | `ChordSketch` | `post-release.yml` on `release: published` | `COCOAPODS_TRUNK_TOKEN` | `cocoapods` rollup entry |
| JetBrains Marketplace | `me.koeda.chordsketch` | manual `./gradlew publishPlugin` | `JETBRAINS_MARKETPLACE_TOKEN` | not yet automated |
| from source | `git clone` + `cargo install --path crates/cli` | always available | none | `source-build` job |
| Library Usage (Rust) | crates.io snippet from README | implicit via crates.io | none | `library-smoke` job |

## Post-Release

After the release workflow completes and the GitHub Release is published:

1. **Automatic updates** — the `post-release.yml` workflow triggers on
   release publication and automatically:
   - Updates the Homebrew formula in `koedame/homebrew-tap`
   - Updates the Scoop manifest in `koedame/scoop-bucket`

2. **Docker images** — the `docker.yml` workflow triggers on release
   publication and builds a multi-arch Docker image (linux/amd64,
   linux/arm64) pushed to **both** `ghcr.io/koedame/chordsketch` AND
   `docker.io/koedame/chordsketch`. The Docker Hub push depends on the
   `DOCKERHUB_USERNAME` / `DOCKERHUB_TOKEN` secrets being present.
   ⚠️ **First-time setup**: if the Docker Hub repo `koedame/chordsketch`
   does not exist yet, create it manually (Public visibility) at
   <https://hub.docker.com/repository/create> before triggering the workflow.
   ⚠️ **Namespace ownership**: the `docker.io/koedame` namespace is owned by
   the project maintainer. If ownership is ever lost or the namespace is
   transferred to a different party, anyone running
   `docker pull docker.io/koedame/chordsketch:latest` would receive whatever
   the new owner serves. In that scenario, immediately remove the Docker Hub
   install instructions from `README.md` and stop referencing
   `docker.io/koedame/chordsketch` in `readme-smoke.yml`.

3. **npm package** — see Step 7 of the Release Checklist above. CI workflow
   only updates existing packages; first publish of any new `@chordsketch/*`
   name requires the manual local fallback (see quirks).

4. **winget submission** — submit a PR to `microsoft/winget-pkgs`:
   1. The 3 manifest files live in `packaging/winget/`. Update
      `PackageVersion` and `InstallerSha256` to match the new release. Get
      the Windows zip directly from the GitHub Release (do **not** rely on a
      local copy that might have been tampered with), then compute the
      sha256:
      ```bash
      gh release download vX.Y.Z -R koedame/chordsketch \
        -p chordsketch-vX.Y.Z-x86_64-pc-windows-msvc.zip
      sha256sum chordsketch-vX.Y.Z-x86_64-pc-windows-msvc.zip | awk '{print toupper($1)}'
      ```
      If `release.yml` ever starts publishing a checksums file alongside the
      archives, cross-check the value above against that file before
      committing it to the manifest.
   2. Fork `microsoft/winget-pkgs` (or use the existing fork). **Before
      branching, sync the fork's `master` with `microsoft/winget-pkgs:master`
      and skim `git log` for unexpected commits** — winget-pkgs is a
      high-traffic upstream and a stale or hijacked fork can quietly publish
      surprising history into your PR. Then clone with sparse checkout — the
      repo has 500K+ files and full clone exhausts filesystem inodes:
      ```bash
      git clone --filter=blob:none --no-checkout --depth 1 \
        https://github.com/<your-fork>/winget-pkgs.git
      cd winget-pkgs
      git sparse-checkout init --cone
      git sparse-checkout set manifests/k/koedame
      git checkout
      ```
   3. Copy the 3 manifest files into
      `manifests/k/koedame/chordsketch/X.Y.Z/` (the directory will not exist
      yet on the first release of a given version).
   4. Push to your fork. ⚠️ If your local SSH key is not for the same GitHub
      user that owns the fork, switch to HTTPS first:
      ```bash
      git remote set-url origin https://github.com/<your-fork>/winget-pkgs.git
      gh auth setup-git
      ```
      Note: `gh auth setup-git` writes credential-helper entries to your
      global `~/.gitconfig`. On a shared maintainer machine, scope it
      narrowly with `gh auth setup-git --hostname github.com`.
   5. Open the PR with `gh pr create -R microsoft/winget-pkgs --base master`.
   6. ⚠️ **First-time contributor**: a `microsoft-github-policy-service` bot
      will request CLA agreement via a comment. Reply on the PR with
      `@microsoft-github-policy-service agree` (no leading whitespace).
      Subsequent PRs from the same account skip this step.
   7. The validation pipeline runs in Azure DevOps. If any check is red,
      address the feedback in your fork branch and force-push.
   8. After Microsoft reviewer approves and merges, `winget install
      koedame.chordsketch` becomes available within minutes. The next
      `readme-smoke.yml` run will turn the `winget (Windows)` job green.

5. **MacPorts Portfile** — MacPorts does **not** have an automated update
   mechanism. After each release, the Portfile must be updated manually and
   submitted as a PR to `macports/macports-ports`:

   1. A reference Portfile lives at `packaging/macports/Portfile`. Update
      the `github.setup` version and the `checksums` block. To compute the
      checksums, download the source tarball that GitHub auto-generates for
      the tag:
      ```bash
      TAG=vX.Y.Z
      curl -L -o chordsketch-${TAG}.tar.gz \
        "https://github.com/koedame/chordsketch/archive/refs/tags/${TAG}.tar.gz"
      openssl dgst -rmd160 chordsketch-${TAG}.tar.gz
      openssl dgst -sha256 chordsketch-${TAG}.tar.gz
      wc -c chordsketch-${TAG}.tar.gz
      ```
   2. If the `cargo.crates` block needs updating (dependency versions
      changed), regenerate it using MacPorts' `cargo2port.py` tool from a
      local MacPorts install.
   3. Fork `macports/macports-ports` (or use the existing fork), place the
      Portfile in `textproc/chordsketch/Portfile`, and open a PR.
   4. Wait for MacPorts CI and maintainer review.

6. **Automated channel rollup** — `.github/workflows/release-verify.yml`
   runs automatically on `release: published`, queries every registry
   listed in `ci/release-channels.toml`, and appends a
   `## Channel Verification` section to the release body. Wait for that
   workflow to complete, then read the appended table on the GitHub Release
   page:

   ```bash
   gh release view vX.Y.Z -R koedame/chordsketch --web
   ```

   Every row must be green. Any ❌ is a release blocker: open a follow-up
   issue tagged with the failing channel and either fix it or mark the
   channel as an explicit `skip` in `ci/release-channels.toml` with a
   `skip_reason`. Do **not** close the release milestone until every row
   is resolved.

   Red-path dry-run: to confirm the rollup actually fails loudly when it
   should, trigger it manually with a forced-stale channel and verify the
   job turns red:

   ```bash
   gh workflow run release-verify.yml -R koedame/chordsketch \
     -f tag=vX.Y.Z -f force_stale_channel=crates-io-cli
   ```

7. **Manual verification** — confirm every documented install path works for
   end users. Easiest: trigger `readme-smoke.yml` via `workflow_dispatch` and
   confirm every job is green. `gh workflow run` does not print the run id,
   so resolve it from the workflow's most recent run before passing it to
   `gh run watch`:
   ```bash
   gh workflow run readme-smoke.yml -R koedame/chordsketch
   # The freshly triggered run can take 2-5 seconds to appear in the list
   # API. Without this pause, `gh run list --limit 1` can return the
   # *previous* run's id and you'd watch an already-completed run.
   sleep 5
   RUN_ID=$(gh run list --workflow=readme-smoke.yml -R koedame/chordsketch \
     --limit 1 --json databaseId --jq '.[0].databaseId')
   gh run watch "$RUN_ID" -R koedame/chordsketch
   ```
   Spot-check from a clean machine:
   - `cargo install chordsketch && chordsketch --version`
   - `brew tap koedame/tap && brew install chordsketch && chordsketch --version`
   - `docker run --rm ghcr.io/koedame/chordsketch:latest --version`
   - `docker run --rm docker.io/koedame/chordsketch:latest --version`
   - `npm install @chordsketch/wasm && node -e "import('@chordsketch/wasm').then(({version}) => console.log(version()))"`
   - `npm view @chordsketch/wasm version`
   - `winget install koedame.chordsketch && chordsketch --version` (after
     winget-pkgs PR merges)

## Required Secrets

| Secret | Scope | Purpose |
|--------|-------|---------|
| `TAP_GITHUB_TOKEN` | `contents:write` on `koedame/homebrew-tap` and `koedame/scoop-bucket` | Push updated formulae/manifests after release |
| `DOCKERHUB_USERNAME` | string | Docker Hub username under which images are pushed (currently `koedame`) |
| `DOCKERHUB_TOKEN` | Docker Hub Personal Access Token, "Read & Write" | Authenticate `docker push` against `docker.io/koedame/chordsketch` from `docker.yml` |
| `NPM_TOKEN` | npm Granular Access Token, scope `@chordsketch` Read & Write, org `chordsketch` Read & Write | ⚠️ Authenticate `npm publish` against the `@chordsketch/*` scope from `npm-publish.yml`. The org-level grant is the **empirically working** configuration, not necessarily the minimal one — see "npm publish via CI" quirk below for what we tried. Narrowing the scope is an open question; if you experiment with it, file a follow-up issue and link results back here. |
| `AUR_SSH_KEY` | ed25519 SSH private key registered with AUR account `koedame` | Authenticate `git push` to `ssh://aur@aur.archlinux.org/chordsketch.git` from `post-release.yml` |
| `SNAP_STORE_TOKEN` | Snapcraft exported credentials (`snapcraft export-login`) | Authenticate `snapcraft upload` + `snapcraft release` from `post-release.yml` |
| `COCOAPODS_TRUNK_TOKEN` | CocoaPods trunk session token (from `~/.netrc` after `pod trunk register`) | Authenticate `pod trunk push` from `post-release.yml` |
| `GITHUB_TOKEN` | provided automatically | Used by `docker.yml` to push to GHCR, by `release.yml` to upload assets, by `npm-publish.yml` checkout |

If any of these secrets are missing or wrong, the corresponding distribution
channel will silently break. The `report-failure` job in `readme-smoke.yml`
auto-creates an issue when smoke jobs fail (managed via the rolling tracking
issue titled "README install smoke tests are failing").

### Secret rotation

None of the tokens above are infinite-lived. A token that silently expires
mid-release surfaces the breakage at the worst possible time. Treat the
following as the rotation policy:

| Secret | Target cadence | Rotation UI |
|--------|----------------|-------------|
| `NPM_TOKEN` | Every 90 days, or immediately if the value has ever been pasted into chat / shared logs | <https://www.npmjs.com/settings/~/tokens> (sign in as the npm account that owns `@chordsketch`) |
| `DOCKERHUB_TOKEN` | Every 90 days | <https://hub.docker.com/settings/security> |
| `TAP_GITHUB_TOKEN` | Every 90 days, or whenever the issuing GitHub account changes 2FA / recovery setup | <https://github.com/settings/tokens> |
| `AUR_SSH_KEY` | Only if the key is compromised or the AUR account changes | <https://aur.archlinux.org/account/koedame> (replace SSH public key, then `gh secret set AUR_SSH_KEY < new_key`) |
| `SNAP_STORE_TOKEN` | Before expiry date (shown in `snapcraft whoami`; current session expires 2027-08-21) | `snapcraft export-login ~/snap-token.txt` then `gh secret set SNAP_STORE_TOKEN < ~/snap-token.txt` |
| `COCOAPODS_TRUNK_TOKEN` | Sessions last ~4 months; re-register if expired | `pod trunk register <email> <name>`, confirm email, then copy token from `~/.netrc` (`grep -A2 trunk.cocoapods.org ~/.netrc`) |
| `DOCKERHUB_USERNAME` | Only if the Docker Hub namespace owner changes | n/a (string, not a credential) |
| `GITHUB_TOKEN` | Provided automatically per workflow run; no rotation needed | n/a |

Procedure for any rotation:

1. Issue the new token with the **same scope** documented in the Required
   Secrets table above.
2. Update the repo secret: `gh secret set <NAME> -R koedame/chordsketch`.
3. Revoke the old token in the issuing UI.
4. Trigger a verification run and confirm the affected smoke job is green:
   ```bash
   gh workflow run readme-smoke.yml -R koedame/chordsketch
   ```

If a token must be rotated out-of-band (e.g., suspected leak), do steps 1-3
in the order listed — do **not** revoke before updating the secret, or the
next release will fail until you set the new value.

## Known Operational Quirks

These are non-obvious gotchas discovered during real publishing. They are not
derivable from the code; check this section before assuming the simple path
will work.

### npm publish via CI cannot create new packages (scoped or unscoped)

The CI publish workflows with the current Granular `NPM_TOKEN` cannot
create *new* packages — neither scoped (`@chordsketch/*`) nor unscoped
(e.g., `tree-sitter-chordpro`). Every attempt to publish a brand-new
package name returns:

```
npm error 404 Not Found - PUT https://registry.npmjs.org/<package-name>
npm error 404  '<package-name>@X.Y.Z' is not in this registry.
```

This was confirmed for both `@chordsketch/wasm` (2026-04-07) and
`tree-sitter-chordpro` (2026-04-14, #1744). The exact mechanism is
unclear (likely a Granular token permission gap not exposed in the npm
UI).

**Workaround** — manual local publish for the first version only:

```bash
cd <package-directory>
npm whoami              # must print the npm account that owns the package/scope
# if not logged in: npm login (interactive 2FA OTP via browser)
npm publish --access public
npm view <package-name> version    # verify
```

Once the package exists on the registry, the CI workflow handles all
subsequent version bumps automatically (confirmed with
`tree-sitter-chordpro` in #1744).

### New GHCR packages are private by default

`koedame` is a GitHub **organization**. The org-level "Public packages
allowed" setting has been enabled, but **each new GHCR package is still
created Private** and must be manually flipped to Public via the package
settings page after the first push:

```
https://github.com/orgs/koedame/packages/container/<package-name>/settings
→ Danger Zone → Change visibility → Public
```

This is the bug originally reported in issue #1001: the v0.1.0 image was
pushed to GHCR successfully but anonymous pull returned `unauthorized`
because the package was still private. The `readme-smoke.yml` `docker-ghcr`
job (added in #1012) now probes the HTTP layer with an anonymous bearer
token to detect this state immediately.

The visibility flip cannot be done via `gh api` with the standard `repo` /
`workflow` token scopes — it requires `admin:packages`, which is not granted
to the maintainer's default `gh` token. So this is a manual web-UI step on
every new package.

### winget-pkgs PRs need a CLA agreement on first contribution

The first time the submitting GitHub account opens a PR to
`microsoft/winget-pkgs`, the `microsoft-github-policy-service` bot will post
a comment requesting Contributor License Agreement signing. The CLA must be
agreed by replying to the PR with:

```
@microsoft-github-policy-service agree
```

(No leading whitespace. For employer-sponsored contributions, append
`company="<name>"`.) Subsequent PRs from the same account skip this step.

### npm package version may be skewed from workspace crates version

When a packaging-only fix is needed (the WASM library code is unchanged but
the npm wrapper needs a bump — e.g., the dual-package fix for the
broken-on-Node `0.1.0` build), the npm package version is allowed to be
**skewed** from the workspace crates version. The wasm `version()` function
exposed by the package returns the **Rust crate version**, not the npm
wrapper version, because it is compiled into the binary.

`@chordsketch/wasm@0.1.0` is published-but-broken-on-Node (the
`wasm-pack --target web` build calls `fetch()` on a `file://` URL which
Node's undici does not implement). It cannot be unpublished (>24h since
publish, npm policy). The fix is to publish `0.1.1` with the dual-package
layout. The `packages/npm/README.md` banner instructs end users to install
`>=0.1.1`. **Do not try to "fix" 0.1.0** — it is permanently broken on the
registry and the only mitigation is the `>=0.1.1` recommendation.

### `packaging/winget/` already contains manifest templates — copy them, do not re-author

The repo ships winget manifest templates at:

```
packaging/winget/koedame.chordsketch.installer.yaml
packaging/winget/koedame.chordsketch.locale.en-US.yaml
packaging/winget/koedame.chordsketch.yaml
```

The release flow is to **update `PackageVersion` and `InstallerSha256` in
these templates**, then copy them into the winget-pkgs PR. Do not re-author
the manifests from scratch — the templates are tuned (per-installer
`NestedInstallerType: portable`, `PortableCommandAlias: chordsketch`, etc.)
and easy to get subtly wrong.

### `report-failure` job has been silently broken since #1004 — fixed in #1031

Historical: from `readme-smoke.yml`'s introduction (#1004) until #1031, the
`report-failure` job that is supposed to auto-create / update a tracking
issue when smoke fails was itself broken — it had no `actions/checkout` and
the `gh issue` calls failed with `fatal: not a git repository`. This means
**no auto-tracking issues were created for any failure between those two
PRs**. Going forward, expect the rolling tracking issue titled "README
install smoke tests are failing" to actually be maintained.

## Version Skew Allowlist Procedure

`ci/version-skew-allowlist.toml` declares intentional drifts between the
canonical workspace crate version and specific manifests or pins in the
repo. The `version-consistency` CI job enforces that every versioned file
either matches canonical or has an entry here. This section describes the
lifecycle of an allowlist entry.

### When to add an entry

Add an entry only when:

1. A channel is **unpublished** and its package version is intentionally
   lagging until the first publish (e.g., the VS Code Marketplace case).
2. A package needs a **patch-only bump** for a packaging-only fix while the
   underlying library remains unchanged (e.g., `@chordsketch/wasm` dual-
   package fix).
3. A CI pin references a version that is **not yet resolvable from the
   registry** (e.g., `readme-smoke.yml` caret constraints point at what
   crates.io actually serves, which lags workspace during the bump-then-
   publish window).

**Do not** add an entry to hide a legitimate mistake (forgot to bump, copy-
paste error). Fix the source instead.

### How to add an entry

1. **File a `type:tracking` issue first.** The issue body must state:
   - Which file/field is drifting and why
   - What condition retires the skew
   - The plan for the PR that performs the retirement

   Labels: `type:tracking`, `size:small`, plus `blocked` if the retirement
   is waiting on independent work (e.g., first-time Marketplace publish).

2. **Add the allowlist entry** with all required fields:
   - `file` and `field` must match the labels emitted by
     `scripts/check-version-consistency.py` for the drifting source.
     Easiest: run the script, see it fail, copy the `(file, field)` pair
     from the error message.
   - `current_value` must exactly equal the literal string the source has
     right now.
   - `reason` must explain why the skew is tolerated. Multi-line OK.
   - `expires_at` must describe the condition that retires the entry in
     human-actionable terms (e.g., "first 0.2.x crates.io publish").
   - `tracking_issue` must be the GitHub issue number from step 1. A
     missing or empty `tracking_issue` fails the check script — this is
     the guardrail that prevents forgotten skips.

3. **Verify the check now passes**:
   ```bash
   python3 scripts/check-version-consistency.py
   ```

### How to retire an entry

When a condition like "next workspace release" or "first Marketplace
publish" is met, **the same PR that fulfils the condition must also
remove the allowlist entry AND close the tracking issue**. The check
script reports stale entries (entries that no longer match any real
source) as errors, so a half-finished retirement cannot silently slip
through.

Closing the tracking issue should reference the PR that fulfils the
condition so the rationale trail is navigable.

## When to update `README.md` `## Installation`

The project's contract with end users is `README.md ## Installation`. Any
change to a documented install method is a user-visible release-blocking
event. This section is enforced via `.claude/rules/readme-sync.md`.

Specifically:

- **New channel added.** Every add requires three concurrent touches in
  the same PR:
  1. `README.md` gets a new subsection under `## Installation`.
  2. `.github/snapshots/readme-commands.txt` is regenerated via
     `python3 scripts/extract-readme-commands.py > .github/snapshots/readme-commands.txt`.
  3. A new smoke job is added to `.github/workflows/readme-smoke.yml`
     that exercises the documented command(s) against the actual binary
     produced by the install (not just `--version`; include a real render
     assertion via the `cli-render-smoke` composite action).
  4. A new `[[channels]]` entry is added to `ci/release-channels.toml`
     so the post-release rollup covers it.

- **Channel removed.** Same three touches, but each is a deletion:
  remove the README subsection, regenerate the snapshot, delete the
  smoke job, and delete the `ci/release-channels.toml` entry (or mark
  it `expected_version = "skip"` with a `skip_reason` if removal is
  temporary).

- **Channel renamed.** Treat it as "remove old + add new" in the same
  PR.

Snapshot drift without corresponding smoke coverage defeats the purpose
of the rule. `readme-sync.yml` fails the PR if the snapshot is touched
without human attention, so a silent rename cannot sneak through.

## napi distribution (`@chordsketch/node`)

`@chordsketch/node` is the native Node.js addon built via napi-rs. It is
shipped as **six** npm packages in the napi-rs prebuilt-binary layout:

- `@chordsketch/node` — pure-JS resolver package that loads the right
  platform binary at runtime
- `@chordsketch/node-linux-x64-gnu`
- `@chordsketch/node-linux-arm64-gnu`
- `@chordsketch/node-darwin-x64`
- `@chordsketch/node-darwin-arm64`
- `@chordsketch/node-win32-x64-msvc`

All six must be published at the same version on every release, or the
resolver package's `optionalDependencies` will fail to install on the
affected platform. The `napi-node` rollup entry in
`ci/release-channels.toml` verifies every one of the six against the git
tag at release time.

### First-time manual publish

Because `NPM_TOKEN` in CI cannot create new packages in the
`@chordsketch` scope (see "npm publish via CI cannot create new packages"
quirk above), the **first publish of each of the six packages must be
done manually from a maintainer's local checkout**. After the first
publish, subsequent version bumps go through the CI publish job in
`.github/workflows/napi.yml`.

Procedure (from a clean local checkout of the target tag):

```bash
cd crates/napi

# Install napi-rs CLI (matches devDependencies in crates/napi/package.json).
npm install

# Build every supported target. This requires cross-compilers; easier to
# download the artifacts from the corresponding napi.yml run instead.
gh run download -R koedame/chordsketch \
  -D /tmp/napi-artifacts \
  $(gh run list -R koedame/chordsketch --workflow=napi.yml --branch vX.Y.Z \
      --limit 1 --json databaseId -q '.[0].databaseId')

# Move each downloaded .node into its platform directory under npm/.
# (The directory layout is created by `napi create-npm-dirs`; see
# crates/napi/npm/ in the committed tree.)
for triple in linux-x64-gnu linux-arm64-gnu darwin-x64 darwin-arm64 win32-x64-msvc; do
  cp /tmp/napi-artifacts/napi-${triple}/*.node npm/${triple}/
done

# Authenticate as the npm account that owns @chordsketch.
npm whoami

# Publish the FIVE platform packages FIRST. Order within this group
# doesn't matter, but all five must succeed before the resolver.
for triple in linux-x64-gnu linux-arm64-gnu darwin-x64 darwin-arm64 win32-x64-msvc; do
  (cd npm/${triple} && npm publish --access public)
done

# Publish the main resolver package LAST. Its `optionalDependencies`
# field references the five platform packages, so it must be published
# after them or installs will fail with ENOENT.
napi prepublish --skip-gh-release --tagstyle npm
npm publish --access public
```

After the first successful manual publish, future releases are automatic
via `napi.yml`'s publish job (no human action required unless that job
fails and fallback to manual is needed).

### Why the decision to ship napi (vs. defer)

`crates/napi` predates the current release-discipline work: the Rust code
and the `napi build` pipeline were already in place, but no publish job
existed and no `@chordsketch/node` package had ever been claimed on npm.
During #1506 the decision was to ship rather than defer, because:

1. `@chordsketch/wasm` already serves Node.js via its `node` export
   condition, so the native addon is purely a performance improvement
   — but leaving the code unpublished creates a "code exists, no one can
   use it" state that future contributors would find confusing.
2. The first-publish-is-manual constraint is identical to the one already
   accepted for `@chordsketch/wasm`, so there is no new operational cost.
3. Registering the six package names on npm now prevents a future squat
   attack on `@chordsketch/node-*`.

If subsequent napi publishes become problematic and the maintenance cost
exceeds the value, the channel can be downgraded by flipping every napi
entry in `ci/release-channels.toml` to `expected_version = "skip"` with a
`skip_reason` — that is the supported way to pause a channel without
deleting its infrastructure.

## Adding a New npm Package

When adding a new npm package to the project (scoped or unscoped), the
following procedure sets up automated CI publishing. This was established
during `tree-sitter-chordpro` (#1744) and applies to any future npm
package.

### Step-by-step

1. **Create the publish workflow** at
   `.github/workflows/npm-publish-<name>.yml`:
   - Use `npm-publish.yml` (the `@chordsketch/wasm` workflow) as a
     template
   - Triggers: `release: [published]` and `workflow_dispatch` with a
     `version` input
   - Environment: `npm` (gates access to `NPM_TOKEN`)
   - Include the duplicate-publish check (skip if version already exists)
   - Use `--access public` on the `npm publish` command
   - If no build step is needed (e.g., pre-committed generated files),
     omit the build steps

2. **Add a channel entry** to `ci/release-channels.toml` **and** a
   matching row to the Distribution Channels table in `docs/releasing.md`
   (both must stay in sync):
   ```toml
   [[channels]]
   id = "npm-<short-name>"
   display = "npm — <package-name>"
   kind = "npm"
   package = "<package-name>"
   expected_version = "tag"
   required_secrets = ["NPM_TOKEN"]
   ```

3. **Add the package to the version bump list** in `docs/releasing.md`
   under "Non-Rust manifests" in the Release Checklist.

4. **Add to version-consistency tracking** — two files must be updated:
   - `scripts/check-version-consistency.py`: add a
     `load_package_json_version()` call in `load_all_sources()`
   - `scripts/test_check_version_consistency.py`: add the package to the
     `_build_repo()` fixture builder so unit tests create the file in
     their temp directories

5. **Sync the package version** with the workspace version (run
   `python3 scripts/check-version-consistency.py` to find the canonical
   version), or add an entry to `ci/version-skew-allowlist.toml` if the
   skew is intentional.

6. **Regenerate any derived files** if the version is embedded in them
   (e.g., tree-sitter `src/parser.c` includes the version from
   `package.json`; run `tree-sitter generate` after bumping).

7. **Merge the PR** with the workflow and infrastructure changes.

8. **Manually publish the first version** — the CI Granular token cannot
   create new packages (see "Known Operational Quirks" above):
   ```bash
   cd <package-directory>
   npm publish --access public
   ```

9. **Verify CI works** by re-triggering the workflow with the same
   version. It should succeed and skip the publish (already exists):
   ```bash
   gh workflow run npm-publish-<name>.yml \
     -R koedame/chordsketch -f version=X.Y.Z
   ```

## First-Time Channel Setup

These procedures document how each distribution channel was initially
set up. They are needed only once per channel; subsequent releases are
automated via `post-release.yml` or dedicated publish workflows.

### AUR (Arch Linux)

Set up on 2026-04-15. Automated via `post-release.yml` `update-aur`.

1. Create an account at <https://aur.archlinux.org/register>.
   The CAPTCHA answer can be computed with:
   ```bash
   docker run --rm archlinux:latest bash -c \
     "LC_ALL=C pacman -V|sed -r 's#[0-9]+#aeb#g'|md5sum|cut -c1-6"
   ```
2. Generate an SSH key and register the public key in the AUR account:
   ```bash
   ssh-keygen -t ed25519 -f ~/.ssh/aur_key -C "aur" -N ""
   # Paste ~/.ssh/aur_key.pub into AUR account → SSH Public Key
   ```
3. Clone the (empty) AUR package repo, generate PKGBUILD + .SRCINFO,
   and push. AUR only accepts the `master` branch:
   ```bash
   GIT_SSH_COMMAND="ssh -i ~/.ssh/aur_key" \
     git clone ssh://aur@aur.archlinux.org/chordsketch.git /tmp/aur
   cd /tmp/aur
   # Download checksums from the GitHub release
   gh release download vX.Y.Z -R koedame/chordsketch -p checksums.txt
   SHA=$(grep "x86_64-unknown-linux-gnu" checksums.txt | awk '{print $1}')
   # Generate PKGBUILD from template
   sed -e "s/{{VERSION}}/X.Y.Z/g" \
       -e "s/{{SHA256_X86_64_UNKNOWN_LINUX_GNU}}/$SHA/g" \
       packaging/aur/PKGBUILD.template > PKGBUILD
   # Generate .SRCINFO (see post-release.yml for the exact format)
   # Commit and push to master
   git add PKGBUILD .SRCINFO
   git commit -m "Initial upload: X.Y.Z"
   GIT_SSH_COMMAND="ssh -i ~/.ssh/aur_key" git push
   ```
4. Store the SSH private key as a GitHub secret:
   ```bash
   gh secret set AUR_SSH_KEY -R koedame/chordsketch < ~/.ssh/aur_key
   ```

### Snap Store

Set up on 2026-04-15. Automated via `post-release.yml` `update-snap`.

Uses **strict confinement** with `home` + `removable-media` plugs
(classic confinement requires Snap Store manual review and is not
needed for a file-processing CLI).

1. Create an Ubuntu One account at <https://login.ubuntu.com>.
2. Export login credentials:
   ```bash
   snapcraft export-login ~/snap-token.txt
   ```
3. Register the snap name:
   ```bash
   SNAPCRAFT_STORE_CREDENTIALS="$(cat ~/snap-token.txt)" \
     snapcraft register chordsketch
   ```
4. Build and upload the snap:
   ```bash
   mkdir -p /tmp/snap-build/stage /tmp/snap-build/snap
   cd /tmp/snap-build
   # Download and extract the prebuilt binary
   gh release download vX.Y.Z -R koedame/chordsketch \
     -p "chordsketch-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz"
   tar xzf chordsketch-vX.Y.Z-*.tar.gz --strip-components=1 -C stage
   chmod +x stage/chordsketch
   # Generate snapcraft.yaml from template
   sed -e "s/{{VERSION}}/X.Y.Z/g" \
     packaging/snap/snapcraft.yaml.template > snap/snapcraft.yaml
   # Build and upload
   snapcraft --destructive-mode
   SNAPCRAFT_STORE_CREDENTIALS="$(cat ~/snap-token.txt)" \
     snapcraft upload chordsketch_X.Y.Z_amd64.snap --release=stable
   ```
5. Store the credentials as a GitHub secret:
   ```bash
   gh secret set SNAP_STORE_TOKEN -R koedame/chordsketch < ~/snap-token.txt
   ```

### CocoaPods

Set up on 2026-04-15. Automated via `post-release.yml` `update-cocoapods`.

The pod ships a prebuilt XCFramework (same artifact as the Swift package).

1. Install CocoaPods: `gem install cocoapods`
2. Register a trunk session:
   ```bash
   pod trunk register <email> <name>
   # Click the confirmation link in the email
   ```
3. Generate and push the podspec:
   ```bash
   sed -e "s/{{VERSION}}/X.Y.Z/g" \
     packaging/cocoapods/ChordSketch.podspec.template > ChordSketch.podspec
   pod trunk push ChordSketch.podspec --allow-warnings
   ```
4. Store the trunk token as a GitHub secret. The token is in `~/.netrc`:
   ```bash
   TOKEN=$(grep -A2 trunk.cocoapods.org ~/.netrc | grep password | awk '{print $2}')
   echo "$TOKEN" | gh secret set COCOAPODS_TRUNK_TOKEN -R koedame/chordsketch
   ```
