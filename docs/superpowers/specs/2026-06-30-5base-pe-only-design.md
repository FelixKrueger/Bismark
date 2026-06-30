# 5-Base (#787) → paired-end only

Date: 2026-06-30
Branch: `rust/issue-787-5base-pe-only` (based on `origin/rust/iron-chancellor`)
Related: #787 (feature), #1015 (merge), #1035 (post-merge nits)

## Motivation

The Illumina 5-Base library is paired-end. The real validation data (DRAGEN,
NA12878 100 ng, BaseSpace) is PE with a dual-UMI in the read name, and DRAGEN
documents 5-Base as directional-only. The single-end (SE) path in `bismark-aligner`
was an early scaffold: its experimental duplex/consensus variants are already
documented as degenerate non-workflows (SE OT/OB reads cover opposite fragment
ends and never pair). This change removes the SE 5-Base surface entirely and makes
`--illumina_5base` paired-end only, with a loud rejection of SE input at config
`resolve()`, following the existing `--non_directional`/`--pbat` rejection pattern.

Scope is confined to the 5-Base path. The faithful bisulfite paths stay byte-frozen;
no faithful `methylation_call` site changes. The shared `five_base_emit_record`
helper is retained (it is used by the PE per-mate emit).

## Components

### 1. Config guard (`rust/bismark-aligner/src/config.rs`)

- In `resolve()`, alongside the existing directional-only guard (around config.rs:482),
  reject `cli.five_base` with a `SingleEnd` layout: `bail!` with a clear message,
  e.g. "`--illumina_5base` is paired-end only: the 5-Base library is paired-end;
  single-end was an early scaffold and is unsupported. Provide `-1`/`-2`."
- Update the module doc comment (config.rs:7) and the "SE + directional" comment
  (config.rs:566) to "PE + directional".

### 2. Dispatch and SE code (`rust/bismark-aligner/src/lib.rs`)

- Remove `run_se_five_base` (lib.rs:1135 to ~1495) and the `ReadLayout::SingleEnd =>
  run_se_five_base(...)` dispatch arm (lib.rs:537). The SingleEnd arm becomes
  `unreachable!("5-Base SE is rejected at resolve()")` as defense in depth.
- Remove `run_five_base_duplex` SE (lib.rs:2017); keep `run_five_base_duplex_pe`
  (lib.rs:2151).
- Remove the SE branch of `run_five_base_consensus` (lib.rs:2310); keep the PE path.
  Audit `run_five_base_consensus_standalone` (lib.rs:457) for SE-only assumptions.
- Keep: `five_base_emit_record` (shared with PE), `run_five_base_deconvolution`
  (post-hoc BAM walk, strand-agnostic), and the entire PE path.

### 3. CLI / help / README (`rust/bismark-aligner/src/cli.rs`, `rust/README.md`)

- `--illumina_5base` and the `--five_base_*` flag help text: mark "(paired-end only)".
- Update the 5-Base clause and Milestones journal in `rust/README.md` to PE-only.

### 4. Tests (`rust/bismark-aligner/tests/five_base_groundtruth.rs` + unit tests)

Remove the SE ground-truth gates and SE unit tests:
- `five_base_groundtruth_real_minimap2_recovers_known_methylation` (SE core, 160)
- `five_base_deconvolution_groundtruth_variant_vs_methylation` (SE, 285)
- `five_base_duplex_groundtruth_pairs_strands_and_reconciles` (SE duplex, 393)
- `five_base_duplex_groundtruth_qname_umi_pairs_strands` (SE duplex qname, 507)
- `five_base_consensus_groundtruth_collapses_and_masks_variant` (SE consensus, 602)
- `five_base_consensus_groundtruth_real_reference_ecoli` (SE consensus, 1116)
- SE-specific module unit tests in lib.rs / five_base_duplex.rs

Port to PE (no existing PE equivalent):
- `five_base_groundtruth_illumina_spaced_header_no_desync` (1018) — the Illumina
  spaced-header desync regression. Convert its fixture to a PE pair.

Keep (PE coverage already present):
- `five_base_pe_groundtruth_real_minimap2` (core, 1264)
- `five_base_pe_duplex_groundtruth_pairs_two_pairs_per_molecule` (duplex, 739)
- `five_base_pe_consensus_groundtruth_collapses_and_masks_variant` (consensus, 861)
- `five_base_controls_deconvolution_no_false_variants` (deconvolution, 1696)
- `five_base_controls_consensus_preserves_methylation_state` (1632)
- `five_base_controls_core_recovers_lambda_and_puc19` (1524)

Remove SE-only fixtures that become orphaned.

### 5. Delivery

- Incremental commits, each validated (`cargo fmt`, `clippy -D warnings`, `cargo test`
  for the aligner), pushed after each commit.
- Open a PR against `rust/iron-chancellor` referencing #787 / #1015 / #1035; merge
  when CI is green (no blocking re-review).

## Out of scope (YAGNI)

- No refactor of the PE 5-Base path.
- No change to any byte-frozen bisulfite path.
- No new 5-Base capability.

## Verification

- `cargo fmt -p bismark-aligner -- --check`, `cargo clippy -p bismark-aligner
  --all-targets --features binseq-input,rammap-inprocess -- -D warnings`, and
  `cargo test -p bismark-aligner` all clean.
- The ground-truth gates fail loud if minimap2 is absent; CI installs minimap2.
- `perl-oracle` byte-identity gate stays green (no faithful path touched).
- Confirm `rg "run_se_five_base|five_base.*SingleEnd"` returns no live call after removal.
