# PLAN_REVIEW_A — Bismark Rust suite packaging & distribution (rev 2)

**Reviewer:** Plan Reviewer A (independent, fresh context)
**Plan:** `plans/06062026_rust-suite-packaging/PLAN.md` rev 2
**Date:** 2026-06-06
**Verdict:** **APPROVE-WITH-CHANGES**

This is a sound *strategic* kickoff. The decisions (D1–D5, lifecycle, OQ-A/B/C) are well-reasoned and the Trim Galore template is the right north star. But the plan is written one altitude above the mechanics, and the mechanics it glosses over are exactly the irreversible, hard-to-reverse ones (crates.io immutability, tag namespace, version-source-of-truth). Several literal TG snippets it proposes to "adopt verbatim" or "adapt" **do not work** against a virtual workspace manifest and will fail or — worse — silently mis-detect the version. None of this sinks the plan; all of it must be nailed in the SPEC before `release.yml` is written. Hence APPROVE-WITH-CHANGES, not APPROVE.

**Findings: 4 Critical · 7 Important · 5 Optional**

---

## The single most important thing to fix

**Define the single source of truth for "the suite version" and the exact tag/version-detection mechanics — TG's `release.yml` reads `^version` from the top-level `Cargo.toml` and asserts a `name = "trim-galore"` line in `Cargo.lock`. Neither exists in a virtual workspace.** §4 says "version from `workspace.package.version`" but `rust/Cargo.toml` has **no `version` key today** (verified) and is a virtual manifest with no `[package]`. The verbatim TG grep (`grep '^version' Cargo.toml`) will match **nothing** (fail) or, if naively pointed at a crate, match the wrong crate. The plan must specify: (a) add `version = "X.Y.Z"` to `[workspace.package]`, (b) the new grep target (`rust/Cargo.toml`'s `[workspace.package]` block, not line-1 `^version`), and (c) what the `Cargo.lock == Cargo.toml` assertion checks now that there's no single `name = "..."` to anchor on. Get this wrong and you either can't cut a release or you tag the wrong number.

---

## Critical findings

### C1. Version-source / lock-assertion mechanics don't port from a single-crate to a workspace
TG's `check-release` job does two things the plan inherits but cannot reuse as-is (verified against `~/Github/TrimGalore/.github/workflows/release.yml` lines 71, 104):

```sh
RAW_VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
LOCK_VERSION=$(awk '/^name = "trim-galore"/{found=1} found && /^version =/{print; exit}' Cargo.lock ...)
```

- `rust/Cargo.toml` is a **virtual workspace** — `version` lives under `[workspace.package]`, not at file top, and `grep '^version'` over it currently matches **nothing** (confirmed: no `version` key present). The plan's §4a step 5 ("set `workspace.package.version`") is the right fix, but §4/§5 still describe "adopt/adapt verbatim" — the grep and the awk **must be rewritten** for the workspace shape, and the SPEC must show the exact replacement.
- The `Cargo.lock == Cargo.toml` invariant is the plan's stated safety gate (§ "Scrutinize: irreversibility"). With a hybrid versioning model there are now **two** versions to assert: the suite version (does any *binary* crate's locked version match `workspace.package.version`? — but only if they use `version.workspace = true`) **and** `bismark-io`'s independent version. The plan does not say which lock entries get asserted. Spell out both assertions or the gate gives false confidence.

### C2. The suite tag format is unspecified and collides with two existing tag namespaces
The plan never states what the workflow-owned tag will be. TG uses `v$RAW_VERSION` (e.g. `v2.2.0`). Applied here that produces `vX.Y.Z`, which **collides head-on with the Perl Bismark tag namespace** — the repo already has `v0.24.2`, `v0.25.0`, `v0.25.1` (verified). At GA the plan wants `bismark` 2.0.0 → tag `v2.0.0`, which is fine (Perl never reached v2), but the **beta `bismark-rust` track** needs tags *now*, and a bare `vX` scheme is ambiguous against the Perl line and against the existing per-crate tags `bismark-io-v1.0.0-beta.6`, `bismark-dedup-v1.2.1-beta.1`, etc. (all verified present). Decide and document the beta tag prefix (e.g. `bismark-rust-vX.Y.Z-beta.N` or `suite-vX`) and confirm the GA `v2.0.0` plan won't be mistaken for a Perl tag. This is irreversible-once-pushed (outward-facing) and the workflow's "tag already exists" guard won't save you from a *semantic* collision.

