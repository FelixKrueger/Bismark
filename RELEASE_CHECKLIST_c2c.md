# `bismark-coverage2cytosine` — real-data release checklist (Phase E + v1.x Phase 4)

The real-data byte-identity gate. A clean run of `scripts/c2c_byte_identity_matrix.sh`
(exit 0) on **oxy** gates the release tag:
- **v1.0** (Phase E): the 9 core-report cells → `bismark-coverage2cytosine-v1.0` (tagged `…v1.0.0-beta.1`).
- **v1.x** (Phase 4): + the 6 niche-mode cells (`gc`/`nome`/`drach`/`ffs`/`ffs_cx`/`ffs_nome`) →
  the v1.x tag (propose `bismark-coverage2cytosine-v1.0.0-beta.2`), then merge `rust/c2c-v1x → iron-chancellor`.

⚠️ **The gate cov MUST be `bismark2bedGraph --CX` (all-context)** — the `gc`-cell GpC
require-nonempty depends on covered GpC-context Cs (Phase-4 PLAN §8). The NOMe *GpC*
streams are existence-only (allowed empty). A CpG-only cov would (correctly, but
confusingly) fail the GpC require-nonempty.

Design: `plans/05292026_bismark-coverage2cytosine/phase-e-byte-identity-gate/PLAN.md` (rev 1)
+ `plans/05312026_bismark-c2c-niche-modes/phase4-byte-identity-gate/PLAN.md` (rev 1).
Machine: **oxy** (Felix directive 2026-05-30; see SPEC §12.3, `reference_oxy_benchmark_env` memory).

---

## 0. Pre-trust: prove the harness FAILS-CLOSED (mandatory — do this FIRST, on any bash ≥4 box)

A green matrix is only trustworthy if the harness reliably FAILS on a real diff. Run
these two self-tests against the tiny committed fixture **before** trusting any
full-genome PASS (the dual-driver fail-open lesson — `feedback_dual_driver_back_port`):

```sh
# Fixture: a Perl-bismark2bedGraph-shaped cov + the phase_b genome (has a short
# scaffold → exercises the empty-split-report path).
gzip -c rust/bismark-coverage2cytosine/tests/data/phase_b/in.cov > /tmp/in.bismark.cov.gz
G=rust/bismark-coverage2cytosine/tests/data/phase_b/genome

# V12 — CORE cells on the fixture must PASS (exit 0). ⚠️ Scope with --cells to the
# CORE (v1.0) cells: the phase_b cov is NOT --CX and covers no GpC/DRACH/ACG-TCG
# positions, so the niche cells' (gc/drach/nome) require-nonempty would correctly
# but confusingly FALSE-FAIL on it (the --CX-cov dependency, §"What this gate
# proves"). The niche cells need a --CX fixture (next block).
scripts/c2c_byte_identity_matrix.sh /tmp/in.bismark.cov.gz --genome "$G" \
  --cells "cx default zero gzip thr split merge merge_disc merge_gzip" \
  --out /tmp/c2c_self_ok --disk-floor-gb 1
echo "expect exit 0: $?"

# V12-niche — the niche cells (gc/nome/drach/ffs/ffs_cx/ffs_nome) need an
# all-context (--CX-shaped) cov covering CpGs (mixed ACG/TCG + other), a GpC
# dinucleotide, and a DRACH motif (e.g. GAACA). Build a tiny one (see the Phase-4
# self-test in plans/.../phase4-byte-identity-gate/PLAN.md "Implementation notes"),
# then run `--cells "default gc nome drach ffs ffs_cx ffs_nome"` → expect exit 0 +
# all 5 niche differentials PASS.

# V1 — inject a 1-byte diff into a Rust output → matrix MUST exit 1:
#   (point --rust-c2c at a wrapper that corrupts one byte, or hand-edit one
#    rust/ output between the perl+rust runs; confirm exit 1 + the cell named.)

# V11 — truncate one side's *.gz → matrix MUST exit 1 (gzip -t catches it),
#   NOT a false PASS. (Truncate a cell's rust .gz, re-run that cell.)
```
If V1 or V11 does **not** exit 1, STOP — the gate is fail-open; fix before proceeding.

> **Note (dev machines):** macOS ships bash 3.2; the harness needs bash ≥4. Use
> `brew install bash` → `/opt/homebrew/bin/bash scripts/c2c_byte_identity_matrix.sh …`,
> or run the self-tests on oxy (bash 5).

---

## 1. oxy setup (verify first — access was deprecated 2026-05-28; details may have drifted)

```sh
dcli ssh oxy                      # verify connection (Q3)
tmux new -s c2c_release           # multi-hour matrix; survive disconnects
micromamba activate bismark-test  # Perl Bismark v0.25.1 + bismark2bedGraph + samtools
coverage2cytosine --version       # MUST show "coverage2cytosine" + "Version: v0.25.1"

# Rust toolchain (install if absent — not pre-provisioned):
which cargo || curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env

cd ~/Github/Bismark && git fetch && git checkout rust/c2c-v1x && git pull --ff-only
cd rust && cargo build --release -p bismark-coverage2cytosine
# ⚠️ ALWAYS rebuild after a pull: the harness only auto-builds the --release binary
#    when it is ABSENT, so a STALE pre-existing release binary would be used as-is.

df -h ~                           # confirm headroom vs oxy's ~99 GB cap (Q1)
```

## 2. Inputs

