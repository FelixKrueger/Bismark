# CODE REVIEW B — Bismark Rust suite packaging / release infrastructure

**Reviewer:** Code Reviewer B (independent, fresh context, adversarial)
**Date:** 2026-06-06
**Worktree:** `/Users/fkrueger/Github/Bismark-packaging` (branch `rust/suite-packaging` off `iron-chancellor 8daa55d`)
**Target:** uncommitted packaging implementation (`git diff rust/iron-chancellor` + untracked infra files)
**Plan:** `plans/06062026_rust-suite-packaging/PLAN.md` (rev 3 + impl notes)

## Verdict

**APPROVE WITH CHANGES.** The release automation is fundamentally sound: dry_run suppresses *every* destructive side effect, the versioning trap is correctly handled (build.rs always emits, never panics, `env!` compiles), the tag scheme is collision-free, all 12 bin names are consistent across Dockerfile/release.yml/justfile, the upstream tool pins are all real, and the full workspace builds `--locked`. No **Critical** show-stoppers. The defects are real but bounded: a **half-publish window** (release created before assets upload), a **missing Docker smoke-test** (plan deliverable dropped), and a **version-coherence leak** in the extractor banner.

- **Critical:** 0
- **Important:** 4
- **Optional:** 4

**Highest-risk finding (I-1):** On a real run, `create-release` pushes the git tag and creates the GitHub Release *before* `upload-binaries` runs and does **not** depend on `docker-merge`. A failure in asset upload or the Docker manifest leaves a **published release + pushed tag with no binaries** (and/or orphan untagged GHCR digests) — the classic half-publish, requiring manual tag/release cleanup before retry (the `check-release` collision guard then blocks the retry).

---

## Claims re-derived (what I actually verified, not took on faith)

- **dry_run leak walk** — read every `if:` + the docker `push=` expression. `docker-build` push expr `push=${{ ... is_dry_run != 'true' }}` → `push=false` on dry run; `docker-merge`/`create-release`/`upload-binaries`/digest-export all gated `if: is_dry_run != 'true'`. **No destructive op leaks on dry_run.** (verified by reading the full workflow)
- **Versioning trap** — `build.rs` `git_short_hash()`/`suite_version()`/`git_path()` all use `.ok()`+`unwrap_or_else(...)`; `build_epoch()` only panics on a *malformed* `SOURCE_DATE_EPOCH` (unset in Docker → `SystemTime::now()`). build.rs always emits all four `cargo:rustc-env` keys, so `env!("BISMARK_SUITE_VERSION")` in lib.rs always compiles. **No silent-wrong-value, no compile failure.**
- **Docker version flow** — `ARG BISMARK_SUITE_VERSION=unknown` → `ENV BISMARK_SUITE_VERSION=...` set in builder stage *before* `RUN cargo build`; build.rs reads it via `env::var` (precedence over `rust/VERSION`); `rerun-if-env-changed` declared. release.yml passes `build-args: BISMARK_SUITE_VERSION=<version>`. In Docker `.git` is absent → hash `unknown` (fallback, no failure). **Reaches build.rs correctly.**
- **CWD / VERSION resolution** — build.rs reads `CARGO_MANIFEST_DIR/../VERSION` = `<checkout>/rust/bismark-meta/../VERSION` = `rust/VERSION`. CI build uses `--manifest-path rust/Cargo.toml`; Dockerfile builds without `--target` (output `rust/target/release/`, COPY matches); CI builds with `--target` (output `rust/target/<triple>/release/`, BINDIR matches). **All paths consistent.**
- **`CARGO_PKG_VERSION` residue** — `command grep -rn CARGO_PKG_VERSION rust/*/src`: only one live hit → `rust/bismark-extractor/src/logging.rs:78` (banner). See I-3.
- **Rename ripple** — `command grep -rn "bismark-methylation-extractor" rust/`: all remaining hits are under `rust/target/` (build artifacts), none in source. `--help` prints `bismark_methylation_extractor_rs`; no name leak in `header.rs`/`output.rs` (so the byte-identity gate is untouched by the rename). **Clean.**
- **Upstream pins are real** — verified via GitHub API/releases: bowtie2 `v2.5.5` ✓, samtools `1.23.1` ✓, minimap2 `2.31-r1302` (released 19 May 2026) ✓. micromamba `1.5.8-bookworm-slim` is a real Docker Hub tag ✓. micromamba `MAMBA_ROOT_PREFIX="/opt/conda"` (from `mamba-org/micromamba-docker` `debian.Dockerfile`) → `ENV PATH=/opt/conda/bin:$PATH` is correct ✓.
- **Tag collision** — `git tag -l "bismark-rust-v*"` = empty; distinct from 77 existing `v*` (Perl) and `bismark-io-v*` tags. **Collision-free.**
- **Builds** — `cargo build --workspace --locked --offline` clean; `cargo test -p bismark-meta` 2/2 pass; `Cargo.lock` in sync (has `bismark-meta 0.1.0`).
- **Empirical version check** — `deduplicate_bismark_rs/bismark_methylation_extractor_rs/bismark_rs/bismark_genome_preparation_rs --version` all print `2.0.0-beta.1`; extractor **banner** prints `version 1.0.0-beta.1` (the leak, I-3).