### C3. crates.io "keep current" is messier than "publish the catch-up" — there are gaps and the exact-pin couples everything
OQ-C says local `beta.8` vs published `beta.6` → "publish the catch-up." Reality (verified via crates.io API): **only `1.0.0-beta.1`, `beta.5`, `beta.6` are published** — beta.2/3/4 and **beta.7 were never published** (local versions were bumped without releasing). So:
- "Publish the catch-up" = publish **beta.7 and beta.8** as fresh immutable versions. beta.7 may never have existed as a coherent published artifact; you'd be minting it now. Confirm beta.7 is a real, buildable point or **skip straight to beta.8** (crates.io does not require contiguous versions).
- Every binary crate pins `bismark-io = "=1.0.0-beta.8"` (exact-pin, verified across all 9 binary crates that depend on it). The moment `release.yml` publishes `bismark-io` at some new version, **the next workspace bump must update the exact-pin in 9 crates atomically** or the workspace won't resolve against the published crate. The plan's "no-bump-during-beta convention" (OQ-C note) directly conflicts with "automate ongoing `bismark-io` publishes" — you cannot publish a *new* immutable crates.io version without bumping `bismark-io`'s version (re-publishing `beta.8` is rejected as a duplicate). State the rule precisely: **a `bismark-io` publish REQUIRES a `bismark-io` version bump REQUIRES updating the `=` pin in all dependents in the same commit.** This is the single most error-prone loop in the whole plan.

### C4. `build.rs` "port verbatim + wire into every binary's `--version`" understates a real, per-crate task
TG has **one** binary with clap auto-version. Bismark has **12 binaries, each with clap auto-version disabled and a hand-rolled `version_string()`** (verified: `bismark-aligner`, `bismark-dedup`, `bismark-extractor`, `bismark-bam2nuc` all do `// --version handled manually`). Each `version_string()` currently uses `env!("CARGO_PKG_VERSION")` (its **own crate version**, not a suite version) and a **different banner format** (e.g. dedup: `deduplicate_bismark_rs <ver> (<os>/<arch>)`; aligner prints a Bismark banner). Consequences the plan must address:
- A workspace `build.rs` only sets env vars for crates that **have** a `build.rs` (or one is added per crate / the env is threaded). A single `rust/build.rs` at the workspace root is **not** automatically run for member crates — build scripts are per-package. The plan says "workspace-shared `build.rs`"; mechanically that means either a shared file `include!`d/`path`-referenced by 12 per-crate `build.rs` files, or a tiny `build.rs` in each crate. Specify which.
- After D1 (hybrid), `--version` becomes self-contradictory: dedup's `version_string()` prints `CARGO_PKG_VERSION` = `1.2.1-beta.1` (its independent crate version) but the suite/tarball is `bismark-rust X.Y.Z`. Users running `deduplicate_bismark_rs --version` will see a number that matches **no release tag**. Decide: does each bin print the suite version, its crate version, or both? `bismark-dedup/src/lib.rs:52` even has a TODO: *"Phase G will extend this with git commit hash and ISO-8601 build timestamp via a build.rs step"* — this work is acknowledged-but-unscheduled and is bigger than "verbatim."

---

## Important findings

