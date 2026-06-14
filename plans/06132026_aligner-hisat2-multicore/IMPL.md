# IMPL (TDD) — HISAT2 multi-core via Approach B-faithful (`--multicore N` → single-instance `-p N`)

**Source plan:** `PLAN.md` rev 2 (Approach **B-faithful** locked; Phase-0 spike confirmed `-p N` is
deterministic-per-N but ≠ single-core → gate vs Perl `--hisat2 -p N`, NOT single-core).
**Crate:** `rust/bismark-aligner` · **Worktree:** `~/Github/Bismark-hisat2mc` · **Branch:** `rust/aligner-hisat2-multicore`
**Goal (one line):** route `--hisat2 --multicore N` to ONE HISAT2 instance with `-p N --reorder`
(reuse the existing plumbing), opt-in + never-silent; byte-identical to Perl `--hisat2 -p N` per N.
**Mode:** TDD (Rust `cargo test`). **Status:** plan only — awaiting dual plan-review on this IMPL → implement trigger.

---

## What the spike settled (the contract this implements)

- `-p N --reorder` (one instance) is **deterministic run-to-run** but **NOT == single-core** → gate is
  **B-faithful** (== Perl `--hisat2 -p N`, per matching N), per-artifact (BAM + report both vs Perl `-p N`).
- The mapping is a **semantic remap**: for HISAT2, `--multicore N` is interpreted as `-p N` intra-instance
  threading (NOT Perl's fork model). Documented loudly (stderr + README). Bowtie 2 `--multicore` (fork,
  Phase 9b) and single-core HISAT2 are **untouched**.

## Key seams (verified against `f1bcf42`)

- `config.rs:254` — the reject to **replace with a route**.
- `config.rs:326-327` — `build_aligner_options(cli, aligner, format, is_paired)` builds `aligner_options`.
- `config.rs:363` — `multicore: cli.multicore.unwrap_or(1)` → must become `1` for the HISAT2 route so
  `lib.rs:144/180` takes the single-instance `run_se`/`run_pe` path (NOT `parallel::run_*_multicore`).
- `options.rs:149-158` — the `cli.bowtie_threads` → `-p {p}` + `--reorder` block (NOT Bowtie 2-gated; the
  `// Bowtie 2` comment is a misnomer). This is where the effective `-p N` is emitted.
- `report.rs:67-72` — the report echoes ONLY `aligner_options` (which carries `-p N --reorder`), so a
  Rust `--multicore N` report == a Perl `-p N` report (per-artifact gate holds).
- `tests/methylseq_conformance.rs:211` — `methylseq_align_hisat2_multicore_known_unsupported` (GAP-2 flip-detector).
- `rust/README.md:64-72` — the cpus-cap stop-gap note to relax.

## Design (the route)

In `resolve()` (`config.rs`), before `build_aligner_options`:
```rust
// B-faithful HISAT2 multicore: --multicore N is interpreted as a single instance
// with `-p N --reorder` (HISAT2 splice discovery is not chunk-invariant, so the
// fork model is not faithful; `-p N` is deterministic-per-N — see the Phase-0 spike).
let hisat2_p_threads: Option<u32> =
    if aligner == Aligner::Hisat2 && cli.multicore.unwrap_or(1) > 1 {
        Some(cli.multicore.unwrap())
    } else {
        None
    };
// Q3: an explicit `-p M` AND the remapped `--multicore N` both set is ambiguous → fail loud.
if hisat2_p_threads.is_some() && cli.bowtie_threads.is_some() {
    return Err(AlignerError::Validation(
        "--hisat2 with both --multicore N and -p M is ambiguous: --multicore is interpreted as \
         HISAT2 `-p` threading, so it conflicts with an explicit -p. Pass only one.".into(),
    ));
}
```
- Pass `hisat2_p_threads` into `build_aligner_options` (new param) → the `-p` block emits
  `-p p --reorder` where `p = cli.bowtie_threads.or(hisat2_p_threads)`.
- `multicore:` field becomes `if hisat2_p_threads.is_some() { 1 } else { cli.multicore.unwrap_or(1) }`.
- Emit the never-silent stderr notice when the remap fires.

