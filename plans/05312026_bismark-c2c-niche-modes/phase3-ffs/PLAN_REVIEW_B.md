# Phase 3 PLAN review — `--ffs` (tetra/penta/hexamer columns) — Reviewer B

**Target:** `plans/05312026_bismark-c2c-niche-modes/phase3-ffs/PLAN.md` (rev 0)
**Reviewer:** B (independent; no shared state with Reviewer A)
**Date:** 2026-05-31
**Worktree:** `/Users/fkrueger/Github/Bismark-c2c` (branch `rust/c2c-v1x`)

## Verdict: **APPROVE-WITH-CHANGES**

The plan is technically sound and its byte-identity crux — the §3.2 offset table — is **correct**. I independently re-derived the offset table from the Perl source (not from the plan) and ran live Perl v0.25.1 on my own from-scratch fixtures: **every load-bearing claim held byte-for-byte**, including the two highest-risk ones (forward-hexa negative-wrap, reverse-hexa `i-3` offset). The required changes are **not logic fixes** — they are about the plan's **stale post-Phase-1 line numbers and signatures** (the plan was written against the pre-Phase-1 `report.rs`/`cli.rs`), plus two small test-surface gaps the implementer must not miss. No Critical correctness defect.

- **Critical:** 0
- **Important:** 4 (all staleness / test-surface; no logic change)
- **Optional:** 3

---

## Live-Perl checks I ran (independent fixtures — NOT the plan's)

All runs used the repo-root `./coverage2cytosine` (v0.25.1) on fixtures I built from scratch.

| # | Fixture (genome / cov) | Command | Result |
|---|------------------------|---------|--------|
| B1 | `chrA=CCGAATTGCGAACG`, `chrB=GCATCGGCC`, `chrC=AAACG`; cov chrA{1,9},chrB{6} | `--CX --ffs` | 10-col report; **byte-identical** to my independent Python re-derivation of the §3.2 table (`diff` empty) |
| B2 | (B1) forward hexa at `i=0` (pos1) & `i=1` (pos2) | (B1 output) | **negative-wrap confirmed**: i=0 hexa=`CG` (last 2 chars), i=1 hexa=`G` (last 1 char) — NOT clamped/empty |
| B3 | (B1) reverse hexa `chrA i=2`(pos3) & `i=7`(pos8) | (B1 output) | i=2 hexa=`""` (guard `pos-4≥0` fails), i=7 hexa=`CGCAAT`=`revcomp(seq[4..10])` → confirms offset `pos-4=i-3` (NOT `i-2`) |
| B4 | (B1) empty windows | (B1 output) | `chrA pos3`: `…CGG\t\t\t\n` (all-three-empty mid/end); `chrZ pos5`: `…CGTT\t\tTACGTT` (empty penta between tabs) — nothing-between-tabs confirmed |
| B5 | `chrU=ACGTACGTAC`(covered), `chrZ=CGTACGTT`(uncovered) | `--CX --ffs` | **uncovered chr emits 10-col `0 0` ffs lines** (chrZ pos1/5/6) — §3.1/V13 confirmed |
| B6 | (B1) | `--CX --ffs` vs `--CX` summary | `*.cytosine_context_summary.txt` **byte-identical** — §3.5/V5 confirmed |
| B7 | (B1) | `--ffs` (CpG-only) | 10-col, **CG context only** — §3.1 confirmed |
| B8 | (B1) | `--CX --ffs --zero_based` | context cols 6–10 **byte-identical** to 1-based (only `pos` shifts) — §3.1 confirmed |
| B9 | `chrM=AACGTTACGAACGTT`; CpG-pair cov | `--ffs --merge_CpGs` vs `--merge_CpGs` | **allowed (no mutex, exit 0)**; `*.merged_CpG_evidence.cov` **byte-identical** — §3.6/V6 confirmed |
| B10 | (B1) | `--CX --ffs --split_by_chromosome` | per-chr `sp.chrchrA.CX_report.txt` 10-col, v1.0 `.chr<NAME>` infix — §3.1 confirmed |

**My from-scratch offset table (re-derived directly from Perl `:262-330`, `:507-585`, `:1421-1493`) — matches the plan's §3.2 exactly:**