### I1. Cutting releases from `rust/iron-chancellor` vs `master` — the plan inherits a guard it then contradicts
TG's workflow **hard-codes** `master` (GA) / `dev` (prerelease) as the only allowed release refs (lines 75–82) and **errors otherwise**. The plan (OQ-B) wants to cut from `rust/iron-chancellor`. So the verbatim guard would **reject every release**. The SPEC must rewrite that branch allow-list to `rust/iron-chancellor` (or a release branch off it). Independently: releasing from a long-lived integration branch is *defensible for a beta track* (it's where the code is) but is a hazard because (a) it's force-push/rebase-prone per your own workflow history, and a tag pointing at a rebased-away commit is dangling; (b) it diverges from `master` where Perl ships — a user `git clone`ing `master` won't see the Rust release source at the tagged ref unless the tag is on a ref reachable from a stable branch. Recommend: cut from a **short-lived `release/bismark-rust-X.Y.Z` branch** snapshotted off iron-chancellor (immutable, won't be rebased), tag there. Document the branch hygiene rule.

### I2. Docker is "optional/variant" in the matrix but is effectively *mandatory* for usability — and that changes the smoke test
TG could bundle FastQC into its binary (Dockerfile line 26–28 installs only `procps`/`ca-certificates`). Bismark **cannot bundle Bowtie2/HISAT2/samtools** (§2 acknowledges this). So the batteries-included image is not a nicety — for most users it's the only turnkey way to run the suite. Implications the plan omits:
- The Docker **smoke test must verify the aligner + samtools are present and runnable** (`bowtie2 --version`, `samtools --version`), not just `<bin> --help`. TG's smoke test (lines 329–332) only checks the binary; copying it verbatim would ship a "batteries-included" image that's silently missing its batteries.
- **End-to-end smoke**: ideally run a tiny genome-prep → align → extract chain in-image. At minimum assert the externals resolve. The plan's "smoke tests (`--help`/`--version` per bin)" is necessary but **insufficient** for the value proposition of D5(b).

### I3. License/redistribution of the bundled aligners — clean, but must be stated, and pinned
Verified licenses: **Bowtie2 GPL-3.0**, **HISAT2 GPL-3.0**, **samtools/htslib MIT**, **minimap2 MIT**. The crate is **GPL-3.0-only**. Bundling GPL-3.0 aligners into a GPL-3.0 image is **license-compatible** (no conflict; you must ship the corresponding source offer / point to upstream). MIT tools are permissive. So there's **no blocker**, but the SPEC should: (a) record the per-tool license + version pin in the Dockerfile and image labels, (b) note the GPL "offer of source" obligation for redistributed Bowtie2/HISAT2 binaries (apt packages satisfy this via Debian, but a hand-built aligner does not — prefer distro packages), (c) pin aligner versions for byte-identity (the aligner port is gated against **Bowtie2 2.5.5** per `aligner.rs:14` `PINNED_BOWTIE2_VERSION` — the image MUST install exactly 2.5.5 or the byte-identity guarantee the whole project rests on evaporates, yet the plan says only "at least Bowtie 2"). **This is nearly Critical**: a batteries-included image that ships a non-2.5.5 Bowtie2 would emit a runtime warning (verified `aligner.rs:74`) and produce non-byte-identical output — undermining the project's core claim. Pin it explicitly and assert in the smoke test.

### I4. Image size / which aligners — decision deferred but cost not estimated
Bookworm-slim + samtools + Bowtie2 + HISAT2 + minimap2 + 12 stripped Rust bins is plausibly 400 MB–1 GB+. The plan defers variant images "later" but doesn't size the default. Recommend: default image = samtools + Bowtie2 only (the gated, supported path); HISAT2/minimap2 are **v1.x aligner features not yet shipped** (aligner is `1.0.0-alpha.1`, Bowtie2-only per the memory) — bundling them now ships dead weight for tools the Rust aligner can't yet drive. Trim the default to what the code supports today.

### I5. Smoke-testing 12 bins vs 1 — matrix coverage gap
TG smoke-tests only the **linux-x86_64** binary (lines 191–202) and only that one. For a 12-bin suite the plan says "`--help`/`--version` per bin" but doesn't say **per target**. The macOS-aarch64 and linux-aarch64 tarballs are the ones most likely to have a cross-build/link problem, yet TG's pattern never tests them. Specify: loop all 12 bins' `--help`/`--version` on **at least linux-x86_64**, and ideally a reduced check on the other two targets (GH-hosted arm64 + macos runners exist in the matrix; reuse them). Also note `--help`/`--version` is a weak smoke for a tool that shells out — see I2.

### I6. macOS-aarch64 unsigned-binary Gatekeeper friction is unaddressed
TG ships an unsigned `macos-aarch64` binary; users hit Gatekeeper quarantine (`killed: 9` / "cannot be opened"). For a **12-binary** tarball this is 12 separate `xattr -d com.apple.quarantine` papercuts. The plan inherits the unsigned approach silently. At minimum document the `xattr` workaround in the release notes / README; ideally note codesigning/notarization as a GA item. Not a blocker for beta but will generate user friction the plan should anticipate.

### I7. Token scopes / OIDC publish — verify the trusted-publisher mapping before relying on it
TG uses `rust-lang/crates-io-auth-action` (OIDC, line 409) — no stored `CARGO_REGISTRY_TOKEN`. For this to work, **crates.io must have a Trusted Publisher configured for `bismark-io`** pointing at `FelixKrueger/Bismark` + the workflow file + the environment. `bismark-io` was previously published (beta.1/.5/.6) — confirm *how* (manual token vs OIDC); if those were manual `cargo publish` runs, the Trusted Publisher binding may not exist and the first automated publish will fail auth. The plan assumes OIDC "just works" by copying TG. Add a SPEC pre-req: configure/verify the crates.io Trusted Publisher for `bismark-io`.

---

## Optional / smaller findings

### O1. `cargo publish --no-verify` for a workspace
TG publishes one crate with `--no-verify` (line 412). Publishing `bismark-io` from the workspace root with `cargo publish -p bismark-io` is needed (bare `cargo publish` in a virtual workspace errors "could not find `Cargo.toml` ... virtual manifest"). Specify `-p bismark-io`. `--no-verify` is fine but means the published crate is never test-compiled in isolation — acceptable for a lib that CI already builds.

### O2. `[package].exclude` hygiene for the published crate
TG's `Cargo.toml` excludes `test_files/`, `plans/`, `.github/` from the crate package (lines 14–22). `bismark-io/Cargo.toml` has **no `exclude`** — its `cargo publish` tarball may carry test fixtures/large files. Add an `exclude` to `bismark-io` before automating publishes (keeps the crate small; immutable once published).

### O3. `justfile` "verbatim" needs 12-bin adaptation
TG's `justfile` `version`/`reproduce`/`validate-paired-end` targets are single-binary (`./target/release/trim_galore`). The Bismark `reproduce` target must diff **all 12** bins (or pick the largest), and `version` must loop. "Adopt verbatim" (§3.3) won't run. Minor, but flag it.

### O4. `Cargo.lock` is tracked (good) — keep it asserted in CI too
Verified `rust/Cargo.lock` is git-tracked. The `--locked` builds depend on it being current. Recommend the `release.yml` (or rust_ci.yml) run `cargo update --locked --dry-run` or `cargo verify-project` so a stale lock is caught *before* a release rather than failing the `--locked` build mid-release.

### O5. Reproducible-build epoch source for a 12-bin suite
TG's `build.rs` reads `SOURCE_DATE_EPOCH` (build.rs lines 17–32). For bit-identity across the suite, the release workflow must export one fixed `SOURCE_DATE_EPOCH` (e.g. the tag commit's `git log -1 --format=%ct`) for the whole `cargo build --release` so all 12 bins share a timestamp. The plan mentions the stamping but not that the *release build* sets the epoch. Note it.