This leaves Bowtie 2 (`hisat2_p_threads` always `None`) and single-core HISAT2 byte-frozen.

---

## Plan coverage checklist

| # | Plan item | Source section | Task(s) |
|---|-----------|----------------|---------|
| 1 | Route `--hisat2 --multicore N` → single instance `-p N --reorder` (not fork) | Decisions / Phase 1 | Task 2 |
| 2 | `config.multicore` forced to 1 for the HISAT2 route (single-instance dispatch) | Phase 1 / seam C2 | Task 2 |
| 3 | `aligner_options` gains `-p N --reorder` from the multicore value | Phase 1 | Task 2 |
| 4 | Remove the `config.rs:254` reject (replaced by the route) | Phase 1 | Task 2 |
| 5 | Q3 conflict: `--hisat2 --multicore N` + `-p M` → fail-loud | Q3 | Task 1 |
| 6 | Never-silent semantic-remap notice (stderr) | Semantic remap | Task 3 |
| 7 | Conformance flip: `..._known_unsupported` → accept + assert the route | Validation | Task 4 |
| 8 | README stop-gap note relaxed | Validation | Task 5 |
| 9 | e2e: `--hisat2 --multicore 2` runs single-instance (fake HISAT2), SE + PE | Validation matrix | Task 6 |
| 10 | Bowtie 2 `--multicore` (Phase 9b) + single-core HISAT2 untouched (regression) | Assumptions 2 / Regression | Task 2 (test) + Final |
| 11 | `--ambig_bam` under B uses the single-instance path (one instance) | Assumption 3 | Task 6 + Final gate |
| 12 | oxy gate: Rust `--hisat2 --multicore N` == Perl `--hisat2 -p N` per N (BAM+report) | Validation | Final verification |

Every row maps to ≥1 task. ✔

## Test infrastructure

