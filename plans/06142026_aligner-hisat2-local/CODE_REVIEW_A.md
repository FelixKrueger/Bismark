# Code Review A — `--hisat2 --local` for the Rust bismark aligner

**Reviewer:** A (fresh context)
**Branch:** `rust/aligner-hisat2-local` (worktree `/Users/fkrueger/Github/Bismark-hisat2local`), uncommitted vs `origin/rust/iron-chancellor`
**Scope:** `rust/` diff adding `--hisat2 --local` (byte-identical to Perl v0.25.1 `--hisat2 --local`).

## Verdict: **APPROVE-WITH-NITS**

The change is correct and byte-faithful to Perl v0.25.1 on every probed axis: the `score_min_params` aligner narrowing, the option-string assembly (incl. the `--no-softclip` drop), the minimap2 reject lift, and the MAPQ local-ladder reuse. All new test arithmetic verifies. No Critical or High findings. Two **stale documentation comments** (Low) that now contradict the code, plus a couple of cosmetic notes.

---

## What I verified against the Perl oracle

I read the live Perl source in this worktree to anchor every byte-identity claim:

- **`bismark:7892-7955`** (`$score_min` / default → `@aligner_options`):
  - local + `$bowtie2` → `--local` + `--score-min G,$i,$s` (default `G,20,8`). Lines 7904-05 / 7942-44.
  - local + HISAT2 (`else`) → `--score-min L,$i,$s` (default `L,0,-0.2`), **NO `--local`**. Lines 7912-13 / 7947-48.
  - end-to-end (any) → `--score-min L,$i,$s` default `L,0,-0.2`. Lines 7921-22 / 7952-53.
- **`bismark:8309-8315`**: HISAT2 + `$local` → `--omit-sec-seq` only; else → `--no-softclip --omit-sec-seq`.
- **`bismark:7376`**: `'local' => \$local` — `$local` is a plain CLI flag set regardless of aligner, so it feeds `calc_mapq`'s `$local` branch for BOTH Bowtie 2-local and HISAT2-local.
- **`bismark:3932-3936` + `4078-4179`**: the local `ln()` scMin + the local MAPQ ladder.

### 1. The Critical (`score_min_params`, `options.rs:352-357`) — CORRECT
`if cli.local && aligner == Aligner::Bowtie2 { ("G,", (20.0, 8.0)) } else { ("L,", (0.0, -0.2)) }` is exactly right:
- Bowtie 2-local → G-form `(20,8)` ✓ (Perl 7942)
- HISAT2-local → L-form `(0,-0.2)` ✓ (Perl 7947)
- end-to-end any → L-form `(0,-0.2)` ✓ (Perl 7952)

**Single production caller** is `config.rs:364`, which passes `aligner`. No other callers (only tests). The validation path is also faithful: HISAT2-local with a G-form `--score_min` is **rejected** (`strip_prefix("L,")` fails → error), mirroring Perl 7908-09; verified by the new `score_min_params_aligner_and_mode_defaults` test (`is_err()` on `G,20,8` + Hisat2).

### 2. Option assembly (`options.rs`) — CORRECT, regressions intact
- **HISAT2-local SE** emits exactly `-q --score-min L,0,-0.2 --ignore-quals --omit-sec-seq` (no `--local`, no `--no-softclip`). The `cli.local && aligner == Bowtie2` narrowing at line 82 routes HISAT2-local into the L-form `else` (no `--local` push); the HISAT2 tail at `apply_aligner_specific_options:324-328` drops `--no-softclip` for `cli.local`. Pinned by `hisat2_local_option_string`.
- **PE analog** `… --no-mixed --no-discordant --maxins 500 --omit-sec-seq` — note `--dovetail` is correctly absent (gated `aligner == Bowtie2`, line 175), matching Perl's HISAT2 exclusion. Pinned in the same test.
- **🔴 Regression — both byte-frozen:**
  - Bowtie 2-local STILL emits `--local --score-min G,20,8` (line 82 branch unchanged; assertion in `hisat2_local_option_string` + `methylseq_align_local_now_accepted`). ✓
  - HISAT2 end-to-end STILL emits `--no-softclip --omit-sec-seq` (the `else` at line 326-327; `cli.local` is false). ✓
- **`apply_aligner_specific_options` early-return** (line 284): for `aligner != Hisat2` it returns `base` untouched, so the new `if cli.local` softclip branch only ever runs for HISAT2 — Bowtie 2-local never reaches it. Correct.

### 3. Reject lift (`config.rs:298-313`) — CORRECT
The gate flipped from `aligner != Bowtie2` to `aligner == Minimap2`. So Bowtie 2 + HISAT2 both pass; minimap2 rejects with the "by design" rationale (Q3). The combined-index reject (307-313) is unchanged and still fires for all aligners. I found **no path** where HISAT2-local slips past a guard it shouldn't, nor where minimap2-local could slip through — the `debug_assert_eq!(aligner, Bowtie2)` formerly in `options.rs:7` is correctly removed (grep confirms none remain), so the option-builder no longer assumes `local ⟹ Bowtie 2`.

