# Versioning and Release Process

## Versioning Policy

All eight Rust crates in the workspace share the same version number and are
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

1. **Update version** in all `Cargo.toml` files:
   - `crates/core/Cargo.toml`
   - `crates/render-text/Cargo.toml`
   - `crates/render-html/Cargo.toml`
   - `crates/render-pdf/Cargo.toml`
   - `crates/cli/Cargo.toml`
   - `crates/wasm/Cargo.toml`
   - `crates/ffi/Cargo.toml`
   - `crates/napi/Cargo.toml`
   - Update inter-crate dependency versions to match

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

| Channel | Identifier | Trigger | Required secret(s) | Verified by |
|---|---|---|---|---|
| crates.io | `chordsketch` (CLI) + 4 lib crates | manual `cargo publish` (Step 6) | maintainer's `~/.cargo/credentials` | `cargo-install` job |
| GitHub Releases | binary archives | `release.yml` on tag push | `GITHUB_TOKEN` | `source-build` job |
| GHCR | `ghcr.io/koedame/chordsketch` | `docker.yml` on `release: published` | `GITHUB_TOKEN` (push), org policy must allow public packages | `docker-ghcr` job |
| Docker Hub | `docker.io/koedame/chordsketch` | `docker.yml` on `release: published` | `DOCKERHUB_USERNAME`, `DOCKERHUB_TOKEN` | `docker-hub` job |
| npm | `@chordsketch/wasm` | `npm-publish.yml` `workflow_dispatch` (Step 7) | `NPM_TOKEN` (Granular Token, see quirks) | `npm-wasm` job |
| Homebrew tap | `koedame/tap/chordsketch` | `post-release.yml` on `release: published` | `TAP_GITHUB_TOKEN` | `homebrew` job |
| Scoop bucket | `koedame/scoop-bucket/chordsketch` | `post-release.yml` on `release: published` | `TAP_GITHUB_TOKEN` | `scoop` job |
| winget | `koedame.chordsketch` | manual PR to `microsoft/winget-pkgs` (Step 8) | none (uses your `gh` token to fork+push) | `winget` job |
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

5. **Manual verification** — confirm every documented install path works for
   end users. Easiest: trigger `readme-smoke.yml` via `workflow_dispatch` and
   confirm every job is green. `gh workflow run` does not print the run id,
   so resolve it from the workflow's most recent run before passing it to
   `gh run watch`:
   ```bash
   gh workflow run readme-smoke.yml -R koedame/chordsketch
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
| `NPM_TOKEN` | npm Granular Access Token, scope `@chordsketch` Read & Write, org `chordsketch` Read & Write | Authenticate `npm publish` against the `@chordsketch/*` scope from `npm-publish.yml`. The org-level grant is the **empirically working** configuration, not necessarily the minimal one — see "npm publish via CI" quirk below for what we tried. Narrowing the scope is an open question; if you experiment with it, file a follow-up issue and link results back here. ⚠️ See "npm publish via CI" quirk below |
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

### npm publish via CI cannot create new packages in `@chordsketch` scope

The CI `npm-publish.yml` workflow with the current Granular `NPM_TOKEN`
cannot create *new* packages under the `@chordsketch` scope, only update
existing ones. Every attempt to publish a brand-new package name returns:

```
npm error 404 Not Found - PUT https://registry.npmjs.org/@chordsketch%2f<name>
npm error 404  '@chordsketch/<name>@X.Y.Z' is not in this registry.
```

This persists even with a Granular Token configured for **scope-level
read+write on `@chordsketch`** AND **read+write on the `chordsketch` org**.
The exact mechanism is unclear (likely a Granular token permission gap not
exposed in the npm UI).

**Workaround** — manual local publish from a machine authenticated to npm
as the account that owns the `@chordsketch` scope:

```bash
cd packages/npm
npm run build           # builds web/ + node/ subdirs (dual package)
npm whoami              # must print the npm account that owns @chordsketch
# if not logged in: npm login (interactive 2FA OTP via browser)
npm publish
npm view @chordsketch/wasm version    # verify
```

After the first version of a package exists, subsequent version bumps **may**
work via CI but should be tried with low expectations and manual fallback
available. As of the 2026-04-07 session, even an existing-package version
bump (`@chordsketch/wasm@0.1.0` → `0.1.1`) failed via CI with the same 404
and was published manually.

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
