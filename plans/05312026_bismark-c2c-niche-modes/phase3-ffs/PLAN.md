# Phase 3 PLAN — `--ffs` (tetra/penta/hexamer nucleotide-context columns)

**Epic:** `05312026_bismark-c2c-niche-modes/EPIC.md`, Phase 3 — `--ffs` (FFS context columns)
**Design contract:** the v1.0 [`../../05292026_bismark-coverage2cytosine/SPEC.md`](../../05292026_bismark-coverage2cytosine/SPEC.md) — §7 (coordinate arithmetic; the `tri_nt` model this phase extends), §7.1 (`perl_substr` negative-wrap), §5 (output topology / report-line shape).
**Status:** rev 1 (2026-05-31) — **dual plan-review folded** (both `PLAN_REVIEW_A.md` + `PLAN_REVIEW_B.md` APPROVE-WITH-CHANGES, **0 Critical**; both independently re-derived the §3.2 offset table from the Perl source and diffed it byte-for-byte against live Perl v0.25.1 — the offset arithmetic, the forward-hexa negative-wrap, and the reverse-hexa `i-3` are all confirmed correct). The folded changes are NOT logic fixes — they are the **post-Phase-1 rebase** (stale line numbers + the `emit_position`/`chromosome_report_bytes` signatures now carry Phase 1's `nome`/`cov_out`; see the §2 rebase note), the precise CLI-rejection-test narrowing (§3.7), and **one real gotcha: the `--help` "Ns ignored" claim is FALSE** — N-windows are emitted verbatim, do not filter (§3.2, V15). **Ready for the implement trigger.** (The exact offset table was already empirically pinned in rev 0; §9 V0.)

---

## 1. Goal

Flip `--ffs` from CLI-**rejected** (v1.0) to **supported**, extending the genome-wide cytosine-report line from **7 columns** (`chr pos strand meth nonmeth context tri_nt`) to **10 columns** by appending three nucleotide-context fields:

```
chr  pos  strand  meth  nonmeth  context  tri_nt  tetra_nt  penta_nt  hexa_nt
```

This is a **report-line FORMAT extension** — it reuses the entire v1.0 genome walk, the per-position kernel (`emit_position`), the covered + uncovered passes, both single-file and `--split_by_chromosome` paths, and the context-summary machinery, **adding three appended columns, not rewriting any of it**. The three extra fields:

- `tetra_nt` — the 4-mer starting at the cytosine.
- `penta_nt` — the 5-mer starting at the cytosine.
- `hexa_nt` — the 6-mer following the `xxCxxx` rule (2 bases before C + C + 3 after).

Each is the empty string when its window runs off a chromosome edge (Perl prints a blank field). On the reverse strand all three are reverse-complemented. **Byte-identical to Perl `coverage2cytosine` v0.25.1** (STDERR exempt).

Worked Perl example (`--help`, Perl `:2293-2294`):
```
U00096.3  90  +  0  0  CG  CGT  CGTG   CGTGA   GCCGTG
U00096.3  91  -  1  0  CG  CGG  CGGC   CGGCA   CACGGC
```

## 2. Context

> ⚠️ **rev 1 — POST-PHASE-1 REBASE (read first).** This plan was drafted against the **pre-Phase-1** `report.rs`/`cli.rs`; Phase 1 (`--gc`/`--nome-seq`) has since landed in the same files and reshaped exactly the functions Phase 3 touches. **All cited line numbers below and the `emit_position` signature in §4 are STALE** — the implementer must re-grep, not trust the numbers. The concrete corrections (both dual reviewers, independently):
> - **`emit_position` already takes `nome: bool` AND `cov_out: &mut Vec<u8>`** (after `threshold` and after `out`, respectively). Thread the new `ffs: bool` **alongside `nome`** — do NOT use the §4 signature verbatim (it predates `nome`/`cov_out`).
> - **`chromosome_report_bytes` now returns `(Vec<u8>, Vec<u8>)`** (report + cov) and has a single `emit_position` call site — pass `config.ffs` there. `extract` has **exactly one** caller (`emit_position`); `run_t`/`run_nome` call `emit_position`, not `extract`.
> - **The unit-test harness is now `run_nome`** (with `run_t`/`run` as thin wrappers over `run_nome(..., nome=false)`). Add `ffs` (default `false`) to `run_nome` so the existing wrappers stay green, then add the ffs assertions.
> - Approx current lines (re-confirm): `perl_substr` ~`:99`, `revcomp` ~`:115`, `extract` ~`:145`, `emit_position` ~`:169`, `chromosome_report_bytes` ~`:264`, `run_single` ~`:316`, `run_split` ~`:406`, `flush_split_chromosome` ~`:471`; `cli.rs` `--ffs` rejection ~`:159-161`.
> - **Adding `pub ffs: bool` to `ResolvedConfig` breaks the in-test struct literal** in `report.rs::nome_cov_path_uses_raw_base` (it enumerates every field) — add `ffs: false` there too, or `cargo test` won't compile.

- **Where:** `rust/bismark-coverage2cytosine/src/`. Touches **two** files: `report.rs` (the kernel + `extract`) and `cli.rs` (un-reject `--ffs`, add `ffs` to `ResolvedConfig`). No new module.
- **The v1.0 substrate this extends** (`report.rs`, all already shipped + green):
  - `pub(crate) fn perl_substr(seq, offset: isize, want) -> &[u8]` (`report.rs:91`) — **already models Perl negative-offset wrap** (the crux for forward `hexa_nt` at chr-start, §3.2). Reuse verbatim.
  - `pub(crate) fn revcomp(seq) -> Vec<u8>` (`report.rs:107`) — `tr/ACTG/TGAC/`, N/other bytes pass through. Reuse verbatim for the three reverse-strand fields.
  - `fn extract(seq, i) -> (tri, upstream, strand)` (`report.rs:137`) — the v1.0 forward-C / reverse-G coordinate routine. **This is where the three new fields are computed** (§3.2 / §5 task 2).
  - `pub(crate) fn emit_position(...)` (`report.rs:161`) — the per-position kernel. **This is where the three new fields are appended to the report line** (§3.3 / §5 task 3), gated on a new `ffs` parameter.
  - `chromosome_report_bytes` (`report.rs:226`) + `run_single` (`:275`) + `run_split` (`:337`) + `flush_split_chromosome` (`:400`) — the callers that thread config flags down to `emit_position`. Add `config.ffs` to the call sites (§5 task 4).
- **CLI** (`cli.rs`): `pub ffs: bool` already parses (`cli.rs:98-99`); `validate()` currently **rejects** it (`cli.rs:158-160`). Phase 3 deletes that rejection and adds `ffs: bool` to `ResolvedConfig` (`cli.rs:103`) + the constructor (`cli.rs:213`). The `--help` doc-comment label `(v1.x, rejected)` (`cli.rs:97`) becomes the real help text.
- **Depends on:** the **merged v1.0** core (EPIC §3 precondition) — Phases A–E. No dependency on sibling Phases 1 (GpC/NOMe) or 2 (DRACH); Phase 3 is mutually independent (EPIC §4).
- **Perl ground truth:** the three structurally-identical extraction blocks —
  - **Covered-chromosome flush** (the per-`chr`-transition block): extraction `:262-341`, emission `:398/413/432/441`.
  - **Last-chromosome flush** (post-EOF block): extraction `:507-585`, emission `:641/656/675/683`.
  - **Uncovered-chromosome pass** (`process_unprocessed_chromosomes`): extraction `:1421-1493`, emission `:1524/1533/1545/1553`.
  - All three compute the same six fields with the same offsets; only the hash variable name differs (`$chromosomes{$last_chr}` vs `{$chr}`). The Rust collapses them into the single `extract` + `emit_position` kernel (v1.0 already does this for `tri_nt`/`upstream`), so the three Perl blocks map to **one** Rust change.

## 3. Behavior

### 3.1 When the columns appear

`--ffs` (Perl `$tetra`, `:2023`) adds the three columns to **every** emitted report line:
- **CpG-only (default)** AND **`--CX`** — Perl prints them in both the `if ($CpG_only)` and the `else` (--CX) branches (`:398/413/432/441` etc.). Confirmed on the fixture (§9 V0): `--ffs` alone produces a 10-col `.CpG_report.txt`; `--CX --ffs` produces a 10-col `.CX_report.txt`.
- **Covered AND uncovered chromosomes** — the uncovered (all-zero) pass also prints them (`:1524` etc.). So `--ffs` columns are present on `0 0` lines too.
- **`--zero_based`** — orthogonal; the only change is `pos -= 1`, the three context fields are byte-identical regardless of coordinate base (extracted from the genome, not the coordinate). Confirmed.
- **`--split_by_chromosome`** — orthogonal; per-chr files carry the same 10-col lines.

It does **not** change: the context-summary file (§3.5), the merged/discordant cov files' own format (§3.6), the column set 1–7 (they are unchanged; 8–10 are appended).

### 3.2 The six extracted fields — EXACT offsets (the byte-identity crux)

For the cytosine at 0-based index `i`, Perl uses `pos = i+1` (1-based). v1.0 already computes `tri_nt` + `upstream`; Phase 3 adds `tetra_nt`, `penta_nt`, `hexa_nt`. **All offsets below are pinned against live Perl v0.25.1** (§9 V0 — a 3-chromosome fixture exercising interior, `i=0`, `i=1`, chr-end, and every empty-window case diffed byte-identical).

**Forward strand (genome `C`, strand `+`)** — each field is extracted **only if its length guard passes**, else the empty string `""`. No reverse-complement.

| field | Perl substr | guard (Perl) | guard with `pos=i+1` | Rust slice |
|-------|-------------|--------------|----------------------|-----------|
| `tetra_nt` | `substr(seq, pos-1, 4)` | `len ≥ pos-1+4` | `len ≥ i+4` | `perl_substr(seq, i as isize, 4)` |
| `penta_nt` | `substr(seq, pos-1, 5)` | `len ≥ pos-1+5` | `len ≥ i+5` | `perl_substr(seq, i as isize, 5)` |
| `hexa_nt`  | `substr(seq, pos-3, 6)` | `len ≥ pos-3+6` | `len ≥ i+4` | `perl_substr(seq, i as isize - 2, 6)` |

⚠️ **Forward `hexa_nt` offset is `pos-3 = i-2`, which is NEGATIVE at `i=0` and `i=1`** — and at those positions the guard `len ≥ i+4` can still pass, so Perl's `substr(seq, NEGATIVE, 6)` **wraps from the string end**. This is the same Perl-negative-offset-wrap class as the v1.0 `upstream` P3 pitfall, and is **exactly what `perl_substr` already models**. Verified (§9 V0): at chr1 `i=1` (`pos2`), `substr(seq,-1,6)` → the single trailing char `"T"`; at chrC `i=0` (`pos1`), `substr(seq,-2,6)` → the trailing two chars `"CC"`. **Do NOT clamp the negative offset to 0** — that would diverge. (The guard, by contrast, is a plain numeric compare; when `len < i+4` the field is `""` regardless of offset.)

**Reverse strand (genome `G`, strand `-`)** — extract, then `revcomp`. Empty string if the guard fails.

| field | Perl substr (pre-revcomp) | guard (Perl) | guard with `pos=i+1` | Rust |
|-------|---------------------------|--------------|----------------------|------|
| `tetra_nt` | `substr(seq, pos-4, 4)` | `pos-4 ≥ 0` | `i ≥ 3` | `revcomp(perl_substr(seq, i as isize - 3, 4))` |
| `penta_nt` | `substr(seq, pos-5, 5)` | `pos-5 ≥ 0` | `i ≥ 4` | `revcomp(perl_substr(seq, i as isize - 4, 5))` |
| `hexa_nt`  | `substr(seq, pos-4, 6)` | `pos-4 ≥ 0` | `i ≥ 3` | `revcomp(perl_substr(seq, i as isize - 4 + 0 …))` → **offset `pos-4 = i-3`, want 6** = `revcomp(perl_substr(seq, i as isize - 3, 6))` |

⚠️ Note the reverse-strand asymmetry, faithfully reproduced from Perl:
- Reverse `hexa_nt` uses offset **`pos-4`** (NOT `pos-3`) and the **`pos-4 ≥ 0`** guard (it reuses the tetra guard, Perl `:323`/`:569`/`:1480`), with `want=6`. So reverse hexa = `revcomp(substr(seq, i-3, 6))`.
- Reverse `penta_nt` uses offset `pos-5 = i-4` and guard `pos-5 ≥ 0` (`i ≥ 4`), `want=5`.
- Because the guards are `≥ 0` integer tests on a non-negative-only path (when the guard passes, `i-3`/`i-4 ≥ 0` so the offset is never negative), there is **no negative-wrap on the reverse strand** — the guard prevents it. (Contrast forward `hexa_nt`, where the guard `len ≥ i+4` does NOT prevent the offset `i-2` from being negative.)

⚠️ **rev 1 (A-V-gap-1) — N-containing windows are NOT filtered; the `--help` is wrong.** Perl's `--help` (`:2291`) claims "sequences containing Ns are ignored," but **v0.25.1 does NOT filter N-windows** — it emits the tetra/penta/hexa fields verbatim even when they span an `N`, and `tr/ACTG/TGAC/` (the `revcomp` helper) leaves `N` unchanged on the reverse strand. Live-Perl-verified by **both** reviewers (e.g. a C whose window spans an `N` emits `CGN`/`CGNT`…). **Do NOT implement the N-filtering the help describes** — that would diverge. The correct behaviour is exactly the §3.2 offset table with the existing `perl_substr`/`revcomp` (which already pass `N` through). Pinned by the new N-window golden (§9 V15).

**Worked examples** (all from §9 V0, genome `chr1 = GCCGTGAAACACGGCTTT`, `chrC = CGTAAACCC`):
- `chr1` `i=2` `pos3` `+`: tetra=`CGTG` (`seq[2..6]`), penta=`CGTGA` (`seq[2..7]`), hexa=`GCCGTG` (`substr(pos-3=0,6)=seq[0..6]`). Matches the help example.
- `chr1` `i=1` `pos2` `+`: tetra=`CCGT`, penta=`CCGTG`, hexa=`T` (negative-wrap: `substr(seq,-1,6)`).
- `chrC` `i=0` `pos1` `+`: tetra=`CGTA`, penta=`CGTAA`, hexa=`CC` (negative-wrap: `substr(seq,-2,6)`).
- `chr1` `i=3` `pos4` `-`: tetra=`CGGC` (`revcomp(seq[0..4]=GCCG)`), **penta=`""`** (guard `i≥4` fails at i=3), hexa=`CACGGC` (`revcomp(seq[0..6]=GCCGTG)`). Matches the help example's bottom-strand pattern.
- `chr1` `i=14` `pos15` `+`: tetra=`CTTT`, **penta=`""`** (guard `len ≥ i+5 = 19` fails; len=18), hexa=`GGCTTT` (`substr(pos-3=12,6)=seq[12..18]`).
- `chrC` `i=6` `pos7` `+`: **tetra=`""`** (`len ≥ 10` fails; len=9), **penta=`""`**, **hexa=`""`** (`len ≥ i+4 = 10` fails).

### 3.3 Emission (append 3 columns)

`emit_position` (`report.rs:161`) gains a `ffs: bool` parameter. After writing the existing 7th field (`tri_nt`, `report.rs:218`), **when `ffs`**, append a tab + `tetra_nt`, a tab + `penta_nt`, a tab + `hexa_nt` (each possibly empty → an empty inter-tab field), then the `\n`. Order: `…\t{tri}\t{tetra}\t{penta}\t{hexa}\n`. The three field byte-strings come from `extract` (§5 task 2). Empty fields are emitted as nothing-between-tabs (e.g. `…\tCGGC\t\tCACGGC\n` for the empty-penta case). The `tri_nt` field and columns 1–6 are byte-unchanged.

### 3.4 Guards / ordering — UNCHANGED from v1.0

The five v1.0 per-position guards (tri.len()<3 skip; last-base skip; coverage lookup; threshold; context classify; CpG-only filter) are **untouched**. The three ffs fields are computed in `extract` **regardless** of those guards (Perl computes `$tetra_nt`/`$penta_nt`/`$hexa_nt` at the top of the loop body, before the guards), but they are only ever *emitted* on a line that survives all guards — so computing them eagerly in `extract` is correct and matches Perl (Perl also computes-then-maybe-skips). No guard depends on the ffs fields. (Perl's `length $penta_nt < 5` commented-out debug at `:351-353` confirms the fields are advisory, never gating.)

### 3.5 Context summary — UNCHANGED

`context_reporting` (the 64-row summary) is fed only `tri_nt` + `upstream` (Perl `:381/624`), **not** the ffs fields. `--ffs` does not alter `*.cytosine_context_summary.txt`. (Re-confirm in V5: a `--ffs` run's summary == a no-ffs run's summary on the same fixture.)

### 3.6 `--merge_CpGs` interaction — UNCHANGED (the merge tolerates extra columns)

`--ffs --merge_CpGs` is **allowed** in Perl (no mutex; Perl `process_commandline:2138-2194` does not couple them — confirmed by grep). The merge re-read (`combine_CpGs_to_single_CG_entity:1802`) does `($chr1,$pos1,$strand1,$m1,$u1,$context1) = (split /\t/, $line1)` — a **6-element list assignment that silently discards the trailing tetra/penta/hexa fields**. So a `--ffs` CpG report merges into a **byte-identical** `*.merged_CpG_evidence.cov` as a non-ffs report (the merged-cov format has no ffs columns).

The Rust `merge::parse_report_row` (`merge.rs:52-77`) already mirrors this: it requires `f.len() ≥ 6` and indexes only `f[0..6]`, **tolerating extra trailing columns**. So **no change to `merge.rs` is needed** — the Phase D merge re-reads a 10-col ffs report and produces the same merged cov. **V6 pins this** (a `--ffs --merge_CpGs` run's merged cov == a no-ffs `--merge_CpGs` run's merged cov, both == Perl). This must NOT regress the Phase A mutex set (Phase A does not reject `--ffs --merge_CpGs`, and must not start).

### 3.7 CLI — un-reject `--ffs`

In `cli.rs::validate()`: **delete only the `--ffs` arm** of the v1.x rejection (the `if self.ffs { return Err(UnsupportedFlag … "--ffs") }` block, ~`:159-161`). Add `ffs: bool` to `ResolvedConfig` (append it — the §4 `:103` insertion point is now occupied by Phase 1's `gc_context`/`nome` fields) and set it in the constructor block from `self.ffs`. Update the `--ffs` doc-comment (`(v1.x, rejected) tetra/penta/hexamer context columns.`) to the real help: e.g. `Append tetra-, penta- and hexamer nucleotide-context columns to each report line (hexamers follow the xxCxxx rule; edge windows are left blank).` The `--drach` rejection **stays** (Phase 2 owns it). **No new mutex** — `--ffs` composes with every supported flag (CpG/`--CX`/`--zero_based`/`--split_by_chromosome`/`--coverage_threshold`/`--gzip`/`--merge_CpGs`/`--discordance_filter`).
- ⚠️ **rev 1 (A-A4 / B-3): the `--ffs` rejection lives in the SHARED `rejects_v1x_flags` test loop** `for (flag, frag) in [("--drach","drach"), ("--ffs","ffs")]`. **Narrow** the loop to `[("--drach","drach")]` — do NOT delete the whole test (that would drop `--drach`'s rejection coverage). Add a separate positive assertion that `--ffs` resolves (V7).
- ⚠️ **rev 1 (B-4): adding `pub ffs: bool` to `ResolvedConfig` will fail to compile** the in-test `ResolvedConfig { … }` literal in `report.rs::nome_cov_path_uses_raw_base` (it enumerates every field) — add `ffs: false` there too.

## 4. Signatures

```rust
// report.rs — extract() gains the three ffs fields. Return a small struct (or
// extend the tuple) so emit_position can append them. Proposed:
struct Extracted {
    tri: Vec<u8>,
    upstream: Vec<u8>,
    strand: u8,
    tetra: Vec<u8>,   // empty when the window runs off the edge
    penta: Vec<u8>,
    hexa: Vec<u8>,
}
fn extract(seq: &[u8], i: usize, ffs: bool) -> Extracted;
//  - when ffs == false, tetra/penta/hexa stay empty (not computed) — micro-opt,
//    and keeps non-ffs runs byte-identical (they never read these fields anyway).

// report.rs — emit_position() gains `ffs: bool`; appends 3 columns when set.
// ⚠️ STALE (pre-Phase-1) — see the §2 rebase note. The LIVE signature already
// has `nome: bool` (after `threshold`) and `cov_out: &mut Vec<u8>` (after `out`);
// thread `ffs` ALONGSIDE `nome`, do NOT copy the param list below verbatim.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_position(
    name: &[u8], seq: &[u8], i: usize,
    buffer: &HashMap<u32,(u32,u32)>,
    cpg_only: bool, zero_based: bool, threshold: u32,
    nome: bool,                      // EXISTS (Phase 1)
    ffs: bool,                       // NEW — add next to nome
    accumulate_summary: bool,
    summary: &mut ContextSummary,
    out: &mut Vec<u8>,
    cov_out: &mut Vec<u8>,           // EXISTS (Phase 1)
);

// cli.rs — ResolvedConfig gains:
pub ffs: bool,
```

(If `extract`'s tuple is preferred over a struct for minimal churn, return `(tri, upstream, strand, tetra, penta, hexa)` — but a named struct reads better given six fields. Implementer's choice; the existing 3-tuple has only two interior call sites: `emit_position` + its unit-test harness `run_t`.)

## 5. Implementation outline (TDD-friendly)

1. **Pin the offset table in unit tests first** (`report.rs` tests): add `extract`-level assertions for forward tetra/penta/hexa (interior + `i=0`/`i=1` negative-wrap hexa + chr-end empty) and reverse tetra/penta/hexa (interior + `i=3` empty-penta + chr-start short). Use the §3.2 worked-example bytes as the expected values (they are live-Perl-verified). These tests fail until step 2.
2. **Extend `extract`** (`report.rs:137`): add a `ffs: bool` param; when `ffs`, compute the six fields per the §3.2 table — forward via `perl_substr(seq, off, want)` (hexa uses signed `i-2` → negative-wrap; **guard the empties with the `len ≥ i+N` numeric tests, NOT by relying on `perl_substr` returning empty**, because forward hexa's offset can be negative *while* the guard passes); reverse via `revcomp(perl_substr(seq, off, want))` guarded by `i ≥ 3` / `i ≥ 4`. Return the `Extracted` struct.
3. **Extend `emit_position`** (`report.rs:161`): add `ffs: bool`; after the `tri` field, when `ffs`, append `\t{tetra}\t{penta}\t{hexa}` before the `\n` (§3.3). Columns 1–7 byte-unchanged.
4. **Thread `config.ffs`** through the call chain: `chromosome_report_bytes` (`report.rs:226`) passes `config.ffs` to `emit_position`; both call sites are `run_single` (`:299/311`) and `flush_split_chromosome` (`:408`) via `chromosome_report_bytes` — they pass `config`, so add the arg inside `chromosome_report_bytes` only. Update the `run_t` test harness signature.
5. **CLI** (`cli.rs`): delete the `--ffs` rejection (`:158-160`); add `ffs` to `ResolvedConfig` + constructor; update the `:97` help doc-comment; remove `("--ffs","ffs")` from the rejection test loop (`:303`) and add a positive resolve test (V7).
6. **Goldens + integration tests**: a `tests/golden_phase3.rs` (matches the `golden_phase_b/c/d.rs` + `golden_phase1.rs` naming) + a `tests/data/phase3_ffs/` fixture dir. ⚠️ **rev 1 (B-Opt1):** there is **no shared `generate_goldens.sh`** — each phase has its OWN under `tests/data/<phase>/`; create `tests/data/phase3_ffs/generate_goldens.sh` modeled on `tests/data/phase1/generate_goldens.sh` (it sets `C2C="$(cd "$HERE/../../../../.." && pwd)/coverage2cytosine"`, `set -eo pipefail`, and writes each FASTA into a per-fixture **directory** via `mkfa`, since Perl reads the genome from a folder). Fixture: a tiny multi-FASTA with chr-start/chr-end/short-scaffold C and G **plus an `N`-containing chromosome** (V15). Diff Rust vs Perl golden for: `--ffs` (CpG), `--CX --ffs`, `--ffs --zero_based`, `--ffs --split_by_chromosome`, `--ffs --gzip` (decompressed), `--ffs --merge_CpGs` (merged cov == no-ffs golden), an uncovered-chromosome `--ffs` line, and the N-window case.
7. **Regression:** full suite green (the v1.0 + Phase D tests must be unaffected — non-ffs runs emit the identical 7-col lines).

## 6. Efficiency

- The genome walk, coverage map, and per-position cost are unchanged. `--ffs` adds three `perl_substr` slices (≤6 bytes each) + up to three `revcomp` allocations (≤6 bytes) per emitted reverse-strand position — O(1) per cytosine, negligible vs the existing `tri`/`upstream` work. When `ffs == false`, the fields are not computed (`extract` skips them), so the v1.0 hot path is untouched (zero regression for the default mode).
- Output grows by three short columns per line (~15–25 bytes/line). For a full-hg38 `CX_report` (~1B lines) that is meaningful on disk — the Phase 4 byte-identity gate must gzip + stream-compare the ffs cells (already its discipline; note the larger ffs output in Phase 4's disk-headroom pre-flight).
- No new heap structures; the three `Vec<u8>` fields live only for the duration of one `emit_position` call (could be `SmallVec`/stack arrays later, but byte-identity-first — leave as `Vec` to mirror the existing `tri`/`upstream`).

## 7. Integration

- **Reads:** unchanged (genome FASTA + cov file). **Writes:** the same report file(s), now with 3 appended columns when `--ffs`; the context summary + merged/discordant cov are **format-unchanged** (§3.5/§3.6).
- **Order:** unchanged — Phase 3 lives entirely inside the Phase B/C walk; Phase D merge runs after and tolerates the extra columns.
- **Downstream:** the extractor inline switch is unaffected (it drives `--ffs` via argv if at all). The Phase 4 gate gains `--ffs` matrix cells (EPIC §4 / §7).
- **Internal contract note:** the report-line bytes are an internal contract for Phase D's re-read. Phase 3 **appends** columns (does not reorder 1–7), and Phase D reads only fields 0–5 — so the contract holds. A future change that reorders columns 1–7 would break Phase D; appending is safe.

## 8. Assumptions

**From epic (shared, EPIC §6):**
1. Byte-identity to Perl v0.25.1 for every output stream; STDERR exempt.
2. Reuse v1.0 infrastructure (genome reader, cov parse, `ReportWriter`, `ResolvedConfig`/`validate()`, error enum, the `--gzip`/`--zero_based`/`--split_by_chromosome`/`-o`/`--dir`/`--parent_dir` machinery); flip the flag rejected→supported (update `validate()` + `--help`).
3. Built on the merged v1.0 (EPIC §3 precondition).
4. Testing model: local Perl-v0.25.1 goldens on tiny fixtures + the oxy real-data gate (Phase 4); worktree isolation.
5. Niche-flag interactions mirror Perl `process_commandline`.

**Phase-3 specific:**
6. `--ffs` is a pure **append-3-columns** report-line extension — columns 1–7 are byte-identical to a non-ffs run; only an emitted line gains `\t{tetra}\t{penta}\t{hexa}` (§3.3). **Verified V0.**
7. The six extraction offsets are exactly the §3.2 table (live-Perl-pinned, V0). Forward `hexa_nt` uses a **signed** offset `i-2` with Perl negative-wrap at `i=0,1`; the empty-window guard for forward hexa is the numeric `len ≥ i+4` test (NOT "perl_substr returned empty"). Reverse fields never negative-wrap (the `i≥3`/`i≥4` guards prevent it).
8. `--ffs` applies in BOTH CpG-only and `--CX`, and in BOTH the covered and uncovered passes (§3.1; the uncovered pass emits 10-col `0 0` lines). **Verified V0.**
9. `--ffs` is **orthogonal** to `--zero_based`, `--split_by_chromosome`, `--gzip`, `--coverage_threshold`, `--merge_CpGs` — no mutex (Perl couples none of them). The merge re-read discards the extra columns (§3.6). **Confirmed by grep + a live `--ffs --merge_CpGs` run.**
10. `revcomp` (the existing `tr/ACTG/TGAC/`) and `perl_substr` (the existing negative-wrap model) are correct for the ffs fields too — they are the same primitives v1.0 uses for `tri`/`upstream`; reuse verbatim.

## 9. Validation

Goldens generated from the **repo Perl v0.25.1** (`generate_goldens.sh` ffs block). V0 already executed during planning (the offset table is pinned).

| # | Verify | How | Expected |
|---|--------|-----|----------|
| V0 | **offset table == live Perl** (done in planning) | a 3-chr fixture (`GCCGTGAAACACGGCTTT`, `AACGCCAAGGCC`, `CGTAAACCC`) through Perl `--CX --ffs`; a from-scratch Perl reimpl of the §3.2 table diffed against it | **IDENTICAL** (already confirmed — every interior, `i=0`/`i=1` negative-wrap, chr-end, and empty-window case) |
| V1 | forward tetra/penta/hexa, interior | unit on `extract`: `chr1 i=2` | `CGTG`/`CGTGA`/`GCCGTG` |
| V2 | **forward hexa negative-wrap** at `i=1`,`i=0` | unit: `chr1 i=1` + `chrC i=0` | hexa `T` / `CC` (NOT clamped-empty) |
| V3 | forward empty windows at chr-end | unit: `chr1 i=14` (penta empty), `chrC i=6` (all three empty) | `CTTT`/`""`/`GGCTTT`; `""`/`""`/`""` |
| V4 | reverse tetra/penta/hexa + empty penta | unit: `chr1 i=3` (`-`) | `CGGC`/`""`/`CACGGC` (revcomp'd; penta guard `i≥4` fails) |
| V5 | **context summary unchanged by --ffs** | run `--ffs` and no-ffs on the same fixture; diff `*.cytosine_context_summary.txt` | byte-identical (ffs doesn't touch the summary) |
| V6 | **--ffs --merge_CpGs merged cov unchanged** | run `--ffs --merge_CpGs` and `--merge_CpGs` (no ffs); diff `*.merged_CpG_evidence.cov`; also diff vs Perl golden | byte-identical (merge discards ffs columns) |
| V7 | CLI: `--ffs` resolves (not rejected); composes with `--CX`/`--gzip`/`--merge_CpGs` | `validate()` unit (replaces the removed rejection test) | `Ok(ResolvedConfig{ ffs: true, .. })`; no `UnsupportedFlag` |
| V8 | **CpG `--ffs` golden** | binary `--ffs` on the fixture; diff `.CpG_report.txt` vs Perl golden | 10-col, byte-identical |
| V9 | **`--CX --ffs` golden** | binary `--CX --ffs`; diff `.CX_report.txt` vs Perl golden | 10-col across CG/CHG/CHH, byte-identical |
| V10 | `--ffs --zero_based` golden | binary; diff vs Perl golden | `pos-1`; the 3 context fields byte-identical to V8 |
| V11 | `--ffs --split_by_chromosome` golden | binary; diff each per-chr file vs Perl golden | 10-col per-chr; `.chr<NAME>` infix per v1.0 |
| V12 | `--ffs --gzip` golden | binary; decompress + diff vs plain V8 golden | byte-identical after decompression |
| V13 | **uncovered-chromosome `--ffs` line** | fixture with a genome chr absent from the cov file, `threshold=0` | the uncovered chr emits 10-col `0 0` lines (Perl `:1524`), byte-identical |
| V14 | regression: v1.0 + Phase D suites | full `cargo test` | green (non-ffs runs emit identical 7-col lines; merge unaffected) |
| V15 | **N-window NOT filtered** (rev 1 A-V-gap-1) | a `chrN` fixture with a C/G whose tetra/penta/hexa windows span an `N`, both strands, `--CX --ffs`; diff vs Perl golden | N-windows **emitted verbatim** (`CGN`/`CGNT`…; revcomp passes `N`→`N`) — proves the false `--help` "Ns ignored" claim is NOT implemented |
| V16 | (optional) all-three-empty trailing-tab + reverse `i=2` all-empty | unit on `extract` (reverse `G` at `i=2`: tetra/penta/hexa all `""`) + a byte-level golden of a chr-end line ending `…\t{tri}\t\t\t\n` | empty fields render as nothing-between-tabs; `parse_report_row` still tolerates it in the `--merge_CpGs` re-read |

## 10. Questions or ambiguities

| Priority | Question | Resolution |
|----------|----------|------------|
| Resolved | Exact tetra/penta/hexa substr offsets on both strands, incl. edges | **Pinned (§3.2)** against live Perl v0.25.1 (V0, byte-identical diff). Forward: tetra `i`/4, penta `i`/5, hexa `i-2`/6 (signed, negative-wrap); reverse: tetra `i-3`/4, penta `i-4`/5, hexa `i-3`/6 (all revcomp'd, guarded `i≥3`/`i≥4`). |
| Resolved | Edge / empty-window handling | Forward field empty iff `len < i+4`/`i+5`/`i+4`; reverse empty iff `i<3`/`i<4`/`i<3`. Forward hexa's offset may be negative *while* the guard passes → Perl negative-wrap (NOT empty) — `perl_substr` already models it. Pinned V2/V3. |
| Resolved | Does `--ffs` apply to CpG-only AND `--CX`, covered AND uncovered? | **Both, both** (§3.1, V0/V13). |
| Resolved | `--ffs` + `--merge_CpGs` mutex? | **No mutex** (Perl allows it; merge discards the extra columns via 6-element list assignment). Rust `parse_report_row` already tolerates extra cols → **no `merge.rs` change** (§3.6, V6). |
| Resolved | Does `--ffs` affect the context summary? | No — summary is fed `tri`+`upstream` only (§3.5, V5). |
| **Resolved (rev 1)** | Are N-containing windows filtered (the `--help` says "Ns ignored")? | **No** — v0.25.1 emits N-windows verbatim; the `--help` claim is FALSE/stale. Do NOT implement N-filtering. Both reviewers confirmed on live Perl; `revcomp` already passes `N` through. Pinned by V15 (§3.2 note). |
| Open (non-blocking) | `Extracted` struct vs extended tuple for `extract`'s return | Implementer's choice (§4); a named 6-field struct reads better. Does not affect output. |

No **Critical** ambiguities remain — the offset table is empirically pinned and the flag interactions are confirmed against live Perl.

## 11. Self-Review

- **Logic:** the three Perl extraction blocks (`:262-341` / `:507-585` / `:1421-1493`) compute identical fields; the Rust collapses them into the single v1.0 `extract` + `emit_position` kernel (which v1.0 already does for `tri`/`upstream`), so the change is localized and the three blocks cannot drift apart (the "dual-driver back-port" trap is structurally avoided). Emission appends, never reorders.
- **Edge cases:** forward hexa negative-wrap at `i=0,1` (V2 — the highest-risk case, since the guard does NOT prevent the negative offset); chr-end empty windows (V3); reverse empty penta at `i=3` (V4); uncovered-chr 10-col lines (V13); empty fields rendered as nothing-between-tabs (V8/V9). The interaction matrix (zero_based/split/gzip/merge) is covered V10–V13.
- **Efficiency:** O(1) extra per cytosine; non-ffs hot path untouched (fields computed only when `ffs`).
- **Integration:** report-line columns 1–7 byte-unchanged → Phase D re-read + context summary unaffected (V5/V6/V14); the Phase A `--ffs`-rejection test must be removed (V7) or the suite fails.
- **Risks:** (a) forward hexa negative-wrap — the single subtle offset; mitigated by V0 (already passed) + V2. (b) Accidentally clamping the forward-hexa negative offset to 0 (a plausible implementer error) → diverges at chr-start; explicitly called out (§3.2 / §5 task 2) + V2. (c) Forgetting to remove the Phase-A `--ffs` rejection test → caught immediately by the suite. (d) Reverse hexa offset confusion (`pos-4` not `pos-3`) — pinned in the §3.2 table + V4.

## Revision history
- **rev 0** (2026-05-31): initial Phase 3 plan from EPIC + v1.0 SPEC §7 + the three Perl extraction blocks. The six tetra/penta/hexa substr offsets pinned byte-identical against live Perl v0.25.1 on a 3-chromosome edge-case fixture (V0); `--ffs` scope (CpG+CX, covered+uncovered), the `--merge_CpGs` no-op interaction, and the context-summary invariance all confirmed against live Perl. Reuses the v1.0 `extract`/`emit_position`/`perl_substr`/`revcomp` substrate (append-only). Awaiting manual review.
- **rev 1** (2026-05-31): **dual plan-review folded** (`PLAN_REVIEW_A.md` + `PLAN_REVIEW_B.md`, both APPROVE-WITH-CHANGES, 0 Critical / 4 Important each; both independently re-derived the §3.2 offset table from the Perl source and diffed it byte-for-byte against live Perl — including the forward-hexa negative-wrap and the reverse-hexa `i-3`, both confirmed correct). Folded: **the POST-PHASE-1 REBASE** (a prominent §2 note — every cited line number is stale, and `emit_position` already takes `nome: bool`+`cov_out`, `chromosome_report_bytes` returns `(report, cov)`, the harness is `run_nome`; thread `ffs` alongside `nome`; the §4 signature flagged stale); **A-A4/B-3** — narrow the SHARED `rejects_v1x_flags` loop to `[("--drach","drach")]` (don't delete it; §3.7); **B-4** — add `ffs: false` to the `nome_cov_path_uses_raw_base` in-test `ResolvedConfig` literal (§3.7); **A-V-gap-1** — the `--help` "sequences containing Ns are ignored" claim is **FALSE** in v0.25.1; N-windows emit verbatim, do NOT filter (§3.2 note, §10, new golden V15); **B-Opt1** — per-phase `generate_goldens.sh` (no shared script) + `golden_phase3.rs` naming (§5 task 6); **A-V-gap-2/B-Opt2** — added V16 (all-three-empty trailing-tab + reverse `i=2` all-empty). **The byte-identity crux (the offset table) had NO defect** — all changes are rebase hygiene + the N-window note. Status → **ready for the implement trigger.**
