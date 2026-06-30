# 5-Base paired-end-only Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `bismark-aligner`'s Illumina 5-Base (#787) path paired-end only: reject single-end input loudly at config `resolve()` and remove all single-end 5-Base code and tests.

**Architecture:** 5-Base is a paired-end library (DRAGEN; NA12878 BaseSpace). The single-end path was an early scaffold with degenerate duplex/consensus variants. We add an SE rejection guard (mirroring the existing `--non_directional`/`--pbat` 5-Base rejection), delete the SE alignment/duplex/consensus code, drop the now-constant `paired_end` parameter from the consensus walk, flip the help/README text to PE-only, remove the SE ground-truth gates, and port the two SE-only regression gates (Illumina spaced-header desync; lambda/pUC19 controls) to PE.

**Tech Stack:** Rust (edition 2024, MSRV 1.89), `noodles` for BAM/SAM, minimap2/bowtie2/hisat2 external aligners, `cargo test`/`clippy`/`fmt`.

## Global Constraints

- Touch only the 5-Base path. Do NOT alter any byte-frozen bisulfite path; every faithful `methylation_call` site keeps `five_base = false`. The `perl-oracle` gate must stay green.
- `cargo clippy -p bismark-aligner --all-targets --features binseq-input,rammap-inprocess -- -D warnings` is a hard gate (no dead code, no unused functions).
- `cargo fmt --all` clean.
- Never silent: SE 5-Base must produce a loud error, not a degraded run.
- Keep the shared helper `five_base_emit_record` (used by the PE per-mate emit) and `five_base_reference_fasta` (used by PE).
- Commit each task as its own small commit and `git push` after every commit.
- Ground-truth gates are `#[ignore]`d and fail loud if minimap2 is absent; CI installs minimap2.

---

### Task 1: Reject single-end 5-Base loudly

**Files:**
- Modify: `rust/bismark-aligner/src/config.rs` (guard block at 477-508; stale doc-comments 200, 203, 277, 1491)
- Modify: `rust/bismark-aligner/src/lib.rs` (standalone consensus SE probe, 477-518)
- Test: `rust/bismark-aligner/src/config.rs` (tests module, near `five_base_duplex_guards` at 1492)

**Interfaces:**
- Consumes: `resolve(cli: &Cli, command_line: String) -> Result<RunConfig>`; `layout: ReadLayout` (resolved at config.rs:475); `ReadLayout::{SingleEnd, PairedEnd}`.
- Produces: SE 5-Base now returns `Err(AlignerError::Unsupported(...))` containing the text `"--illumina_5base is paired-end only"`.

- [ ] **Step 1: Write the failing test**

In the `config.rs` tests module (after `five_base_duplex_guards`, ~line 1518), add:

```rust
    /// `--illumina_5base` is paired-end only: single-end input is rejected loud.
    #[test]
    fn illumina_5base_rejects_single_end() {
        let err = resolve(&cli_from(&["--illumina_5base", "reads.fq"]), "cmd".into()).unwrap_err();
        assert!(
            err.to_string().contains("--illumina_5base is paired-end only"),
            "got: {err}"
        );
        // PE still resolves past the SE guard (may fail later for no genome, but not here).
        if let Err(e) = resolve(
            &cli_from(&["--illumina_5base", "-1", "r1.fq", "-2", "r2.fq"]),
            "cmd".into(),
        ) {
            assert!(
                !e.to_string().contains("paired-end only"),
                "PE 5-Base must not hit the SE guard: {e}"
            );
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p bismark-aligner --lib illumina_5base_rejects_single_end`
Expected: FAIL — SE currently resolves (no `"paired-end only"` error produced).

- [ ] **Step 3: Add the SE rejection guard**

In `config.rs`, inside the `if cli.illumina_5base {` block, replace the comment at 477-480 and add the guard right after the `non_directional || pbat` check (after line 488):