| field | forward (C, strand `+`) | reverse (G, strand `-`, then revcomp) |
|-------|--------------------------|----------------------------------------|
| tetra | `substr(i,4)`, guard `len≥i+4` | `substr(i-3,4)`, guard `i≥3` |
| penta | `substr(i,5)`, guard `len≥i+5` | `substr(i-4,5)`, guard `i≥4` |
| hexa  | `substr(i-2,6)` **(signed → negative-wrap at i=0,1)**, guard `len≥i+4` | `substr(i-3,6)`, guard `i≥3` |

Confirmed in all three Perl blocks (covered `:262-330`, last-chr `:507-585`, uncovered `:1421-1493`) — the offsets are identical across blocks, so the plan's "collapse to one Rust kernel" is justified.

---

## Findings by area

### Logic — sound

- **§3.2 offset table: CORRECT.** Independently re-derived and live-diffed (B1). The forward-hexa signed offset `i-2` with Perl negative-wrap (B2) and the reverse-hexa `pos-4=i-3` offset with the `pos-4≥0` guard (B3) are both faithfully captured. The plan's explicit warning "do NOT clamp the forward-hexa negative offset to 0" is exactly right and is the single subtlest point.
- **Empty-window rendering (§3.3): CORRECT** (B4). Empty field = nothing-between-tabs; an all-three-empty chr-end line renders `…\t{tri}\t\t\t\n`.
- **Scope (§3.1): CORRECT.** CpG-only AND `--CX` (B7), covered AND uncovered (B5 — the uncovered pass emits 10-col `0 0` lines), orthogonal to `--zero_based` (B8) and `--split` (B10).
- **Merge (§3.6): CORRECT** (B9). No Perl mutex; merged cov byte-identical. I read the current Rust `merge::parse_report_row` (`merge.rs:47-72`): it requires `f.len() >= 6` and indexes only `f[0..=5]`, so it tolerates the 10-col report unchanged — **no `merge.rs` edit needed**, as the plan states.
- **Guard ordering (§3.4): CORRECT.** Fields are advisory and never gate; computing them eagerly in `extract` matches Perl (which computes `$tetra_nt`/etc. at the top of the loop, before the guards).

### Assumptions — one is stale, the rest verified

All ten §8 assumptions are individually verified above **except** that the plan's "Context" (§2) and "Implementation outline" (§5) describe the **pre-Phase-1** code. Phase 1 (`--gc`/`--nome-seq`) landed and reshaped exactly the functions Phase 3 touches. The mechanics (append-3-columns) still hold, but every cited line number and the two key signatures are wrong now (see Important-1).

### Efficiency — fine

`--ffs` adds ≤3 short `perl_substr` slices + ≤3 `revcomp` allocations per emitted position — O(1), negligible. Gating computation on `ffs` keeps the default hot path untouched (§6 is correct). The `Vec<u8>`-vs-`SmallVec` note is appropriately deferred.

### Validation sufficiency — strong, two gaps

V0–V14 cover the high-risk paths well: forward-hexa negative-wrap (V2), reverse-hexa empties (V4), all-empty chr-end (V3), uncovered 10-col line (V13), summary/merge invariance (V5/V6), the full flag matrix (V8–V12). See Important-3/Important-4 for the two test-surface gaps the implementer must honor or the suite breaks.

### Alternatives — none needed

Named `Extracted` struct vs extended tuple is the only open design choice (non-behavioral, §10). Given the post-Phase-1 reality where `extract` is a private 3-tuple with a single interior caller, either is trivial. No alternative architecture is warranted — append-only on the shipped kernel is correct.

---

## Action items

### Important

- **Important-1 — STALE line numbers AND signatures (post-Phase-1).** The plan was written against pre-Phase-1 `report.rs`/`cli.rs`. The implementer must **re-locate** every call site; the cited lines are all wrong, and two signatures changed:
  - `report.rs`: `perl_substr` `:91`→**99**; `revcomp` `:107`→**115**; `extract` `:137`→**145** and now returns a bare 3-tuple `(Vec<u8>, Vec<u8>, u8)` with **no params besides `(seq, i)`**; `emit_position` `:161`→**169** and now **already takes `nome: bool` AND `cov_out: &mut Vec<u8>`** (Phase-1 additions) — `ffs` must be threaded **alongside `nome`**, not into the signature the plan prints in §4; `chromosome_report_bytes` `:226`→**264** and now **returns `(Vec<u8>, Vec<u8>)`** (report + cov), taking `config` — add `config.ffs` to the single `emit_position` call there (`:279-292`); `run_single` `:275`→**316**; `run_split` `:337`→**406**; `flush_split_chromosome` `:400`→**471**.
  - `cli.rs`: `ffs` field declared `:98-99`→**99-101**; rejection block `:158-160`→**159-161** (it now rejects only `--drach` + `--ffs`; deleting the `--ffs` arm leaves `--drach`); the `ResolvedConfig` insertion point (plan says `:103`) is now occupied by the Phase-1 fields (`gc_context`, `nome`) — append `ffs` to the struct (`cli.rs:106-143`) **and** to the constructor (`cli.rs:234-251`).
  - **Recommend:** add a one-line note to the plan ("§2/§4/§5 line numbers are pre-Phase-1; re-grep `extract`/`emit_position`/`chromosome_report_bytes`; thread `ffs` next to `nome`") rather than rewriting every number.

