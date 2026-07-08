# Plan — W3: bioconda `bismark` → Rust suite (`mamba install bismark`)

> **Epic:** `07062026_install-story/EPIC.md`, Workstream W3 (post-GA fast-follow)

**Status:** ✅ Rev 2 — recipe **finalized + submission-ready**. Fully validated against the shipped GA source tarball (not just Trim Galore inference). Submission is prepped in [`SUBMISSION.md`](SUBMISSION.md) + [`PR_BODY.md`](PR_BODY.md) — Felix owns the fork/PR (external, flips the default package). bioconda CI is the authoritative build gate; no local conda-build on Altos infra needed.

## Revision history
- **Rev 0 (2026-07-07)** — scaffold.
- **Rev 1 (2026-07-08)** — resolved Q1 (bioconda build **has network** during `cargo build` — proven by Trim Galore's `build.sh` fetching crates live with only `--locked`; so our crates.io deps + the `rammap-core` git dep resolve at build like the Dockerfile, no vendoring). Decided Q2 (`bismark-rust-v{{version}}` source tag), Q3 (**drop** coreutils — the Rust suite is self-contained for I/O; only the aligners are runtime deps), Q4 (features `rammap-inprocess,binseq-input`, matching the shipped binary), and the binary layout (**1 binary + 11 symlinks** like the Docker/tarball, NOT 12 fat `cargo install` binaries). Wrote the concrete `recipe-draft/` (sha256 of the GA source tarball computed: `da04c2d…`).
- **Rev 2 (2026-07-08)** — **validated every recipe assumption against the shipped GA source tarball** (extracted tag `bismark-rust-v3.0.0`): sha256 re-verified unchanged; crate/features/manifest confirmed; build cmd == Docker/`release.yml` byte-for-byte; **no `host: zlib`** (pure-Rust `zlib-rs`/`deflate_rust`) but **`{{ compiler('c') }}` required** (mimalloc compiles C); argv[0] self-routing + all-12 `--version` wiring confirmed (alias test safe); version banner contains `3.0.0` w/o `.git`. **Corrected the draft:** added the **platform matrix** (`skip: True # [osx and x86_64]` + `additional-platforms: [linux-aarch64, osx-arm64]`) — the exact 3 platforms GA `release.yml` builds on **stable** Rust (supersedes the stale "rammap needs nightly on macOS" note); pinned `BISMARK_SUITE_VERSION=$PKG_VERSION` in build.sh; preserved the `usegalaxy-eu` identifier + added the paper DOI; added `set -x`. De-risked run deps: bowtie2 (bioconda current = 2.5.5) / hisat2 / minimap2 all build all 3 platforms; conda-forge rust = 1.96.1 ≥ required 1.89.

---

## ⚠️ Epic re-framing (read first)
The install-story epic was written at the **2.0.0** framing, before the single-binary consolidation. Since then:
- **W1 (meta-crate) is SUPERSEDED** — the consolidation (epic `07062026_single-binary-suite/`, GA **3.0.0** shipped 2026-07-07) already made `bismark` ONE crate that builds one multicall binary; `cargo install bismark` works natively. No 12-tool-crate meta-crate needed.
- **W2 (docs) is largely DONE** — the docs pivot + GA reframe landed (#1060) and the homepage hotfix (#1062).
- **W3 (bioconda) is the only remaining install-story piece** — and it must target the **3.0.0 single crate**, not "the 12 tool crates."

## 1. Goal
Flip the **default** bioconda `bismark` package from Perl v0.25.1 → the **Rust suite 3.0.0**, so `mamba install bismark` yields a fully-functional environment: the one `bismark` multicall binary + its 11 classic-name symlinks, plus the external aligner backends **Bowtie2 2.5.5 / HISAT2 / minimap2**, and **NO samtools** (pure-Rust `noodles` I/O). rammap is compiled in (opt-in `--rammap`/`--rammap_inprocess`), not an external dep. Byte-identical to Perl v0.25.1 on the faithful path. Keep **`bismark=0.25.1`** (Perl) installable for anyone pinning the old behavior.

## 2. Context

### Reference recipe (RESOLVES the epic's crates.io-vs-tarball question)
**Trim Galore v2.3.0** (the Rust bioconda reference, `bioconda-recipes/recipes/trim-galore`):
- `source.url: https://github.com/FelixKrueger/TrimGalore/archive/refs/tags/v{{version}}.tar.gz` — **GitHub release tarball**, NOT crates.io.
- `build.requirements`: `{{ compiler('rust') }}`, `{{ compiler('c') }}`, `{{ stdlib('c') }}`, `make`, `pkg-config`.
- `host`: `zlib`; run deps minimal ("single static binary; no Perl/Python/pigz").
→ **W3 mirrors this: GitHub tarball + `cargo build`.**

### Current recipe (what W3 replaces)
`bioconda-recipes/recipes/bismark` — **Perl v0.25.1**:
- `source.url: …/Bismark/archive/refs/tags/v{{version}}.tar.gz`
- `run`: `coreutils, perl, samtools, bowtie2, hisat2, minimap2`

### GA artifacts to build from (3.0.0, shipped 2026-07-07)
- GH release + **tag `bismark-rust-v3.0.0`** → source tarball `…/Bismark/archive/refs/tags/bismark-rust-v3.0.0.tar.gz` (whole repo; the crate lives under `rust/`).
- crates.io `bismark` 3.0.0 (alternative build source; the Dockerfile/release build from the repo, so the tarball path is the faithful mirror).
- The reference build (Dockerfile/`release.yml`): `cargo build --release --locked --manifest-path rust/Cargo.toml -p bismark --bin bismark --features bismark/rammap-inprocess,bismark/binseq-input` → one `bismark` binary; ship + 11 classic-name symlinks.

## 3. Behavior / Implementation outline
**Progress (Rev 1):** steps 1 (research) + 2 (recipe authoring) are ✅ **DONE** — the concrete recipe is drafted in [`recipe-draft/meta.yaml`](recipe-draft/meta.yaml) + [`recipe-draft/build.sh`](recipe-draft/build.sh). Steps 3–5 (local `conda build` test → PR → the Docker follow-up) remain and need a conda-build environment + Felix's go (they're external/outward — a bioconda-recipes fork + PR).

1. ✅ **Research (was the load-bearing step): read Trim Galore's `build.sh` + `meta.yaml` in full** and determine how it satisfies cargo dependencies inside the bioconda build sandbox (network-restricted). Bismark additionally has a **git dependency** (`rammap-core =1.1.1`, live on crates.io) + crates.io deps — the recipe must fetch/vendor these the way Trim Galore does (vendoring, or the bioconda "internet during build" allowance for compiled recipes). **Resolve this before writing build.sh** (see Q1).
2. **Fork `bioconda/bioconda-recipes`**; edit `recipes/bismark/`:
   - `meta.yaml`: `version: 3.0.0`; `source.url` → the Rust tag (see Q2 — `bismark-rust-v{{version}}` vs a new plain `v3.0.0` tag) + its sha256; `build.requirements`: add `{{ compiler('rust') }}`, `{{ compiler('c') }}`, `{{ stdlib('c') }}`, `make`, `pkg-config`; **`run`**: `bowtie2=2.5.5`, `hisat2`, `minimap2` (DROP `perl` + `samtools`; decide `coreutils` — Q3); bump `build.number: 0`.
   - `build.sh`: `cargo build --release --locked --manifest-path rust/Cargo.toml -p bismark --bin bismark --features bismark/rammap-inprocess,bismark/binseq-input`; `install -m755 rust/target/release/bismark $PREFIX/bin/`; symlink the 11 classic names → `bismark` (`deduplicate_bismark`, `bismark_methylation_extractor`, `bismark2bedGraph`, `coverage2cytosine`, `bismark_genome_preparation`, `bam2nuc`, `NOMe_filtering`, `filter_non_conversion`, `methylation_consistency`, `bismark2report`, `bismark2summary`).
   - `test.commands`: `bismark --version` (→ 3.0.0) + `--help` on all 12 names.
3. **Build + test locally** (`conda build` / `bioconda-utils build` / `rattler-build` per current bioconda tooling) on linux-64; confirm the package installs all 12 names, `bismark --version` = `3.0.0`, deps resolve **without samtools**, and a smoke alignment runs (needs bowtie2).
4. **Submit the bioconda-recipes PR** — external review. Coordinate: a clear PR description (default `bismark` flips Perl→Rust; `bismark=0.25.1` stays pinnable), the version bump, and respond to bioconda maintainers.
5. **Container follow-up (separate, from epic):** the GA Dockerfile still bundles samtools + `smoke-test-docker` asserts it; once bioconda is samtools-free, decide whether to drop samtools from the image too (the suite never shells out to it). Track as its own task; not part of the recipe PR.

## 4. Assumptions
**From epic:** `bowtie2=2.5.5` is the byte-identity-pinned aligner; NO samtools (pure-Rust noodles); rammap compiled in (no external dep); keep `bismark=0.25.1` (Perl) installable (bioconda serves the pre-built old version — unaffected by the recipe update).
**Plan-specific:** mirror Trim Galore's GitHub-tarball + cargo-build pattern; the `rammap-core` git dep (`=1.1.1`, on crates.io) resolves at build like the release/Docker build; the `rammap-inprocess,binseq-input` features are wanted for parity with the shipped binary.

## 5. Validation
| # | Check | Expected |
|---|---|---|
| V1 | Local `conda build recipes/bismark` on linux-64 | builds green (cargo deps + rammap-core git dep resolve in the sandbox) |
| V2 | Install the built package into a clean env | all 12 names present; **no samtools** pulled in; bowtie2=2.5.5/hisat2/minimap2 present |
| V3 | `bismark --version` | `Bismark (Bismark Rust suite) v3.0.0` (or the methylseq-parseable banner) → **3.0.0** |
| V4 | Smoke a tiny alignment (bowtie2) + extract | runs; produces expected outputs |
| V5 | `mamba install bismark=0.25.1` still resolves | the Perl package remains installable (older version) |

**Local validation (oxy linux-64, 2026-07-08) — ✅ V1–V3 PASSED:** `conda-build` + `conda-forge-pinning` built `bismark-3.0.0-hdab8a38_0.conda` (~3min); in-recipe test phase green; clean-env install pulled **bowtie2 2.5.5 / hisat2 2.2.2 / minimap2 2.31, NO samtools**; `bismark --version`→`v3.0.0`; all 12 names resolve. (Root cause of the prior "conda broken on oxy": a `~/.condarc` `mirrored_channels` block routing conda-forge past the Altos mirror to the MITM'd anaconda.org — now removed; see [[altos-egress-umbrella-mitm-conda-cargo-mirrors]].) **V4 (smoke alignment)** not run locally; **V5** deferred — both left to bioconda CI (the authoritative multi-platform gate).

## 6. Questions / open decisions
- **Q1 — RESOLVED: bioconda build has network; no vendoring.** Trim Galore's `build.sh` is `cargo install --locked --no-track --path . --root "${PREFIX}"` with **no** `cargo vendor`/offline config — it fetches crates live and builds green on bioconda CI. So the crates.io deps + the `rammap-core` git dep (`=1.1.1`, live on crates.io) resolve at build exactly as in our Dockerfile; `--locked` keeps it deterministic. No vendoring step needed.
- **Q2 — DECIDED: `source.url` uses `bismark-rust-v{{version}}`** (no new tag). Explicit; couples to the Rust tag scheme, which is correct.
- **Q3 — DECIDED: DROP `coreutils`** (and perl + samtools). The Rust suite does its own I/O (noodles) and gzips via `gzp` (Rust), and only shells out to the aligner/indexer — so the sole runtime deps are `bowtie2=2.5.5`, `hisat2`, `minimap2` (matches the epic's "just Rust" goal + Trim Galore's minimalism).
- **Q4 — DECIDED: features `bismark/rammap-inprocess,bismark/binseq-input`** (matches the shipped binary — `.vbq`/`.cbq` decode + the in-process rammap opt-in).
- **Binary layout — DECIDED: 1 binary + 11 symlinks** (like the Docker/tarball), via `cargo build --bin bismark` + symlinks — NOT `cargo install`'s 12 fat binaries (~12× disk). The multicall binary self-routes on argv[0].
- **Q5 — samtools in the GA Docker image** (§3.5) — separate follow-up; not this PR.
- **Open (execution): re-verify the source sha256 at submission.** Drafted as `da04c2dccd234877d61bc217afa39474eba4aca56f34fc3752ef3bc891c11636` (GitHub archive of `bismark-rust-v3.0.0`); GitHub archive tarballs can drift — recompute with `curl -sL <url> | shasum -a 256` when opening the PR.

## 7. Self-Review
- **Scope:** re-framed to the 3.0.0 single crate (W1 superseded / W2 done), so W3 targets one binary + symlinks, not a 12-crate meta-build. The recipe mirrors the proven Trim Galore Rust pattern (GitHub tarball + cargo). The one genuine unknown (Q1 — cargo deps in the sandbox) is called out as the first, load-bearing execution step, not buried.
- **Edge cases:** the `bismark-rust-v` tag scheme (Q2), keeping Perl 0.25.1 pinnable (V5), no-samtools dep resolution (V2), the git dep (Q1/V1).
- **Risk/coordination:** high-stakes external PR (flips the default package for all users) — the plan makes the changelog/coordination explicit and keeps 0.25.1 available. The container samtools-drop is deliberately deferred to its own task so it can't block the recipe PR.
- **Not a scaffold gap, but flagged:** the exact `meta.yaml`/`build.sh` YAML is written during execution (step 2), after Q1 is resolved — this plan is the structure + decisions, per "scaffold."