Replace:
```rust
    // --illumina_5base (#787) v1 scope guards. The 5-Base path is single-end +
    // directional, FASTQ, single-instance only; everything else is a deferred
    // follow-up phase. Reject loudly (never silently degrade) BEFORE the generic
    // minimap2 guards so the error names --illumina_5base, not --minimap2.
    if cli.illumina_5base {
        if cli.non_directional || cli.pbat {
            return Err(AlignerError::Unsupported(
                "--illumina_5base is directional only in v1 (drop --non_directional/--pbat): \
                 the 5-Base library is directional."
                    .into(),
            ));
        }
```
with:
```rust
    // --illumina_5base (#787) scope guards. The 5-Base path is paired-end +
    // directional, FASTQ, single-instance only; everything else is rejected.
    // Reject loudly (never silently degrade) BEFORE the generic minimap2 guards
    // so the error names --illumina_5base, not --minimap2.
    if cli.illumina_5base {
        if matches!(layout, ReadLayout::SingleEnd { .. }) {
            return Err(AlignerError::Unsupported(
                "--illumina_5base is paired-end only: the 5-Base library is paired-end. \
                 Provide -1/-2 (single-end was an early scaffold and is no longer supported)."
                    .into(),
            ));
        }
        if cli.non_directional || cli.pbat {
            return Err(AlignerError::Unsupported(
                "--illumina_5base is directional only in v1 (drop --non_directional/--pbat): \
                 the 5-Base library is directional."
                    .into(),
            ));
        }
```

- [ ] **Step 4: Reject SE BAM in the standalone consensus**

In `lib.rs` `run_five_base_consensus_standalone`, after the `paired_end` probe (after line 489), add:

```rust
    if !paired_end {
        return Err(AlignerError::Validation(format!(
            "5-Base consensus is paired-end only: {} looks single-end (FLAG 0x1 unset). \
             5-Base is a paired-end library.",
            bams[0].display()
        )));
    }
```

- [ ] **Step 5: Fix stale SE doc-comments**

In `config.rs`, update these doc-comments (text only):
- Line 1491: `/// `--five_base_duplex` requires `--illumina_5base` and is SE-only (rejects `-2`).` → `/// `--five_base_duplex` requires `--illumina_5base` and is paired-end only.`
- Line 200: `... reconciliation). SE + PE (PE is the real workflow). Requires ...` → `... reconciliation). Paired-end only. Requires ...`
- Line 203: `... (implies `five_base_duplex`). SE + PE (PE is the real workflow). Requires ...` → `... (implies `five_base_duplex`). Paired-end only. Requires ...`
- Line 277: `... the faithful per-strand instances. SE directional only this phase; the` → `... the faithful per-strand instances. PE directional only; the`
- Line 566 (config.rs — `// #787 5-Base mode (guarded above: SE + directional, ...`): change `SE + directional` → `PE + directional`. (Confirm with `rg -n "SE \+ directional" rust/bismark-aligner/src`.)

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test -p bismark-aligner --lib illumina_5base_rejects_single_end`
Expected: PASS.

- [ ] **Step 7: Confirm the existing PE guard tests still pass**

Run: `cargo test -p bismark-aligner --lib five_base`
Expected: PASS (incl. `five_base_duplex_guards`, `five_base_consensus_guards_and_implies_duplex`).

- [ ] **Step 8: Commit and push**

```bash
git add rust/bismark-aligner/src/config.rs rust/bismark-aligner/src/lib.rs
git commit -m "feat(5base): reject single-end --illumina_5base loud at resolve() (#787)"
git push -u origin rust/issue-787-5base-pe-only
```

---

### Task 2: Remove the single-end 5-Base alignment + duplex code

**Files:**
- Modify: `rust/bismark-aligner/src/lib.rs` (dispatch arm 537; remove `run_se_five_base` 1135-1256; remove `run_five_base_duplex` SE 2017-2150)

**Interfaces:**
- Consumes: `config.layout: ReadLayout`; `run_pe_five_base(config, mates1, mates2)` (retained).
- Produces: no `run_se_five_base` / `run_five_base_duplex` (SE) symbols remain. `run_five_base_duplex_pe` is the only duplex driver.

- [ ] **Step 1: Make the SE dispatch arm unreachable**

In `lib.rs` (~537), replace:
```rust
    if config.five_base {
        match &config.layout {
            ReadLayout::SingleEnd { reads } => return run_se_five_base(config, reads),
            ReadLayout::PairedEnd { mates1, mates2 } => {
                return run_pe_five_base(config, mates1, mates2);
            }
        }
    }
