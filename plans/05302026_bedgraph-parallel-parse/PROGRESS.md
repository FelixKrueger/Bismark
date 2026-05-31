# PROGRESS — `bismark2bedGraph_rs` parallel per-file parse (Family A)

**Plan:** `PLAN.md` (rev 0, 2026-05-30) · **Spike:** `../05292026_bismark-bedgraph/spikes/SPIKE_read_phase_split.md`
**Branch (planned):** `rust/bedgraph-parallel-parse` off merged `rust/iron-chancellor`.
**One-line goal:** always-on concurrent per-file parse + argv-order batched merge → ~2.0× on full `--CX`, byte-identical (N-invariant).

## Pipeline status

| Step | Status | Notes |
|------|--------|-------|
| 1. Spike (read-phase split) | ✅ done | insert-bound (CHH 79.5%), merge ~2% → Family A; ~2.0× projected |
| 2. Plan written | ✅ done | `PLAN.md` rev 0 → **rev 1** (review folded) |
| 3. Manual review (Felix) | ✅ done | approved 2026-05-30 |
| 4. Dual plan-review (agents) | ✅ done | A + B both APPROVE WITH CHANGES (no design rework); `PLAN_REVIEW_A.md`/`_B.md`; all Critical+Important folded into rev 1 |
| 5. Implementation | ✅ done (local) | branch `rust/bedgraph-parallel-parse`; `error.rs`+`cli.rs`+`aggregate.rs`+`parallel.rs`(new)+`lib.rs`; fmt + clippy `-D warnings` clean; 74 unit + 8 fixtures + 5 CLI + 3 doctests pass (V1–V6, V9–V12) |
| 6. Verify | ✅ done | dual `code-reviewer` APPROVE + `plan-manager` COMPLETE. **Oxy `--CX` gates (×2) + controlled experiment: byte-identity PERFECT (N-invariant, == Perl), but parallel ANTI-SCALES (memory-bandwidth-bound; sequential fastest, allocator+sharding-independent).** See §14. |
| 7. Disposition | ✅ done (Felix 2026-05-31) | **Removed `--parallel` machinery; kept mimalloc only** (free ~12% sequential win). End state = #893 + mimalloc; tracked diff = `Cargo.toml`+`main.rs`+`Cargo.lock`. Rebuilt clean: 64 unit + 8 fixtures + 5 CLI + 3 doctests. Branch `rust/bedgraph-parallel-parse` ready (uncommitted; awaiting commit/PR decision). |

## Key decisions

- **D-PP1:** always-on parallel, optional `--parallel N` cap, `N=1` = sequential (Felix, 2026-05-30; byte-identity is N-invariant).
- **D-PP2:** merge-in-argv-order reuses existing `order_key` (first-wins) — no `into_sorted` change; decoupled-ownership variant deferred.
- **D-PP3:** batched (chunked) merge for v1 — `threads` is both the parallelism and peak-RAM lever (~46–67 GB on `--CX`).

## Open (non-blocking)

- `--parallel` default: `min(#files, cores)` for now (files self-cap at ≤6/≤12).
- Streaming merger-thread (~46 GB regardless of argv order) — future RAM optimization.

## Gotchas to carry into implementation

- `aggregate.rs` is the **most byte-identity-load-bearing module** — mandatory real-data re-gate at `--parallel 1` AND `--parallel 6` (each identical to Perl + to each other).
- One `Aggregator` **per file**, never per worker (a worker accumulating 2 files breaks within-worker argv ownership).
- `--parallel` must NOT touch the gzp compression thread pool (separate, already saturated at 4).
- oxy inputs live at `/tmp/bg_keep/ext_full/` (ephemeral); worktree `/tmp/Bismark-bg` (target `target_oxy`).
