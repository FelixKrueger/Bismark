# Code Review A — Bismark Rust suite packaging / release infrastructure

**Reviewer:** Code Reviewer A (independent, fresh context)
**Branch/worktree:** `rust/suite-packaging` @ `/Users/fkrueger/Github/Bismark-packaging` (off `iron-chancellor 8daa55d`, UNCOMMITTED)
**Scope:** versioning foundation (`bismark-meta`), `_rs` normalization, release infra (`release.yml`, `Dockerfile`, `justfile`, `THIRD-PARTY-NOTICES.md`)
**Plan:** `plans/06062026_rust-suite-packaging/PLAN.md` (rev 3 + implementation progress)

## Verdict

**APPROVE WITH CHANGES.** The versioning + `_rs` normalization halves are **correct and complete** — clean, well-factored, fully validated (workspace builds, all 12 `--version` report `2.0.0-beta.1`, per-crate versions and the `bismark-io =1.0.0-beta.8` pin untouched, `publish = false` confirmed). The `release.yml` dry-run gating is **solid**. The risk concentrates in the **Docker image**, which was never actually built — it has a likely-fatal micromamba-USER bug and no image-level smoke test, so a broken "batteries-included" image could be published undetected.

- **Critical:** 1
- **Important:** 3
- **Optional:** 3

**Single most important fix:** the `Dockerfile` runs `micromamba install` and `COPY ... /usr/local/bin/` as the base image's **non-root `mambauser`**, which cannot write to root-owned `/usr/local/bin` — almost certainly a build failure. Add `USER root` before those layers (and run a real `docker build` before any non-dry-run dispatch).

---

## Critical

### C1. Dockerfile: `mambaorg/micromamba` runs as non-root `mambauser` → `COPY .../usr/local/bin` will fail
**File:** `Dockerfile:28–61`