- Unit tests: `rust/bismark-aligner/src/config.rs` `#[cfg(test)]` module (resolve-level tests — fixture-free,
  the reject/route fires before any disk I/O, per the conformance test's own comment).
- e2e: `rust/bismark-aligner/tests/` — reuse the existing **fake-HISAT2** harness (the Phase-2a/2b/9b fakes
  that emit SAM per read and honour `-p`/`--reorder` as no-ops). Confirm the fake ignores `-p` gracefully
  (it should — `-p`/`--reorder` are pass-through flags). If the fake rejects unknown flags, extend it to
  accept-and-ignore `-p`/`--reorder` (test-only).
- Runner: `cargo test -p bismark-aligner -- --test-threads=2` (ETXTBSY write-then-exec flake needs `-2`).
- Gates: `cargo clippy -p bismark-aligner --all-targets -- -D warnings` + `cargo fmt -p bismark-aligner -- --check`.

---

## Task 1 — Q3 guard: `--hisat2 --multicore N` + `-p M` is fail-loud

**Files:** Modify `src/config.rs` (in `resolve`, near the current `:254` reject) + a `#[cfg(test)]` test.

**Step 1: failing test**
```rust
#[test]
fn hisat2_multicore_plus_explicit_p_is_rejected() {
    let cli = Cli::try_parse_from([
        "bismark","reads.fq.gz","--genome","idx","--bam",
        "--hisat2","--multicore","4","-p","2",
    ]).expect("parses");
    let err = config::resolve(&cli, "cmd".to_string())
        .expect_err("--hisat2 --multicore N + -p M must be rejected");
    assert!(err.to_string().contains("ambiguous"), "got: {err}");
}
```
**Step 2: run, confirm fail** — `cargo test -p bismark-aligner hisat2_multicore_plus_explicit_p -- --test-threads=2` (today: resolve still rejects multicore outright with the *old* message → assert on "ambiguous" fails).

**Step 3: implement** — add the `hisat2_p_threads` + Q3 conflict block (see Design) in `resolve`, replacing the `:254` reject `if`. (Task 2 completes the route; this task lands the guard + the `hisat2_p_threads` binding.)

**Step 4: run, confirm pass.** **Step 5:** no refactor. **Step 6:** `cargo test -p bismark-aligner config:: -- --test-threads=2`.

## Task 2 — Core route: single instance `-p N --reorder`, `multicore=1`, reject removed

**Files:** Modify `src/config.rs:254` (remove reject; add route), `:326-327` (pass `hisat2_p_threads`),
`:363` (multicore→1 when routed); `src/options.rs:149-158` (new param + `or(hisat2_p_threads)`); new test.

**Step 1: failing test**
```rust
#[test]
fn hisat2_multicore_routes_to_single_instance_p_threading() {
    let cli = Cli::try_parse_from([
        "bismark","reads.fq.gz","--genome","idx","--bam","--hisat2","--multicore","4",
    ]).expect("parses");
    // resolve() reaches build_aligner_options before any disk I/O for the option string;
    // if a later discovery step needs a real index, assert via build_aligner_options directly:
    let (opts, _) = options::build_aligner_options(&cli, Aligner::Hisat2, ReadFormat::FastQ, false)
        .expect("options build");
    // NOTE: build_aligner_options must now receive the multicore-derived -p; if the signature
    // takes hisat2_p_threads, call with Some(4). Assert the emitted string:
    assert!(opts.contains("-p 4") && opts.contains("--reorder"),
        "HISAT2 --multicore 4 must emit `-p 4 --reorder`: {opts}");
}
```
Plus a resolve-level assertion that the routed `RunConfig.multicore == 1` (use the fixture-free resolve path,
mirroring the conformance test) and that Bowtie 2 `--multicore 4` still yields `multicore == 4` (no `-p`).

**Step 2: run, confirm fail.**

**Step 3: implement**
- `config.rs`: replace the `:254` reject with the `hisat2_p_threads` binding (Task 1) + Q3 guard; pass
  `hisat2_p_threads` to `build_aligner_options`; set `multicore: if hisat2_p_threads.is_some() { 1 } else { cli.multicore.unwrap_or(1) }`.
- `options.rs:149`: `let p = cli.bowtie_threads.or(hisat2_p_threads); if let Some(p) = p { opts.push(format!("-p {p}")); opts.push("--reorder".into()); }` (add the `hisat2_p_threads: Option<u32>` param; Bowtie 2 / minimap2 callers pass `None`). Update the `// Bowtie 2 intra-instance threads` comment (it's aligner-agnostic).

**Step 4: run, confirm pass.**
**Step 5: refactor** — fix the misleading `// 10. -p + --reorder (Bowtie 2 …)` comment to name HISAT2 too.
**Step 6:** `cargo test -p bismark-aligner -- --test-threads=2` (regression: Bowtie 2 multicore + single-core HISAT2 tests stay green).

## Task 3 — Never-silent semantic-remap notice (stderr)

**Files:** `src/lib.rs` (where the run starts / where the existing `deferred_flags`/notices print) or `config.rs`
(emit during resolve). Prefer the same locus as the existing stderr notices.

**Step 1: failing test** — extract the notice into a pure fn returning the string and unit-test it:
```rust
#[test]
fn hisat2_multicore_remap_notice_mentions_p_threading() {
    let s = hisat2_multicore_remap_notice(4);
    assert!(s.contains("--multicore") && s.contains("-p 4") && s.contains("HISAT2"));
}
```
**Step 3: implement** — `fn hisat2_multicore_remap_notice(n: u32) -> String` (e.g. *"Note: --hisat2 with
--multicore N is interpreted as a single HISAT2 instance with -p N threading (HISAT2 splice discovery is
not chunk-invariant; this is deterministic per N but differs from single-core)."*); `eprintln!` it when the
route fires. **Step 4/6:** confirm pass + full suite.

## Task 4 — Conformance flip: GAP-2 `KnownUnsupported` → accept

**Files:** `tests/methylseq_conformance.rs:205-233`.

**Step 1:** rewrite `methylseq_align_hisat2_multicore_known_unsupported` → e.g.
`methylseq_align_hisat2_multicore_accepts_via_p_threading`: assert `resolve(--hisat2 --multicore 2)` is
**Ok** (fixture-free path) with `multicore == 1`, and `build_aligner_options(Hisat2, …, Some(2))` contains
`-p 2 --reorder`. Move the row out of Tier 3 / `KnownUnsupported` into a Tier-1/Tier-3 accept; update the
module-doc comment (the GAP-2 description + the stale `config.rs:251` cite). **Step 2-4:** the test now
passes ONLY after Tasks 1-2; confirm. **Step 6:** `cargo test -p bismark-aligner --test methylseq_conformance -- --test-threads=2`.

## Task 5 — Relax the README stop-gap note

**Files:** `rust/README.md:64-72` (+ the aligner-row line ~156 mentioning `--multicore`+`--hisat2` rejected).
Implementation-first (docs): rewrite the bullet — `--hisat2 --multicore N` is now supported as
single-instance `-p N` threading (deterministic per N, byte-identical to Perl `--hisat2 -p N`, NOT
node-independent); the cpus-cap workaround is no longer required. Keep the "don't override `ext.args`" note.
**Verify:** `grep -n "single-core\|cpus" rust/README.md` reads correctly; the aligner table row updated.

## Task 6 — e2e: `--hisat2 --multicore 2` runs single-instance (SE + PE), incl. `--ambig_bam`

**Files:** `tests/` (reuse the fake-HISAT2 e2e harness). Implementation-first (integration).
**Step 1:** add SE + PE e2e tests: `--hisat2 --multicore 2` produces a BAM (the fake aligner maps),
exits 0, the report's aligner_options contains `-p 2 --reorder`, and a `--ambig_bam` cell writes
`*_bismark_hisat2.ambig.bam` (single-instance path; no multicore temp-name machinery). Assert NO
`parallel::run_*_multicore` path is taken (e.g. via output-naming or a single fake-invocation count).
**Step 2: verify** — `cargo test -p bismark-aligner --test <name> -- --test-threads=2`.

---

## Final verification

1. **Local:** `cargo test -p bismark-aligner -- --test-threads=2` (all green) + `cargo clippy -p
   bismark-aligner --all-targets -- -D warnings` + `cargo fmt -p bismark-aligner -- --check`.
2. **oxy B-faithful byte-identity gate** (the real validation; build Rust on oxy via tar|dcli ssh, cargo
   1.96 --release; HISAT2 2.2.2 + Perl v0.25.1; reads `~/bismark_benchmarks`):
   - **Oracle = Perl `--hisat2 -p N`** (NOT single-core, NOT `--multicore N`), per matching N ∈ {2,4,8}.
   - Compare **decompressed BAM** (`samtools view`, @PG-filtered — Rust argv has `--multicore N`, Perl has
     `-p N`; both filtered) + **report** (wall-clock + version filtered; aligner_options must match — both
     carry `-p N --reorder`, verified: report echoes only `aligner_options`).
   - **Matrix:** SE + PE × {directional, non-directional, pbat} × {FastQ, FastA} + `--ambig_bam` /
     `--unmapped` / `--ambiguous` (a justified subset is acceptable — the strand/format machinery is
     unchanged from the shipped single-core HISAT2 gate; the new variable is purely `-p N`).
   - **Regression cells:** Bowtie 2 `--multicore N` worker-invariance (Phase 9b) unchanged; single-core
     `--hisat2` (no `-p`) unchanged. Write `GATE_OXY.md`.
3. Dual `/code-reviewer` (Agent, fresh context) + `/plan-manager` → COVERAGE COMPLETE.

## Commit plan

- One commit (squash-merge anyway): `feat(aligner): --hisat2 --multicore N via single-instance -p N threading (B-faithful)`.
- Stage: `src/config.rs`, `src/options.rs`, (maybe `src/lib.rs` for the notice), `tests/methylseq_conformance.rs`,
  the new e2e test file, `rust/README.md`, + the plans/ artifacts.
- rust/README.md status-journal Milestones line added **at merge into iron-chancellor** (epic convention), not on the feature commit.
- On merge: cut beta.6 (bump `rust/VERSION` + 3 mirror literals → dry-run → publish) + bump the methylseq pin :2.0.0-beta.5→beta.6 — **only on Felix's explicit go**.
