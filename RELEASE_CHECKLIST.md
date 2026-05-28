# Bismark Rust rewrite — release checklist

The binding gate for tagging release versions of the Rust port. This file
captures the manual steps + the matrix-driver invocations that verify
byte-identity vs Perl Bismark v0.25.1 on real WGBS data.

Self-hosted runner + scheduled CI is **not** the gate (see Phase H plan
§10 rev 1 — Felix-approved manual-checklist approach). Continuous CI runs
the 303-test in-repo suite on every PR; that's the merge gate. This
checklist is the release gate.

## Roles

- **Release engineer:** Felix (single-person process for v1.0).
- **Sign-off recording:** comment on epic [#798](https://github.com/FelixKrueger/Bismark/issues/798) with the matrix output + a PASS or FAIL marker. Attach
  `<OUT>/speedup_table.md` + `<OUT>/matrix_verdict.txt` as a gist link
  or inline code block.

## Escalation paths

### Mid-checklist regression (exit 1)

If the matrix reports FAIL (exit code 1 from either SE or PE matrix):

1. **Save evidence**: archive the failing `<OUT>/` dir (matrix_verdict.txt,
   cross_n_summary.txt, speedup_table.md, per-cell `cell_*` subdirs). Don't
   re-run on the same `--out` dir — the driver rejects non-empty output dirs
   to preserve the evidence.
2. **File a `bug(extractor):` sub-issue** under epic #798 with:
   - The failing cell + diff excerpt from `diff_summary.txt`
   - `matrix_verdict.txt` content
   - Suspected Phase (consult rev 1 plan §3.4 contract per assertion)
3. **Pause v1.0 tag** work until the bug is resolved + the matrix re-run
   PASSes on a fresh `--out` dir.

### Performance-target miss (exit 3)

If exit code 3 (byte-identity PASS but Rust scaling < SPEC §9.7's 4× target):

1. File a `perf(extractor):` follow-up sub-issue under epic #798 with the
   speedup_table.md attached.
2. The v1.0 tag MAY proceed; perf miss is informational, not a byte-identity
   regression. SPEC §9.7's target may itself need revision based on the
   measurement.

### Pre-flight USAGE error (exit 2)

Typically: BAM path wrong, Perl version drift in `bioinf` env, `--out` dir
not empty, `--parallel N > nproc`. Driver emits an explicit error message
with the remediation hint. Fix and re-run; this is not a code regression.

## bismark-extractor v1.0 — Phase H byte-identity sub-gate 1

Prerequisites: Phase G merged (`ff961d3` or later) on `rust/iron-chancellor`.

### Pre-matrix setup (one-time per release-prep session)

**`tmux` / `screen` is non-optional** — the SE matrix takes 1-3 hours; the
PE matrix similar. SSH disconnect over a 2-hour run leaves orphaned
subprocesses and corrupted half-state. The matrix driver also warns if it
detects `$TMUX` / `$STY` are unset.

**`bash >= 4.0` required** — the matrix driver uses `declare -A` and the
empty-array-under-`set -u` idiom; both bash 4.0+. Colossal Linux ships
bash 5.x by default. The macOS default `/bin/bash` is 3.2 and is rejected
by the driver's pre-flight (run `brew install bash` and use
`/opt/homebrew/bin/bash` for local-Mac development).

**Perl version equivalence** — the matrix's pre-flight asserts that the
discovered `bismark_methylation_extractor` reports `Bismark Extractor
Version: v0.25.1`. By default the driver discovers via `$PERL_BIN` which
falls back to the repo's checked-in `./bismark_methylation_extractor`
script (which IS the v0.25.1 source). The `bioinf` env's PATH binary is
the bioconda packaging of the same v0.25.1 source, so both should agree.
Override `PERL_BIN` only if you intentionally want a different binary.

```bash
dcli ssh colossal
tmux new -s phase_h_release   # or screen -S phase_h_release
cd ~/Github/Bismark   # or wherever the working copy lives on colossal
git checkout rust/iron-chancellor
git pull --ff-only
git log --oneline -1   # confirm HEAD on rust/iron-chancellor

micromamba activate bioinf   # provides Perl bismark v0.25.1 + samtools + bowtie2
bismark_methylation_extractor --version | head -3
# Expect: "Bismark Extractor Version: v0.25.1" — pre-flight will assert this.

bash --version | head -1   # expect 4.0+; colossal Linux ships 5.x
nproc                      # confirm core count for --parallel-set sizing

# Budget ~5-15 min for cargo build on a cold cache (first checkout of the
# day) or a clean target/ dir. Subsequent rebuilds are ~30s. Matrix driver
# doesn't run cargo build itself — do this manually before invoking.
cargo build --release --manifest-path rust/Cargo.toml -p bismark-extractor
# This produces rust/target/release/bismark-methylation-extractor-rs which
# the matrix driver discovers automatically. NO manual RUST_BIN override needed.
```

### SE matrix (closes #871)

```bash
# Confirm the 10M SE BAM path on colossal first:
ls /weka/projects/bioinf/Data/Felix/bismark_benchmarks/10M_SE/
# Expected: directional_10M_R1_val_1_bismark_bt2.bam (mirroring oxy layout;
# verify on first colossal session and update this checklist + plan if path differs).

bash scripts/phase_h_se_matrix.sh \
  /weka/projects/bioinf/Data/Felix/bismark_benchmarks/10M_SE/directional_10M_R1_val_1_bismark_bt2.bam \
  --out ~/phase_h_se_release/   # use a fresh dir, NOT clobbering a prior run
```

Verify:

- [ ] Exit code 0 (PASS) or 3 (perf-miss-only). Exit 1 blocks v1.0; exit 2
      is pre-flight (fix env + re-run).
- [ ] `~/phase_h_se_release/matrix_verdict.txt` reports PASS aggregates.
- [ ] `~/phase_h_se_release/cell_p1_i0_i30/diff_summary.txt` confirms:
      - `*_splitting_report.txt` cmp PASS
      - `*.M-bias.txt` cmp PASS at byte size **5712** (the locked Phase C.1
        regression-guard baseline).
- [ ] `~/phase_h_se_release/cross_n_summary.txt` shows PASS for all 5
      ignore-pairs (Rust-N=1 ≡ Rust-N=4 raw-byte; SPEC §8.3 row 4).
- [ ] `~/phase_h_se_release/speedup_table.md` shows the **Rust/Perl** column
      (NOT Perl/Rust — column header semantics matter for release-engineer
      reading; rev 3 fixed an inversion bug). Rust-scaling-at-N=4 ≥ 4×.
- [ ] `~/phase_h_se_release/speedup_table.md` M-bias row-count differential
      section reports PASS — ignore-flag cells produce fewer rows than the
      (D, N=1) baseline (rev 3 absorption per Coverage §3.4 #4).

Recording:

- [ ] Comment on epic #798 with the speedup_table.md content + "SE matrix:
      PASS at <date>".

### PE matrix (closes #872)

```bash
# Confirm the 10M PE BAM path on colossal first:
ls /weka/projects/bioinf/Data/Felix/bismark_benchmarks/10M_PE/
# Expected: SRR24827378_10M_R1_val_1_bismark_bt2_pe.deduplicated.bam (mirroring
# oxy layout; verify on first colossal session and update this checklist +
# plan if path differs).

# Pre-flight: the driver computes the BAM MD5 (~5-10 s for 1.2 GB) +
# samtools view -c (~30 s) for the overlap-fraction gate. Total pre-flight
# ~1 min; matrix proper takes 1.5-4 h (rev 1 A-O2 honest range).
bash scripts/phase_h_pe_matrix.sh \
  /weka/projects/bioinf/Data/Felix/bismark_benchmarks/10M_PE/SRR24827378_10M_R1_val_1_bismark_bt2_pe.deduplicated.bam \
  --out ~/phase_h_pe_release/   # use a fresh dir, NOT clobbering a prior run
```

Verify:

- [ ] Exit code 0 (PASS) or 3 (perf-miss-only — v1.0 may ship at exit 3 per
      PHASE_H_PE_PLAN §1; Phase C.2 era measured 0.9× on default PE, so
      first colossal run may legitimately land on exit 3). Exit 1 blocks v1.0;
      exit 2 is pre-flight (fix env + re-run).
- [ ] `~/phase_h_pe_release/matrix_verdict.txt` reports PASS aggregates.
- [ ] `~/phase_h_pe_release/cell_p1_D/diff_summary.txt` confirms:
      - `*_splitting_report.txt` cmp PASS per-cell (rev 1 A-I5 dropped the
        absolute 875 B size lock; per-cell Perl-vs-Rust byte-cmp is the
        Phase C.2 format regression guard).
      - `*.M-bias.txt` cmp PASS at byte size **11,443** (the locked Phase C.1
        polarity regression-guard baseline).
- [ ] `~/phase_h_pe_release/cross_n_summary.txt` shows PASS for all 5
      CELL_IDs (Rust-N=1 ≡ Rust-N=4 raw-byte; SPEC §8.3 row 4). Cross-N
      runs unconditionally per cell even if byte-identity FAILed (rev 1
      B-Imp-4 — preserves diagnostic signal).
- [ ] `~/phase_h_pe_release/speedup_table.md` shows the **Rust/Perl** column
      (NOT Perl/Rust — column header semantics matter for release-engineer
      reading; mirrors SE rev 3 B-L1 fix). Rust-scaling-at-N=4 ≥ 4×
      preferred but exit 3 acceptable for v1.0.
- [ ] `~/phase_h_pe_release/speedup_table.md` "Mixed-metric differential"
      section reports PASS:
      - `r1_5p` / `r2_5p` / `r1r2_3p`: M-bias data row count strictly < D
        (`--ignore N` removes positions monotonically).
      - `overlap`: M-bias count-sum (methylated + unmethylated) strictly > D
        by ≥5% (rev 1 A-O3 — `--include_overlap` accumulates counts at
        existing positions; row count is unchanged because M-bias positions
        are read-relative; count-sum strictly increases).
- [ ] `~/phase_h_pe_release/speedup_table.md` "Properly-paired fraction"
      header ≥ 80% (rev 1 A-I1 pre-flight gate already enforced this; this
      verify confirms the recorded value).
- [ ] `~/phase_h_pe_release/speedup_table.md` "Input BAM MD5" matches the
      planner's reference (rev 1 B-Opt-4 fixture-drift detector). On first
      colossal session, record the MD5; mismatch on later runs signals
      silent BAM swap-in.

Recording:

- [ ] Comment on epic #798 with the speedup_table.md content + "PE matrix:
      PASS at <date>" (or "PE matrix: PASS + perf-miss at <date>" with the
      perf sub-issue link if exit 3).

### Escalation: colossal-vs-planner baseline drift (rev 1 A-I4)

If the PE matrix exits 1 with `M-bias baseline 11,443 B drift (or missing
file) at (D, N=1) cell` BUT `cell_p1_D/diff_summary.txt` shows per-cell
Perl-vs-Rust byte-cmp PASS:

1. This is BAM/env mismatch, NOT a Rust regression. Confirm via per-cell
   byte-cmp PASS.
2. Verify the BAM MD5 in `speedup_table.md` differs from the planner's
   reference (or document a new locked BAM if intentional).
3. Verify the prior 11,443 B baseline was a transcription error: search
   git log + memory for the original oxy measurement.
4. If verification confirms the colossal value is correct: file a rev-2
   baseline-update PR replacing 11,443 B with the new value in BOTH
   `scripts/phase_h_pe_matrix.sh` AND SPEC §8.3.
5. Re-run matrix on a fresh `--out` dir. Do NOT bypass the gate without
   the baseline-update PR.

### v1.0 tag steps

- [ ] Both SE matrix (this section, #871) and PE matrix (#872) recorded
      PASS on epic #798.
- [ ] `cargo test -p bismark-extractor` clean on `rust/iron-chancellor` HEAD
      (no regressions since Phase G's 303-test baseline; current count may
      be higher if intermediate tests landed).
- [ ] Crate version bump in `rust/bismark-extractor/Cargo.toml`:
      `1.0.0-alpha.9` → `1.0.0`. Description updated to "v1.0 release".
- [ ] Tag commit on `rust/iron-chancellor`:
      `git tag -a bismark-extractor-v1.0 -m "v1.0 release"`
      `git push origin bismark-extractor-v1.0`
- [ ] Comment on epic #798:
      "v1.0 tagged at `<tag commit SHA>`; matrix evidence at <gist or
      comment URL for both SE + PE speedup tables>".
- [ ] Update memory `reference_colossal_access.md` with the v1.0
      verified-on-colossal baseline numbers (analogous to the existing
      post-C.2 oxy-era baseline section).

## (Future) bismark-bedgraph v1.0 — sub-gate 2 release gates

Phase H sub-gate 2 covers byte-identity of the bedGraph / coverage /
cytosine_report streams. Currently **blocked on epic #797** (Rust
`bismark-bedgraph` crate). The streams currently pipe through Perl
`bismark2bedGraph` + `coverage2cytosine` subprocesses, so a Phase G–era
comparison would tautologically pass (both pipelines share the same Perl
producer; see memory `project_phase_h_byte_identity_ordering`).

Once epic #797 lands a Rust `bismark-bedgraph`, the extractor's
`--bedGraph` flag will switch from subprocess-to-Perl to inline-Rust,
giving two independent producers per stream. At that point sub-gate 2's
matrix work begins.

This section will be filled in when #797 lands.

## Reference

- Phase H SE plan: `plans/05262026_bismark-extractor/PHASE_H_SE_PLAN.md`
- Phase H PE plan (TODO #872): `plans/05262026_bismark-extractor/PHASE_H_PE_PLAN.md`
- SPEC: `rust/bismark-extractor/SPEC.md` §8.3 (Phase H matrix subsection),
  §9.7 (speedup target), §10 row H (sub-gate split).
- Memories: `reference_colossal_access.md`, `project_phase_h_byte_identity_ordering`.
- Epic: [#798](https://github.com/FelixKrueger/Bismark/issues/798).