`FROM mambaorg/micromamba:1.5.8-bookworm-slim` ends (in the base image's own Dockerfile) with `USER $MAMBA_USER` (UID 1000). Every subsequent `RUN`/`COPY` therefore executes as that **non-root** user. The Dockerfile then:
- `RUN micromamba install -y -n base ...` — may work (mambauser owns `/opt/conda`), but the `-n base` env-modify under a non-activated shell is exactly the case the micromamba docs say needs `ARG MAMBA_DOCKERFILE_ACTIVATE=1` for some operations.
- `COPY --from=builder ... /usr/local/bin/` and `COPY license.txt /usr/local/share/bismark/...` — these write to **root-owned** `/usr/local/bin` and `/usr/local/share`. As mambauser this fails with a permission error.

This was never caught because the plan explicitly notes the Docker build was *not* run locally ("needs a Docker build / dry-run"). The `release.yml` `docker-build` job (line 162+) builds the image but **never runs a container**, so a build failure surfaces only at dispatch — and a *successful-but-broken* image (e.g. install silently into the wrong prefix) would be pushed.

**Fix:** insert `USER root` after the `FROM` (and before the `RUN micromamba install` + the `COPY`s). The official pattern is:
```dockerfile
FROM mambaorg/micromamba:1.5.8-bookworm-slim
USER root
RUN micromamba install -y -n base -c bioconda -c conda-forge ... && micromamba clean --all --yes
...COPYs...
```
Strongly recommend an actual `docker build .` (amd64) before any real release dispatch — this whole stage is unexercised.

---

## Important

### I1. No Docker image smoke test — plan deliverable #4 not met; broken image can publish undetected
**File:** `.github/workflows/release.yml:162–203` (docker-build), Dockerfile (no `CMD`/`ENTRYPOINT`/healthcheck)

Plan §4a deliverable #4 explicitly states: *"Smoke test must verify the bundled externals resolve (not just `bismark_rs --help`)."* The implemented `docker-build` job only `build-push-action`s; there is **no `docker run`** verifying `bismark_rs --version`, that the bundled `bowtie2`/`samtools`/etc. are on PATH, or that `/opt/conda/bin` resolves. Only the **tarball** binaries get a smoke test (`smoke-test-binaries`, lines 138–160). Combined with C1, the image is entirely unvalidated end-to-end. Trim Galore's template smoke-tested its image.

**Fix:** add a step in `docker-build` (after build, on the just-built local image — buildx `load: true` for the smoke arch, or a follow-on job that pulls the digest in non-dry-run) that runs e.g. `docker run --rm <img> bash -c 'bismark_rs --version && bowtie2 --version && samtools --version && hisat2 --version && minimap2 --version'`.

### I2. Half-published release risk: `create-release` does not depend on `docker-merge`
**File:** `.github/workflows/release.yml:205–249`

Dependency graph: `docker-merge` (manifest tagging → user-facing `:beta`/`:version`/`:latest`) and `create-release` (tag push + GH release) both `needs: docker-build` and run **in parallel**. `create-release` does NOT need `docker-merge`. If `docker-merge` fails (e.g. `imagetools create` error) while `create-release` succeeds, you get: a pushed `bismark-rust-v<ver>` tag + a published GH release + uploaded tarballs, but **no tagged Docker manifest** (only orphan by-digest blobs). Because the workflow owns the tag and `check-release` aborts if the tag exists, re-running requires **manual tag + release deletion**. The "workflow-owns-the-tag" invariant is intact, but the recovery story for a partial failure is poor.

**Fix:** make `create-release` (or at least the tag push) `needs: [..., docker-merge]`, so the public tag/release is only created once both the tarballs *and* the tagged image manifest are in place. Alternatively gate the GH release on docker-merge success.

### I3. Dockerfile `COPY . .` with no `.dockerignore` → giant context incl. `rust/target/` and `.git`
**File:** `Dockerfile:23`; no `.dockerignore` at repo root (confirmed absent)

`COPY . .` from the repo-root context pulls in the entire working tree, including `rust/target/` (a multi-GB build-artifact tree already present in this worktree) and `.git`. This massively inflates the build context upload and image-layer cache churn, and risks leaking stale artifacts. The `--locked` build is unaffected functionally, but it's a real performance/hygiene problem for a CI image build. (TG's single-crate context was tiny; the suite's is not.)

**Fix:** add a `.dockerignore` excluding at minimum `rust/target/`, `**/target/`, `.git`, `plans/`, and other non-build inputs. Keep `rust/VERSION` + `.git` only if `build.rs` needs the hash — but since `BISMARK_SUITE_VERSION` is passed as a build-arg and the git-hash is currently unwired into `--version` (see O1), `.git` can be excluded entirely, simplifying the context.

---

## Optional

### O1. Plan deliverable #3 partially unmet: git-hash/provenance built but never shown in `--version`
**Files:** `rust/bismark-meta/src/lib.rs:11–22`, all 12 `version_string()`

`bismark-meta` exposes `GIT_SHORT_HASH`, `BUILD_TIMESTAMP`, `VERSION_BODY`, and a `version_line(tool)` helper — but **none of the 12 binaries use them**. Every `version_string()` interpolates the bare `bismark_meta::SUITE_VERSION` only (verified: no `version_line`/`VERSION_BODY`/`GIT_SHORT_HASH` reference outside the meta crate). Plan §4a #3 wanted "suite-version **+ git-hash**". So `build.rs`'s git-hash capture + the `rerun-if-changed` directives are effectively dead from the shipped output's perspective (still exercised by the meta-crate unit tests). This is **not a bug** (version isn't byte-gated; suite version alone is the user-facing contract), but it's a deviation from the stated intent and leaves provenance machinery unwired. Either wire `version_line()` into the bare-format binaries, or trim the unused consts/build.rs git logic to match what's actually emitted. Document the decision either way.

### O2. Branch guard allows prereleases from `master` (more permissive than the plan)
**File:** `.github/workflows/release.yml:66–73`

The guard reads: `master` → any version allowed (GA *or* prerelease); `rust/iron-chancellor` → prerelease only. The plan says "prereleases only from `rust/iron-chancellor`; GA from `master`". Allowing a prerelease from `master` is harmless today (master has no Rust suite) and arguably convenient, but it diverges from the documented intent. If strictness is desired, require `master` ⇒ non-prerelease. Low priority.

### O3. `bismark-io` not packaged into the suite tarball / not listed as a "binary"
**File:** `release.yml` packaging (lines 118–126), `THIRD-PARTY-NOTICES.md`

The header comment in `release.yml` says "12 binaries + the bismark-io lib", but `bismark-io` is a library and is (correctly) not in the tarball — it ships only inside the binaries. No action needed; just noting the comment could read as if the lib were a shipped artifact. The D2 decision (no crates.io during beta) is correctly reflected: there is no `cargo publish` job anywhere.

---

## What was verified correct (no action)

- **`_rs` rename complete in tracked source.** `command grep -rn "bismark-methylation-extractor-rs"` over tracked `*.rs`/`*.toml`/`*.md` (excluding `rust/target/`) returns **zero** hits. All stragglers are in gitignored `rust/target/`. The clap `name`, the `version_string()` banner, `[[bin]]` name, and ~18 test/src references all use `bismark_methylation_extractor_rs`. `cli.rs` test harness updated.
- **Bin-name enumerations agree.** The 12 `[[bin]]` names declared in the crates exactly match the lists in `release.yml` (build + smoke), `Dockerfile`, and `justfile` (incl. case-sensitive `bismark2bedGraph_rs`, `NOMe_filtering_rs`).
- **All 12 `--version` report the suite version.** Each crate's `version_string()` now uses `bismark_meta::SUITE_VERSION`; each `main.rs` prints it via the manual `--version` path (clap auto-version disabled). Built + ran dedup/bam2nuc/genome-prep → all print `2.0.0-beta.1`.
- **`bismark-meta` is internal-only.** `publish = false` confirmed via `cargo metadata` (`publish: []`). `env!("BISMARK_SUITE_VERSION")` resolves at compile because `build.rs` always emits it (with `unknown` fallback). `suite_version()` reads `$BISMARK_SUITE_VERSION` else `rust/VERSION`. Worktree-safe git path resolution via `git rev-parse --git-path` verified (returns the main-repo gitdir). `cargo test -p bismark-meta` → 2/2 green.
- **Per-crate versions + io pin untouched.** dedup `1.2.1-beta.1`, aligner `1.0.0-alpha.1`, c2c/bedgraph `1.0.0-beta.2`, io `1.0.0-beta.8`, rest `1.0.0-beta.1`; all 7 dependents keep `bismark-io = { version = "=1.0.0-beta.8", ... }`. `Cargo.lock` correctly gained the `bismark-meta` package + the dep edge in all 12 crates → `--locked` builds will resolve.
- **`release.yml` dry-run path suppresses ALL destructive effects.** `docker-build` builds with `push=${{ ...is_dry_run != 'true' }}` (build-only on dry-run); `docker-merge`, `create-release`, `upload-binaries`, and the digest export/upload are all `if: is_dry_run != 'true'`. No tag push, no GHCR push, no GH release, no crates.io (no such job exists). Tag scheme `bismark-rust-v<ver>` avoids the Perl `v*` and `bismark-io-v*` namespaces; collision check (`git tag -l`) present with `fetch-tags: true`. Action pins reasonable (checkout@v4, artifacts@v4, docker actions @v3/@v6, rust-toolchain@stable). `permissions: contents/packages: write` is minimal-correct (no `id-token` needed since crates.io OIDC is deferred).
- **Licensing.** `license.txt` (GPL-3.0 v3, 35 KB) and `rust/README.md` exist at the paths the workflow/Dockerfile `cp`; `license.txt`→`LICENSE` rename handled (avoids TG's silent `cp LICENSE` no-op). THIRD-PARTY-NOTICES licenses are accurate (Bowtie2 GPL-3.0, HISAT2 GPL-3.0, minimap2 MIT, samtools/htslib MIT/Expat).
- **MSRV match.** Workspace `rust-version = "1.89"`, edition 2024; Dockerfile build stage `rust:1.89-bookworm` — exact, and ≥ the 1.85 edition-2024 floor. Build output paths consistent: workflow `--target <triple>` → `rust/target/<triple>/release` (packaging uses same); Dockerfile no `--target` → `rust/target/release` (COPY uses same).
- **bioconda pins NOT verifiable here** — `api.anaconda.org` is OpenDNS-blocked in this sandbox (HTTP 403). From knowledge the pins are plausible (`bowtie2=2.5.5`, `hisat2=2.2.2`, `samtools=1.23.1` are real; `minimap2=2.31` is the package version, `-r1302` is the runtime `--version` git-rev suffix — the notice's `2.31-r1302` is the right user-facing string). **Recommend the implementer confirm availability during the first `docker build`.**

## Files reviewed
- `rust/VERSION`, `rust/bismark-meta/{Cargo.toml,build.rs,src/lib.rs}`, `rust/Cargo.toml`, `rust/Cargo.lock`
- All 12 binary crates' `Cargo.toml` + `src/lib.rs` `version_string()` (+ `bismark-aligner/src/main.rs`, `bismark-genome-preparation/src/main.rs`)
- `rust/bismark-extractor/{Cargo.toml,src/lib.rs,src/main.rs,src/cli.rs}` (rename)
- `.github/workflows/release.yml`, `Dockerfile`, `rust/justfile`, `rust/THIRD-PARTY-NOTICES.md`
- `plans/06062026_rust-suite-packaging/PLAN.md`