- **Important-2 — the `run_t`/`run_nome` test harness signature must be updated.** `emit_position`'s test driver in `report.rs` is now **`run_nome`** (`report.rs:684-721`), not the `run_t`/`run` the plan references (those are thin wrappers over `run_nome`). Adding the `ffs` param to `emit_position` breaks the `run_nome` call site (`:701-714`). The implementer must thread `ffs` through `run_nome` (default `false`) so the existing `run`/`run_t` wrappers stay green, then add ffs-specific assertions (V1–V4).

- **Important-3 — the CLI rejection test edit is precise but the cited line is stale.** The `("--ffs","ffs")` entry lives in the **`rejects_v1x_flags`** test (`cli.rs:320-332`, not `:303`), in the loop `for (flag, frag) in [("--drach","drach"), ("--ffs","ffs")]`. Phase 3 must drop **only** the `("--ffs","ffs")` tuple and keep `("--drach","drach")` (Phase 2 owns DRACH). If the implementer removes the whole loop or the wrong entry, either the suite fails or `--drach`'s rejection loses coverage.

- **Important-4 — adding a field to `ResolvedConfig` breaks an in-test struct literal.** There is a literal `ResolvedConfig { … }` construction in the `nome_cov_path_uses_raw_base` test (`report.rs:875-892`) that enumerates every field. Adding `pub ffs: bool` to the struct will fail that test to compile until `ffs: false` is added there too. The plan's §5 task 5 lists the struct + constructor but not this test literal — call it out so `cargo test` doesn't fail to build.

### Optional

- **Optional-1 — golden-test naming convention.** The repo uses `tests/golden_phase1.rs`, `golden_phase_b/c/d.rs`. The plan proposes `tests/golden_phase3_ffs.rs`; harmless, but `golden_phase3.rs` would match the established pattern. There is **no single** `generate_goldens.sh` — each phase has its own under `tests/data/<phase>/generate_goldens.sh`. The implementer should create `tests/data/phase3_ffs/generate_goldens.sh` modeled on `tests/data/phase1/generate_goldens.sh` (which embeds `C2C="$(cd "$HERE/../../../../.." && pwd)/coverage2cytosine"` and `set -eo pipefail`).

- **Optional-2 — a chr-start short-scaffold reverse case is worth one more golden.** V4 pins reverse penta-empty at `i=3`. Consider also pinning a reverse `G` at `i=2` (where reverse **tetra and hexa** are empty because `pos-4<0`) — my B3 hit exactly this (`chrA pos3` → all three reverse fields empty). It's covered transitively by the `--CX --ffs` golden (V9) if the fixture contains such a position, but an explicit `extract`-level unit assertion would make the reverse-edge guard self-documenting.

- **Optional-3 — note the genome-folder fixture format.** The Perl reads the genome from `<*.fa>`/`<*.fasta>` in `--genome_folder` (`read_genome_into_memory:1648-1672`); goldens must place the FASTA in a directory, not pass a file. The phase1 generator already does this (`mkfa` writes `genome.fa` into a per-fixture dir); the plan's §5 task 6 should mirror it. (Minor — the implementer will copy the phase1 pattern anyway.)

---

## Bottom line

The offset arithmetic — the only part that could silently produce wrong bytes — is **independently verified correct against live Perl v0.25.1**, including the two trap cases. The plan is a clean append-only extension of the shipped kernel with no Critical defect. The required changes are entirely about re-syncing to the **post-Phase-1** `report.rs`/`cli.rs` (stale line numbers, the `nome`/`cov_out` signatures, the `run_nome` harness, the `rejects_v1x_flags` loop, and the in-test `ResolvedConfig` literal). Address Important-1..4 during implementation (or fold a "lines are pre-Phase-1, re-grep" note into the plan) and proceed.
