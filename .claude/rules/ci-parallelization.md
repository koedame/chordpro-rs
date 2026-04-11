# CI Parallelization and Fan-in Discipline

Codifies the patterns discovered during the CI parallelization audit
(see issues #1461–#1465). Apply these rules when adding or modifying any
`.github/workflows/*.yml` file.

## 1. Fan-in discipline

A job's `needs:` MUST list only the jobs whose artifacts the downstream job
actually downloads or depends on. Do NOT use `needs:` as a coarse
"wait for phase X" barrier.

**Ask for every `needs:` edge:** "What file does this job read that would not
exist without the upstream job?" Only jobs that produce such files belong in
`needs:`.

**Antipattern:** if job B consumes only the linux-x64 artifact, it MUST NOT
`needs:` Windows or macOS build cells — those cells produce artifacts B never
reads, so the edge only adds unnecessary blocking time.

**Worked example (ruby.yml, kotlin.yml after #1461):**
```yaml
generate-and-test:
  needs: [build-native-linux-x64]   # only linux-x64 artifact is consumed here
  ...

publish:
  needs: [build-native-linux-x64, build-native-others, generate-and-test]
  # publish needs ALL artifacts, so all edges are legitimate
```

## 2. `rust-cache` — frequency-aware

Every Rust-compiling job (any step that invokes `cargo build/test/clippy/doc`,
or a tool that internally runs cargo: maturin, napi-rs, wasm-pack, cross) SHOULD
include `Swatinem/rust-cache` with a meaningful `shared-key` — **unless** the
workflow runs less often than once per 7 days.

GitHub Actions cache entries are evicted by LRU after 7 days of inactivity.
Cache additions to infrequent workflows deliver no wall-clock benefit and add
YAML noise.

**Required (workflows that run at least weekly):**
`ci.yml`, `wasm.yml`, `python.yml`, `ruby.yml`, `kotlin.yml`, `swift.yml`,
`napi.yml`, `ffi.yml`, `deploy-playground.yml`

**Intentionally omitted (infrequent — cache expires before next run):**
- `release.yml` — ~every 10 days on version tags
- `npm-publish.yml` — a few times per month
- `extended-tests.yml` — dispatch-only, rarely invoked

Workflows in the "intentionally omitted" category SHOULD have a YAML comment
explaining the frequency-based rationale so future contributors do not
re-propose adding cache.

The `shared-key` SHOULD include the target triple when the job compiles for a
specific target, to avoid cache thrashing across targets:
```yaml
- uses: Swatinem/rust-cache@...
  with:
    shared-key: kotlin-x86_64-unknown-linux-gnu
```

## 3. Public-repo parallelism

Because this repository is public, runner minutes are free. When a job performs
N independent units of work that each take non-trivial time, prefer a matrix
split with N runners over a sequential loop — **unless** measured evidence shows
that warm-cache sharing between units dominates per-runner setup overhead.

`swift.yml` is a documented case where this trade-off is non-obvious: the
workflow runs approximately every 4 hours (warm cache almost always), so 5
sequential targets on one runner complete in ~31 s while 5 parallel cells would
each pay runner boot + cache restore independently. Any proposed matrix split for
`swift.yml` MUST measure both the cold-cache and warm-cache cases before merging.
See #1465 for the tracking issue.

## 4. Measure before shipping workflow performance changes

PRs that claim to improve CI wall-clock MUST include before/after numbers from
representative runs (one per workflow touched), gathered via:

```bash
gh run view <run-id> --json jobs -R koedame/chordsketch
```

A PR that says "this makes CI faster" without concrete numbers is not
reviewable.

## 5. Workflow frequency is a first-class design input

Before adding a cache layer, a matrix split, or any optimization to a workflow,
verify how often it actually runs:

```bash
gh run list --workflow <file>.yml -R koedame/chordsketch --limit 100 --json createdAt
```

If the workflow runs less than weekly, most caching optimizations will be
ineffective — the cache expires before the next run uses it.