### 4. MAPQ (`mapq.rs`) — genuinely no change needed; new test arithmetic VERIFIED
`calc_mapq` / `calc_mapq_local` are sign-agnostic: `diff = sc_min.abs()`, `best_over = as_best - sc_min`, `best_diff = |abs(best) - abs(second)|`. The negative-slope `(0,-0.2)` HISAT2 case (scMin negative) flows through identically to the `(20,8)` case. The local ladder (lines 143-220) is a verbatim port of Perl 4082-4178 — I diffed it line-by-line against the oracle: identical thresholds and return values.

I reimplemented the ladder in Python and confirmed **all six** hand-computed assertions in `local_hisat2_default_params_mapq`:
| call | got | expected |
|---|---|---|
| `(50,None,0,None)` | 44 | 44 ✓ |
| `(50,None,-1,None)` | 22 | 22 ✓ |
| `(150,None,0,None)` | 44 | 44 ✓ |
| `(50,None,0,Some(-1))` | 40 | 40 ✓ |
| `(50,None,-1,Some(-1))` | 0 | 0 ✓ |
| `(150,Some(150),0,Some(-1))` | **34** | 34 ✓ |

The PE `150+150 → 34` ln-ULP-sensitive leaf checks out: `diff = -0.4·ln(150) = 2.0042541…`, `diff·0.5 = 1.0021270… > 1`, so `best_diff = 1` is **not** `≥ diff·0.5` and falls into the 0.4 bucket; `best_over == diff` → 34. The test comment's claim that a sub-1.0 wobble would flip it to 35 is accurate. The bit-safety of `ln()` rests on the Phase-0 spike (`plans/06132026_aligner-local-mode/spikes/`, cited in the module doc) — I take that as established, not re-verified here.

### 5. Naming / style / comments
- `cli.rs:169-171` doc is accurate (describes the 3 aligner behaviors).
- `config.rs:177-181` + `292-297` doc comments are accurate.
- `options.rs:76-81` (the `--local` branch) and `343-351` (`score_min_params`) docs are accurate.
- `mapq.rs:1-3` module doc correctly updated to "both branches … HISAT2 since the HISAT2-`--local` work."
- README block updated correctly (Bowtie 2 **and** HISAT2 supported; minimap2 "by design").

---

## Findings

### Low (documentation only — code is correct, tests pass)

**L1 — Stale docstring contradicts the code it documents (`options.rs:271-282`).**
The doc comment on `apply_aligner_specific_options` still says the HISAT2 tail "is `[…] --no-softclip --omit-sec-seq`" (no longer always true — local drops `--no-softclip`) and, worse, the `NB:` paragraph (279-282) explicitly states:
> `--local` is rejected upstream for every aligner … so Perl's experimental HISAT2+`--local` path (`--omit-sec-seq` only, 8310-8312) is intentionally **not reproduced** …

The function body immediately below (324-328) now **does** reproduce exactly that path. This is a direct comment↔code contradiction and should be rewritten to describe the new local/non-local split. (Recommend — the fix is a doc rewording; not editing per shared-worktree instruction.)

**L2 — Stale comment in conformance test (`tests/methylseq_conformance.rs:185-186`).**
The `methylseq_align_local_now_accepted` docstring claims "HISAT2/minimap2-local + `--local`+combined-index are still rejected." HISAT2-local is now supported. The test body is unaffected (it only asserts Bowtie 2), so the test still passes, but the comment is now false and should drop HISAT2 from the "still rejected" list. (The PR updated `cli.rs`/`config.rs`/`options.rs:76-81`/`mapq.rs`/README docs but missed these two — L1 and L2 are the only doc drift I found.)

### Nits (cosmetic, optional)
- `mapq.rs:11-14` (the `calc_mapq` param doc) still says "local default `20.0`/`8.0`" — true only for Bowtie 2-local; HISAT2-local default is `0.0`/`-0.2`. Minor, since these are passed in by the caller, but could note "(Bowtie 2-local; HISAT2-local = 0.0/-0.2)".
- The new integration test `hisat2_local_softclip_roundtrip_and_options` is a good end-to-end probe (asserts report echoes the delta + the `2S4M` soft-clip round-trips into the BAM via the `S`-as-`I` path). No issue.

---

## Regression confidence
- Bowtie 2-local option string + MAPQ: byte-frozen (`hisat2_local_option_string` regression assertion, `accepts_local_for_bowtie2_emits_local_and_g_score_min`, `local_calc_mapq_uses_ln_scmin_and_local_ladder`).
- HISAT2 end-to-end option string: byte-frozen (`else` branch untouched; `score_min_params_aligner_and_mode_defaults` covers the `(0,-0.2)` default).
- End-to-end MAPQ ladder: untouched (the `local == false` arithmetic is unchanged; `local` adds nothing when false).
- combined-index + minimap2 rejects: intact.

Local gates (394 lib + 97 integ + 3 conformance, clippy `-D`, fmt) reported PASS — consistent with my static read; I did not re-run cargo.