- **Genome:** the full-hg38 FASTA dir (verify exact subpath, e.g. `~/bismark_benchmarks/genome/`).
- **Cov (`<COV_GZ>`):** a **Perl-`bismark2bedGraph`-generated** `*.bismark.cov.gz` from the 10M PE
  dataset's methylation-extractor output. Using a *Perl*-generated cov keeps the two c2c producers
  independent (SPEC §13). **Q2 — confirm whether one already exists on oxy; if not, generate once:**
  ```sh
  # from the 10M PE extractor output (CpG/CHG/CHH context files):
  bismark2bedGraph --CX -o sample.cov_input CpG_context_*.txt CHG_context_*.txt CHH_context_*.txt
  # → produces sample.cov_input.bismark.cov.gz   (the <COV_GZ> below)
  ```

## 3. Run the matrix

```sh
scripts/c2c_byte_identity_matrix.sh <COV_GZ> \
  --genome ~/bismark_benchmarks/genome/ \
  --out ~/c2c_byte_identity_$(date -u +%Y%m%dT%H%M%SZ)/   # distinct out-dir (Felix directive)
# Optional: --disk-floor-gb N (default 30) · --keep-all · --cells "cx default merge"
```
The `cx` cell runs first (heaviest; max free space). Outputs land in the out-dir:
`matrix_verdict.txt`, `byte_identity_summary.md`, `perf_table.md`, and per-cell `cell_*/`.

## 4. Pass criteria

- **Exit 0** = every cell byte-identical (gzip post-decompression) **and** every cross-cell
  differential satisfied. This is the gate.
- **Exit 1** = a byte-diff, a missing/empty-where-required output, a gzip-integrity failure, or a
  differential violation. Investigate the named cell (`cell_<name>/diff.txt` + retained outputs).
- **Exit 2** = pre-flight/usage (bad args, wrong Perl version, insufficient disk).

The 15 cells:
- **v1.0 core (9):** `cx` (`--CX --gzip`), `default`, `zero` (`--zero_based`), `gzip`, `thr`
  (`--coverage_threshold 5`), `split` (`--split_by_chromosome`), `merge` (`--merge_CpGs`),
  `merge_disc` (`+ --discordance_filter 10`), `merge_gzip`.
- **v1.x niche (6, Phase 4):** `gc` (`--gc`), `nome` (`--nome-seq`), `drach` (`--drach`),
  `ffs` (`--ffs`), `ffs_cx` (`--ffs --CX --gzip`), `ffs_nome` (`--ffs --nome-seq`).

Require-nonempty vs existence-only (Phase-4 PLAN §3.2): the core CpG report + summary, the
`gc` GpC report+cov, the `nome` NOMe core report+`.NOMe.CpG.cov`, and the `drach` report+cov are
**required-nonempty**; the **NOMe GpC streams are existence-only**; and `ffs_nome`'s
`.NOMe.CpG.cov` is the **suppressed 0-byte file** (validated present-and-empty, NOT required-nonempty).

5 new cross-cell differentials (Phase 4): `gc` core==default (regression); `nome` lines `!=`+`<` default
(ACG/TCG filter fired); `drach` standalone (no normal report); `ffs` 10-col-every-line + lines==default;
`ffs_nome` `.NOMe.CpG.cov` present-and-0-byte both sides (the `--ffs`-suppresses-CYTCOV Critical).

> **Disk note (v1.x cells):** `ffs_cx` (gzipped CX) is the largest consumer; the
> **`nome` cell** is the 2nd-largest — it writes four full-genome streams
> (`NOMe.CpG_report.txt`, `.NOMe.CpG.cov`, `NOMe.GpC_report.txt`, `.NOMe.GpC.cov`)
> ×2 binaries before purge. cx-first + purge-on-pass keeps the working set to one
> cell; raise `--disk-floor-gb` if headroom is tight.

## 5. Disk fallback (Q1)

If the `cx` cell fails the pre-flight/per-cell disk gate on oxy (~99 GB cap, full-hg38 CX is tens
of GB even gzipped + streamed), re-run `cx` against a **chromosome-subset genome** (e.g. a dir with
only chr20-22) — `--cells cx --genome <subset_dir>` — and run the rest against the full genome.
Document the subset in the run notes. (The CX *content* path is also covered by the §12.2 fixtures.)

## 6. Tag (only on a clean full-genome exit 0)

**v1.0 (Phase E) — already done** (tagged `bismark-coverage2cytosine-v1.0.0-beta.1`, PR #892 merged).

**v1.x (Phase 4)** — on a clean exit 0 of the 15-cell matrix on oxy:
```sh
git tag -a bismark-coverage2cytosine-v1.0.0-beta.2 -m "coverage2cytosine Rust port v1.x — \
--gc/--nome-seq/--drach/--ffs byte-identical to Perl v0.25.1 across the 15-cell matrix on oxy (<dataset>, <date>)."
git push origin bismark-coverage2cytosine-v1.0.0-beta.2
```
Then: tick epic #891-v1.x Phase 4, note the gate result + perf table, and **merge `rust/c2c-v1x → rust/iron-chancellor`** (the epic close).

## 7. What this gate proves

Rust `coverage2cytosine_rs` reproduces Perl `coverage2cytosine` v0.25.1 byte-for-byte on real
full-genome data, across CpG/CX/zero/threshold/gzip/split/merge/discordance **and the v1.x niche
modes** (`--gc` GpC report, `--nome-seq` NOMe filtering, `--drach`/`--m6A` DRACH report, `--ffs`
context columns — incl. the `--ffs × --nome-seq` cov-suppression). Because the cov input
is Perl-`bismark2bedGraph`-generated (not Rust), this is a genuine **two-producer** comparison —
the independent Rust producer the extractor's Phase H **sub-gate 2** needs (SPEC §13). The extractor's
inline switch to call this crate is a separate, downstream task (parallel session).