---

## Assumptions to validate before SPEC
1. **`version.workspace = true` will be added to all 12 binary crates** (none use it today — verified). This is a non-trivial edit and changes every crate's published/printed version. Confirm Felix wants the 12 bins to *lose* their independent versions (dedup is `1.2.1-beta.1`, aligner `1.0.0-alpha.1` — these become the suite version).
2. **`bismark-io` keeps `version = "x"` literal (not `version.workspace`)** under D1(c) — correct, and the plan says so. Good.
3. **The aligner is Bowtie2-only today** (`1.0.0-alpha.1`); HISAT2/minimap2 are v1.x/future. Don't bundle what can't be driven (see I4).
4. **Releases cut from iron-chancellor will have a stable ref** — see I1; assumed false unless a snapshot/release branch is used.

## What TG's release.yml does that the plan should explicitly carry over
- `concurrency` group + `cancel-in-progress` (line 39) — prevents two releases racing. Plan omits.
- `permissions: contents: write, packages: write` at workflow level + `id-token: write` scoped to the publish job only (lines 35–37, 398–400) — least-privilege. Plan omits.
- Pinned action SHAs (every `uses:` is SHA-pinned) — supply-chain hygiene. Plan should require it.
- Digest-based multi-arch Docker merge (jobs 2/3) — the plan says "same digest→manifest pattern," good; just ensure the GHCR tag set is adapted (TG's `latest`/`beta`/`dev` map cleanly to your beta track).
- The release-notes generation (`gh release create --generate-notes`) — fine to inherit.

## Verdict
**APPROVE-WITH-CHANGES.** The strategy is sound and the template is well-chosen. Before anyone writes `release.yml`, the SPEC must close C1–C4 (version source-of-truth + lock assertion; suite tag format; the crates.io gap/exact-pin coupling rule; per-bin `--version` wiring across 12 hand-rolled printers) and pin the Bowtie2 2.5.5 version in the Docker image (I3). The remaining Important items are real but mechanical and can be resolved in the SPEC pass.