### `curl`/`grep`/tooling run
- `command grep -rn CARGO_PKG_VERSION rust/*/src`; `command grep -rn "bismark-methylation-extractor" rust/`
- GitHub API/WebFetch: minimap2 releases+tags, samtools tags, bowtie2 tags, micromamba Docker Hub 1.5.8 tags, `mamba-org/micromamba-docker` `debian.Dockerfile` + `_dockerfile_setup_root_prefix.sh`
- `cargo build --workspace --locked --offline`; `cargo test -p bismark-meta`; ran each binary `--version`/`--help`; ran the extractor on a tiny SAM to trigger the banner
- **Could NOT verify:** bioconda *availability* of the exact pins (`api.anaconda.org` is DNS-blocked in this sandbox → HTTP 403 OpenDNS block). See I-2.

---

## Important

### I-1 — Half-publish window: release created before assets; `create-release` doesn't gate on `docker-merge`
`release.yml`, jobs `create-release` (L232-249) and `upload-binaries` (L251-261).

`create-release` (`needs: [check-release, build-binaries, smoke-test-binaries, docker-build]`) does `git push origin <tag>` + `gh release create` **first**. `upload-binaries` (`needs: [create-release, build-binaries]`) uploads the tarballs **after**. If `upload-binaries` fails (artifact download hiccup, `gh release upload` error, a transient), you are left with: **a pushed `bismark-rust-v<ver>` tag + a published GitHub Release with zero assets.** Re-dispatch then dies at `check-release` (tag collision guard, L76) until someone manually deletes the tag *and* the release.

Compounding: `create-release` does **not** `need` `docker-merge`, and `docker-merge`/`docker-build` push images by-digest/tag independently. So you can also get: release+tag created but the Docker `:beta`/`:version` manifest never tagged (orphan untagged digests in GHCR), or vice-versa. The publish is not atomic across the binary + Docker + release surfaces.

**Recommendation:** make the Release the *last* irreversible act. Either (a) merge `create-release` + `upload-binaries` into one job that creates the release and uploads in the same step (so a failed upload fails before/with the release), or (b) create the release as a **draft** first, upload assets, then flip to published as the final step; and add `docker-merge` to the gating so the image manifest is confirmed before the release goes public. At minimum, document the manual-cleanup runbook (delete tag + release) since the collision guard will otherwise wedge retries.

### I-2 — Docker image is never smoke-tested (plan deliverable dropped); bioconda pin availability unverifiable
`release.yml` has 7 jobs; there is **no docker smoke-test job**. The image's bundled externals (bowtie2/hisat2/minimap2/samtools) and the 12 binaries inside it are tagged + pushed (`docker-merge`) with **zero verification**. This directly contradicts PLAN §4a deliverable 4 ("Smoke test must verify the bundled externals resolve (not just `bismark_rs --help`)") and §5 item 5. The *tarball* binaries are smoke-tested (`smoke-test-binaries`), but the container — the more failure-prone artifact (bioconda solve + non-root user + PATH) — is not.

Separately, the `RUN micromamba install ... bowtie2=2.5.5 hisat2=2.2.2 minimap2=2.31 samtools=1.23.1` is a **single solve that must succeed for all four pins simultaneously**. The upstream *source* versions are all real (verified), but I could not confirm bioconda has each packaged at that exact version *and* that they co-solve (minimap2 2.31 was released only ~2.5 weeks before this plan — bioconda packaging lag is plausible). If any pin is absent or unsolvable, the **image build fails outright** — and with no docker smoke-test/build gate run yet (impl notes admit "needs a Docker build / dry-run"), this is unproven.

**Recommendation:** add a docker smoke-test job (run `docker run <image> <each bin> --version` + `bowtie2 --version`/`samtools --version` to prove the bundled externals resolve on `PATH`), and actually run a `docker build` (locally or a CI dry-run) before the first real release to confirm the bioconda solve. Consider splitting the four pins into separate `RUN` layers so a failing pin is isolated/diagnosable.

### I-3 — Version-coherence leak: extractor startup banner still prints the per-crate version
`rust/bismark-extractor/src/logging.rs:73-79` — `banner()` still uses `env!("CARGO_PKG_VERSION")`, called from `parallel.rs:258` on **every** extraction run:

```
*** Bismark methylation extractor (Rust port) version 1.0.0-beta.1 ***
```

while `bismark_methylation_extractor_rs --version` correctly prints `2.0.0-beta.1` (empirically confirmed — ran both). This is exactly the failure PLAN §4a item 3 set out to eliminate ("would print numbers matching no release tag" → "every bin reports the suite version"). `1.0.0-beta.1` matches no release tag. The extractor is one of only two crates with a *second* version surface (the banner) beyond `version_string()`, and the rollout missed it. (It does **not** affect the byte-identity gate — the banner is `--quiet`-gated stderr, not output data — which is why it's Important not Critical, but it's a user-visible incoherence and a dropped deliverable.)

