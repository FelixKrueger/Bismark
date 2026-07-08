# bioconda W3 — submission handoff (Felix owns the PR)

**Goal:** flip the default bioconda `bismark` package Perl v0.25.1 → the Rust suite **3.0.0**.
This is an **external, outward** PR (changes the default package for every bioconda user), so
**you submit it.** Everything below is prepped + verified; the recipe is in
`recipe-draft/{meta.yaml,build.sh}`.

> **Authoritative build gate = bioconda's own CI** (runs in bioconda infra, all 3 platforms).
> The recipe was validated by inspection against the shipped GA source tarball; there is no
> need to conda-build locally on Altos infra before submitting (per the plan's option (b)).

---

## 0. What was verified this session (2026-07-08)

All against the **shipped GA source tarball** (tag `bismark-rust-v3.0.0`, the exact bytes
bioconda compiles):

- **sha256 re-verified, unchanged:** `da04c2dccd234877d61bc217afa39474eba4aca56f34fc3752ef3bc891c11636`
- Build command is **byte-for-byte the Docker/`release.yml` reference build**:
  `cargo build --release --locked --manifest-path rust/Cargo.toml -p bismark --bin bismark --features bismark/rammap-inprocess,bismark/binseq-input`
- **No `host: zlib`** — the suite uses pure-Rust `flate2 = zlib-rs` + `gzp = deflate_rust` (unlike Trim Galore, which links system zlib).
- **`{{ compiler('c') }}` IS required** — the `mimalloc` allocator compiles C at build.
- **1 binary + 11 symlinks is provably valid** — `bismark::cli::dispatch()` routes on `file_name(argv[0])`, so a symlink named `deduplicate_bismark` runs the dedup path.
- **All 12 tools wire `--version`/`-V`** (each `Cli` has a `version: bool` field) → the alias test is safe.
- **`bismark --version` contains `3.0.0`** even without `.git` (VERSION files ship in the tarball; build.sh also pins `BISMARK_SUITE_VERSION=$PKG_VERSION`).
- **Platform matrix** = the 3 the GA `release.yml` builds on **stable** Rust: linux-64, linux-aarch64, osx-arm64 (skip Intel Mac). Same matrix as Trim Galore.
  - _(This supersedes the stale "rammap needs nightly on macOS" note — the GA shipped a stable `aarch64-apple-darwin` build; rammap-core is pinned `=1.1.1`.)_
- **All 3 run deps resolve on all 3 platforms:** bowtie2 (bioconda current = **2.5.5**, our pin), hisat2, minimap2 all declare `additional-platforms: [linux-aarch64, osx-arm64]`.
- **Toolchain OK:** crate needs rustc ≥ 1.89 (edition 2024); conda-forge ships **1.96.1**.
- **Local conda-build PASSED on oxy (linux-64, 2026-07-08):** `conda-build` + `conda-forge-pinning` (via `-m`) built `bismark-3.0.0-hdab8a38_0.conda` in ~3min; the in-recipe **test phase passed** (`bismark --version`→3.0.0, `--help`, all 11 aliases' `--version`); a clean-env `install bismark=3.0.0` pulled **bowtie2 2.5.5 / hisat2 2.2.2 / minimap2 2.31, NO samtools**, all 12 names resolve. Build hash inputs confirm `c_compiler: gcc`, `c_stdlib: sysroot`, `rust_compiler: rust`. (Recipe-render-equivalent to bioconda CI, not the full `bioconda-utils` harness — bioconda CI stays the authoritative multi-platform gate.)

---

## 1. Re-verify the sha256 (do this at submission time — archives can drift)

```bash
curl -sL https://github.com/FelixKrueger/Bismark/archive/refs/tags/bismark-rust-v3.0.0.tar.gz | shasum -a 256
# expect: da04c2dccd234877d61bc217afa39474eba4aca56f34fc3752ef3bc891c11636
```
If it differs, update `source.sha256` in `recipe-draft/meta.yaml` before committing.

---

## 2. Fork, apply the recipe, push (run these; `gh` needs a re-auth first)

`gh auth status` currently reports the keyring token is invalid — re-auth once:

```bash
gh auth login -h github.com          # interactive; pick GitHub.com + your account

# Fork WITHOUT the (huge) full clone, then blobless-clone the fork (fast):
gh repo fork bioconda/bioconda-recipes --clone=false
git clone --filter=blob:none https://github.com/FelixKrueger/bioconda-recipes.git ~/Github/bioconda-recipes
cd ~/Github/bioconda-recipes
git remote add upstream https://github.com/bioconda/bioconda-recipes.git   # optional, for future rebases

# Branch + apply the finalized recipe (replaces the Perl 0.25.1 meta.yaml + build.sh):
git checkout -b bismark-rust-3.0.0
DRAFT=~/Github/Bismark/plans/07062026_install-story/W3-bioconda/recipe-draft
cp "$DRAFT/meta.yaml" recipes/bismark/meta.yaml
cp "$DRAFT/build.sh"  recipes/bismark/build.sh

# Sanity: recipes/bismark/ should now contain ONLY these two files:
ls -la recipes/bismark/
git --no-pager diff --stat

git add recipes/bismark/meta.yaml recipes/bismark/build.sh
git commit -m "Update bismark to 3.0.0 (Rust rewrite)"
git push -u origin bismark-rust-3.0.0
```

> Note: `recipes/bismark/` currently holds exactly `meta.yaml` + `build.sh`, so the two
> `cp`s fully replace the recipe (no stale files to remove). The `ls` above confirms it.

---

## 3. Open the PR (base = `bioconda/bioconda-recipes:master`)

```bash
gh pr create \
  --repo bioconda/bioconda-recipes \
  --base master \
  --head FelixKrueger:bismark-rust-3.0.0 \
  --title "Update bismark to 3.0.0 (Rust rewrite)" \
  --body-file ~/Github/Bismark/plans/07062026_install-story/W3-bioconda/PR_BODY.md
```

(`PR_BODY.md` is written alongside this file — review/edit it before submitting.)

After opening: reply to the bioconda bot with `@BiocondaBot please add label` if a
maintainer label is needed, and watch the CI (lint + `linux-64`/`linux-aarch64`/`osx-arm64`
builds). CI is the authoritative gate.

---

## 4. If bioconda CI fails — most-likely causes & fixes (ranked)

1. **rammap-core git fetch blocked** — the recipe's single non-crates.io dependency is a
   pinned git dep (`https://github.com/jwanglab/rammap` @ `5ea62cd`). bioconda's
   compiled-recipe builders have network, so this should resolve, but if a builder is
   network-restricted the fix is to vendor (`cargo vendor` + a `.cargo/config.toml`) — this
   is the one thing we could not exercise without their CI.
2. **`bowtie2 =2.5.5` unavailable on a platform** — currently fine (it's bioconda's latest +
   builds all 3 platforms). If a future rebuild can't satisfy it, relax to `>=2.5.5` or drop
   the platform via a `skip`.
3. **rustc too old / edition 2024** — not expected (conda-forge = 1.96); if a pinned older
   rust is used, no recipe fix — it's a conda-forge pin issue.
4. **A platform's Rust build fails** (e.g. osx-arm64) — narrow the matrix: remove that entry
   from `extra.additional-platforms` (or add a `skip`). linux-64 alone still ships a usable
   package.

---

## 5. Known considerations (not blockers)

- **nf-core/methylseq version scraping:** the bioconda `bismark` is the *real* binary, so
  `bismark --version` emits the suite banner `bismark (Bismark Rust suite) v3.0.0 (...)`.
  The Docker image's methylseq-parseable version-probe **wrapper is image-only** (a
  `docker/` accommodation) and is intentionally NOT replicated in the conda package. Covered
  by the methylseq maintainer heads-up you're posting (version re-baseline v0.25.1→3.0.0).
- **`bismark=0.25.1` (Perl) stays installable** — bioconda serves the pre-built old version;
  this recipe update only changes what `bismark` (unpinned) resolves to.
- **`recipe-maintainers: [FelixKrueger, ewels]`** — added (matches Trim Galore). Confirm
  Phil is happy to be listed before submitting, or drop him.
- **Docker samtools-drop** is a separate follow-up (the image still bundles samtools for
  methylseq co-residency); not part of this recipe.