```
with:
```rust
    if config.five_base {
        match &config.layout {
            // 5-Base single-end is rejected at config::resolve(); never reached here.
            ReadLayout::SingleEnd { .. } => {
                unreachable!("5-Base single-end is rejected at config::resolve()")
            }
            ReadLayout::PairedEnd { mates1, mates2 } => {
                return run_pe_five_base(config, mates1, mates2);
            }
        }
    }
```

- [ ] **Step 2: Delete `run_se_five_base`**

Delete the entire `fn run_se_five_base(config: &RunConfig, reads: &[String]) -> Result<()>` (lib.rs:1135 through the line before `fn five_base_reference_fasta` at 1257). This removes the only caller of `run_five_base_duplex` (SE) and the SE call of `run_five_base_consensus`.

- [ ] **Step 3: Delete `run_five_base_duplex` (SE)**

Delete the entire `fn run_five_base_duplex(` (lib.rs:2017 through the line before `fn run_five_base_duplex_pe` at 2151). Keep `run_five_base_duplex_pe`.

- [ ] **Step 4: Build and clippy**

Run: `cargo build -p bismark-aligner && cargo clippy -p bismark-aligner --all-targets --features binseq-input,rammap-inprocess -- -D warnings`
Expected: clean (no unused-function / dead-code warnings for `run_se_five_base`, `run_five_base_duplex`).

- [ ] **Step 5: Confirm symbols are gone**

Run: `rg -n "run_se_five_base|fn run_five_base_duplex\b" rust/bismark-aligner/src`
Expected: no matches except `run_five_base_duplex_pe`.

- [ ] **Step 6: Run the aligner unit tests**

Run: `cargo test -p bismark-aligner --lib`
Expected: PASS.

- [ ] **Step 7: Commit and push**

```bash
git add rust/bismark-aligner/src/lib.rs
git commit -m "refactor(5base): remove single-end alignment + SE duplex path (#787)"
git push
```

---

### Task 3: Make the consensus walk paired-end only

**Files:**
- Modify: `rust/bismark-aligner/src/lib.rs` (`run_five_base_consensus` signature + SE branch, 2310-2669; PE caller ~1661; standalone caller ~510)

**Interfaces:**
- Consumes: callers `run_pe_five_base` (passes `true`) and `run_five_base_consensus_standalone` (now SE-rejected in Task 1).
- Produces: `run_five_base_consensus(genome, refid, bam_paths, consensus_bam_path, header, umi_swap, min_mapq)` — the `paired_end: bool` parameter is removed; the body always uses the PE branch.

- [ ] **Step 1: Drop the `paired_end` parameter from the signature**

In `lib.rs:2310`, remove the `paired_end: bool,` parameter line so the signature becomes:
```rust
fn run_five_base_consensus(
    genome: &Genome,
    refid: &HashMap<String, usize>,
    bam_paths: &[&Path],
    consensus_bam_path: &Path,
    header: &noodles_sam::Header,
    umi_swap: Option<crate::five_base_duplex::UmiSwap>,
    min_mapq: u8,
) -> Result<()> {
```

- [ ] **Step 2: Collapse the two `paired_end` branch points in the body**

Within `run_five_base_consensus` (2310-2669) there are exactly two uses of `paired_end` besides the parameter:
- An `if paired_end { ... }` block: keep the `true` branch unconditionally, delete the `else` (SE) branch.
- A label `if paired_end { "PE" } else { "SE" }`: replace with the literal `"PE"`.

Use `rg -n "paired_end" rust/bismark-aligner/src/lib.rs` to confirm only the intended sites remain after editing.

- [ ] **Step 3: Update the PE caller**

In `run_pe_five_base` (~1661), remove the `true, // PE` argument line from the `run_five_base_consensus(...)` call.

- [ ] **Step 4: Update the standalone caller**

In `run_five_base_consensus_standalone` (~510), remove the `paired_end,` argument from the call. The `paired_end` local is still used by the Task 1 SE-reject guard and the `eprintln!` note — keep the local; if the `eprintln!` still prints the `if paired_end {"PE"} else {"SE"}` label, change it to the literal `"PE"` (it is always PE past the guard).

- [ ] **Step 5: Build and clippy**

Run: `cargo build -p bismark-aligner && cargo clippy -p bismark-aligner --all-targets --features binseq-input,rammap-inprocess -- -D warnings`
Expected: clean (no unused `paired_end`).

- [ ] **Step 6: Run the aligner unit tests**

Run: `cargo test -p bismark-aligner --lib`
Expected: PASS.

- [ ] **Step 7: Commit and push**

```bash
git add rust/bismark-aligner/src/lib.rs
git commit -m "refactor(5base): drop paired_end param from consensus walk — PE only (#787)"
git push
```

---

### Task 4: Flip help and README to paired-end only

**Files:**
- Modify: `rust/bismark-aligner/src/cli.rs` (five_base help, anchors at 141-150 and the `--illumina_5base` flag itself)
- Modify: `rust/README.md` (5-Base clause + Milestones journal)

**Interfaces:**
- Consumes: nothing (doc/help text only).
- Produces: `--help` and README describe 5-Base as paired-end only.

- [ ] **Step 1: Update the `--illumina_5base` flag help**

Run `rg -n "illumina_5base" rust/bismark-aligner/src/cli.rs` to find the `--illumina_5base` flag doc-comment. Edit it so the supported-scope sentence reads "paired-end only (the 5-Base library is paired-end)" and remove any "SE + PE" / "single-end" wording.

- [ ] **Step 2: Update the duplex/consensus help (cli.rs:137-150)**

Rewrite the `five_base_duplex` help (around 137-150) to drop the SE-duplex "KNOWN LIMITATION" paragraph (no longer relevant) and state the duplex pairs the two strands of a paired-end molecule by fragment span. Change the `five_base_consensus` help at line 150 from "(SE + PE; PE is the real workflow)" to "(paired-end only)".

- [ ] **Step 3: Update README**

Run `rg -n "5-Base|five_base|illumina_5base" rust/README.md`. In the aligner-row 5-Base clause and the dated Milestones entry for #1015, change any "SE + PE" / "single-end and paired-end" wording to "paired-end only", and add a one-line journal entry dated 2026-06-30: "5-Base narrowed to paired-end only (single-end scaffold removed); #787 follow-up."

- [ ] **Step 4: Verify help builds and renders**

Run: `cargo run -p bismark-aligner --bin bismark_rs -- --help 2>&1 | rg -i "5-base|illumina_5base"`
Expected: text shows "paired-end only"; no "single-end" claim for 5-Base.

- [ ] **Step 5: fmt + clippy**

Run: `cargo fmt --all && cargo clippy -p bismark-aligner --all-targets --features binseq-input,rammap-inprocess -- -D warnings`
Expected: clean.

- [ ] **Step 6: Commit and push**

```bash
git add rust/bismark-aligner/src/cli.rs rust/README.md
git commit -m "docs(5base): mark --illumina_5base paired-end only in help + README (#787)"
git push
```

---

### Task 5: Remove the single-end ground-truth gates

**Files:**
- Modify: `rust/bismark-aligner/tests/five_base_groundtruth.rs` (remove SE gate fns; remove orphaned SE helpers)

**Interfaces:**
- Consumes: shared helpers `gen_reference`, `write_genome`, `cpg_positions`, `make_methylated_reads`, `revcomp`, `bin`, `have_minimap2`, `real_genome_gz`, `load_first_contig_gz` (retained — used by PE gates and the Task 6 ports).
- Produces: only PE ground-truth gates remain (plus the two PE ports from Task 6).

- [ ] **Step 1: Delete the SE ground-truth test functions**

Delete these entire `#[test]` functions from `five_base_groundtruth.rs` (each have a PE equivalent):
- `five_base_groundtruth_real_minimap2_recovers_known_methylation` (~160) — PE equivalent: `five_base_pe_groundtruth_real_minimap2`.
- `five_base_deconvolution_groundtruth_variant_vs_methylation` (~285) — PE equivalent: `five_base_controls_deconvolution_no_false_variants`.
- `five_base_duplex_groundtruth_pairs_strands_and_reconciles` (~393) — PE equivalent: `five_base_pe_duplex_groundtruth_pairs_two_pairs_per_molecule`.
- `five_base_duplex_groundtruth_qname_umi_pairs_strands` (~507) — covered by the PE duplex gate + `canonical_umi` unit tests.
- `five_base_consensus_groundtruth_collapses_and_masks_variant` (~602) — PE equivalent: `five_base_pe_consensus_groundtruth_collapses_and_masks_variant`.
- `five_base_consensus_groundtruth_real_reference_ecoli` (~1116) — PE equivalent: `five_base_controls_consensus_preserves_methylation_state`.

- [ ] **Step 2: Build the test target and let clippy flag orphaned helpers**

Run: `cargo test -p bismark-aligner --test five_base_groundtruth --no-run 2>&1 | rg -i "never used|unused"`
Expected: `emit_se_control` reported unused (its only callers were `five_base_controls_core_recovers_lambda_and_puc19`, ported in Task 6 — so it may still show used until Task 6; if so, defer its removal to Task 6 Step 5). `make_methylated_reads` should still be used (by `five_base_groundtruth_illumina_spaced_header_no_desync`, ported in Task 6) — keep it.

- [ ] **Step 3: Run the remaining gates (with minimap2 present)**

Run: `cargo test -p bismark-aligner --test five_base_groundtruth -- --ignored`
Expected: the remaining PE gates PASS; no SE gate runs.

- [ ] **Step 4: Confirm no SE gate names remain**

Run: `rg -n "fn five_base_(groundtruth_real_minimap2_recovers|deconvolution_groundtruth|duplex_groundtruth|consensus_groundtruth)" rust/bismark-aligner/tests/five_base_groundtruth.rs`
Expected: no matches (these were the SE fns; PE fns use `_pe_` or `_controls_`).

- [ ] **Step 5: Commit and push**

```bash
git add rust/bismark-aligner/tests/five_base_groundtruth.rs
git commit -m "test(5base): remove single-end ground-truth gates (PE equivalents exist) (#787)"
git push
```

---

### Task 6: Port the two SE-only regression gates to paired-end

**Files:**
- Modify: `rust/bismark-aligner/tests/five_base_groundtruth.rs` (rewrite `five_base_groundtruth_illumina_spaced_header_no_desync` and `five_base_controls_core_recovers_lambda_and_puc19` as PE; remove `emit_se_control`)

**Interfaces:**
- Consumes: `make_methylated_reads`, `gen_reference`, `write_genome`, `revcomp`, `bin`, `have_minimap2`, `load_controls_or_skip`, `emit_pe_control_duplex`, `count_cpg_keyed`, `pct_meth`.
- Produces: PE versions of both regression gates; `emit_se_control` removed.

- [ ] **Step 1: Port the Illumina spaced-header desync gate to PE**

Rewrite `five_base_groundtruth_illumina_spaced_header_no_desync` (~1018) to drive a paired-end run. Generate reads with `make_methylated_reads`, write R1 to `r1.fq` with each header line followed by a space and ` 1:N:0:GTAACTGAAG+TCNCGACTCC`, and R2 to `r2.fq` from `revcomp` of each read with ` 2:N:0:GTAACTGAAG+TCNCGACTCC`. Invoke `bin()` with `--illumina_5base -1 r1.fq -2 r2.fq` (mirror the `-1`/`-2` invocation in `five_base_pe_groundtruth_real_minimap2`). Keep the assertion that the run succeeds and the emitted BAM record count matches the input pair count (no lockstep desync from the spaced header).

- [ ] **Step 2: Port the lambda/pUC19 controls core gate to PE**

Rewrite `five_base_controls_core_recovers_lambda_and_puc19` (~1524) to emit paired-end control reads instead of `emit_se_control`. Mirror the PE control emission already used by `five_base_controls_consensus_preserves_methylation_state` (`emit_pe_control_duplex`, ~1582): write a `-1`/`-2` pair for lambda (unmethylated) and pUC19 (CpG-methylated), run `--illumina_5base -1 .. -2 ..`, then assert lambda CpG methylation is near-zero and pUC19 CpG methylation is high using `count_cpg_keyed` + `pct_meth`.

- [ ] **Step 3: Remove the now-orphaned `emit_se_control`**

Delete `fn emit_se_control(` (~1457). It has no remaining caller after Step 2.

- [ ] **Step 4: Build the test target and clippy**

Run: `cargo test -p bismark-aligner --test five_base_groundtruth --no-run && cargo clippy -p bismark-aligner --all-targets --features binseq-input,rammap-inprocess -- -D warnings`
Expected: clean (no unused `emit_se_control`; `make_methylated_reads` still used by the ported header test).

- [ ] **Step 5: Run both ported gates**

Run: `cargo test -p bismark-aligner --test five_base_groundtruth -- --ignored five_base_groundtruth_illumina_spaced_header_no_desync five_base_controls_core_recovers_lambda_and_puc19`
Expected: both PASS.

- [ ] **Step 6: Commit and push**

```bash
git add rust/bismark-aligner/tests/five_base_groundtruth.rs
git commit -m "test(5base): port spaced-header + lambda/pUC19 control gates to PE (#787)"
git push
```

---

### Task 7: Final verification and open the PR

**Files:** none (verification + delivery)

**Interfaces:** the full aligner workspace builds, lints, and tests clean; PR opened against `rust/iron-chancellor`.

- [ ] **Step 1: Full fmt + clippy + test**

Run:
```bash
cargo fmt --all -- --check
cargo clippy -p bismark-aligner --all-targets --features binseq-input,rammap-inprocess -- -D warnings
cargo test -p bismark-aligner
```
Expected: all clean / PASS.

- [ ] **Step 2: Run the ignored ground-truth gates (minimap2 installed)**

Run: `cargo test -p bismark-aligner --test five_base_groundtruth -- --ignored`
Expected: all remaining PE gates + the two ports PASS.

- [ ] **Step 3: Confirm no single-end 5-Base surface remains**

Run: `rg -n "run_se_five_base|single.end" rust/bismark-aligner/src | rg -i "5.base|five_base"`
Expected: no live SE 5-Base references (only the resolve() rejection guard, the `unreachable!` arm, and the standalone SE-reject error text).

- [ ] **Step 4: Confirm the byte-frozen path is untouched**

Run: `git diff origin/rust/iron-chancellor --stat -- rust/bismark-aligner/src/methylation.rs`
Expected: no changes to `methylation.rs` (the faithful `methylation_call` polarity site), confirming the bisulfite path is byte-frozen. If any change appears, revert it.

- [ ] **Step 5: Open the PR**

```bash
gh pr create --base rust/iron-chancellor --head rust/issue-787-5base-pe-only \
  --title "refactor(5base): make Illumina 5-Base paired-end only (#787)" \
  --body "$(cat <<'EOF'
Follow-up to #1015 / #1035. The Illumina 5-Base library is paired-end (DRAGEN; NA12878 BaseSpace, dual-UMI in the read name). The single-end path was an early scaffold with degenerate duplex/consensus variants. This PR makes `--illumina_5base` paired-end only:

- **resolve() guard:** SE input is rejected loud ("`--illumina_5base` is paired-end only"); the standalone consensus-from-BAM also rejects single-end BAMs.
- **Code removed:** `run_se_five_base`, the SE `run_five_base_duplex`, and the SE branch of the consensus walk (the `paired_end` parameter is dropped — always PE).
- **Help/README:** 5-Base flagged paired-end only.
- **Tests:** SE ground-truth gates removed (all had PE equivalents); the two SE-only regression gates (Illumina spaced-header desync; lambda/pUC19 controls) ported to PE.

Byte-frozen bisulfite paths untouched (`methylation.rs` unchanged; `perl-oracle` green). Ground-truth PE gates pass with minimap2 installed.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 6: Verify CI is green, then merge when green**

Run: `gh pr checks --watch`
Expected: all checks green; merge per "PR directe, merge quand vert".