**Fix (low-risk):** `logging.rs:78` → `bismark_meta::SUITE_VERSION` (the crate already depends on `bismark-meta`). Update the doc-comment at L73-74.

### I-4 — `bismark-meta` is `publish = false` with version-less path deps → blocks the deferred GA crates.io publish
All 12 binary crates declare `bismark-meta = { path = "../bismark-meta" }` (no version), and `bismark-meta/Cargo.toml` sets `publish = false`. `cargo publish` **refuses** a crate whose dependency has no version requirement, and refuses to publish a crate depending on an unpublished crate. PLAN D2/§6 explicitly plans to publish the 12 binary crates to crates.io at GA — that publish is now **structurally blocked** by this wiring. It is correctly deferred (not a beta blocker), but it's an **unstated assumption** that will surface at GA and isn't flagged in the plan's GA checklist.

**Recommendation:** note this in the plan's GA-deferred list. At GA, either give `bismark-meta` a real version + publish it (it's tiny), or inline the version-stamping into each binary's own build.rs / make the meta-dep a `dev`/build-only path so it isn't a publish dependency. Flag now so GA isn't surprised.

---

## Optional

### O-1 — Plan/impl deviation: `rust/VERSION` file vs the `[workspace.package] version` the plan specified
PLAN rev-3 D1 says the single source should be "a `[workspace.package] version` in `rust/Cargo.toml`"; the implementation instead uses a standalone `rust/VERSION` file (and `rust/Cargo.toml` `[workspace.package]` has no `version`). The impl note (PLAN L74) documents this, and it's arguably *cleaner* (avoids per-crate inheritance), but the plan body still references `[workspace.package] version` in two places (§4a item 5, D1) — stale/contradictory. Cosmetic; reconcile the plan text.

### O-2 — `dry_run` skips the tag-collision check → false-green dry run
`check-release` L76: `if [ "${{ inputs.dry_run }}" != "true" ] && git tag -l "$TAG" | grep -q .`. A dry run does **not** fail if the tag already exists, so a green dry run does not guarantee a subsequent real run will pass `check-release` (the real run *will* fail there, before any push — so it's safe, just mildly misleading). Consider running the collision check unconditionally (it's read-only) so the dry run is a faithful preflight.

### O-3 — No `.dockerignore` → build context includes `rust/target/` + the worktree `.git` pointer
No `.dockerignore` exists, so `COPY . .` ships the entire repo (incl. a multi-GB `rust/target/` if present, and the worktree `.git` *file* that points outside the context — harmless, just yields `unknown` hash). Bloats the build context / cache and slows CI. Add a `.dockerignore` excluding `target/`, `rust/target/`, `.git`. (Functionally fine — `cargo build --locked` rebuilds regardless — but wasteful.)

### O-4 — `strip = "debuginfo"` (not full `strip = true`) + unsigned macOS binaries
`[profile.release]` strips debuginfo but keeps the symbol table; TG-style full strip (`strip = true`) would yield smaller, more reproducible binaries. And the macOS-aarch64 tarball ships unsigned (Gatekeeper quarantine) — PLAN §5 flags this caveat but the release notes / README in the tarball don't document the `xattr -d com.apple.quarantine` workaround. Both are documented-caveat territory, not defects.

---

## What TG (1 binary) didn't need that a SUITE does — checked
- **Single build.rs / single git-hash source:** correctly solved — only `bismark-meta` has a build.rs; the other 12 link it, so the hash/version is captured once, not 12 inconsistent times. ✓
- **N-binary name consistency:** the bin list is identical across Dockerfile COPY, release.yml package+smoke loops, and the justfile `version`/`reproduce` loops — verified all 12 names match the `[[bin]]` definitions. ✓
- **Bundled externals (TG bundled FastQC *into* its binary; Bismark can't):** handled via the batteries-included Docker image + THIRD-PARTY-NOTICES — but the externals are **not smoke-tested** (I-2). ◑
- **Per-crate independent semver preserved:** binary crates keep their own `Cargo.toml` versions (aligner `1.0.0-alpha.1`, dedup `1.2.1-beta.1`, …); no blanket `version.workspace = true`. ✓ — except this surfaces the publish-block (I-4).

## Things the plan got right (adversarial probes that came up clean)
- dry_run is airtight across all destructive ops (no tag/GHCR/release/crates.io leak).
- No crates.io job at all (D2 deferral honored).
- build.rs never panics with git absent; `unknown` fallback throughout; `env!` always compiles.
- `license.txt` (lowercase) is correctly used (TG's `cp LICENSE` no-op trap avoided).
- Branch guard: GA-from-iron-chancellor is correctly rejected; prerelease-only from the integration branch.
- Native-arch matrix (no cross-compile) sidesteps the `cc`/mimalloc cross-build risk.
- The `_rs` rename touches no byte-identity-gated output — the gate is safe.
