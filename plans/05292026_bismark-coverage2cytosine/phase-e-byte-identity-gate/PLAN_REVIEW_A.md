# Phase E PLAN — Review A (Reviewer A, independent)

**Plan under review:** `phase-e-byte-identity-gate/PLAN.md` (rev 0) — real-data byte-identity gate (oxy), a harness (`scripts/c2c_byte_identity_matrix.sh`) + RELEASE checklist, NOT crate code.
**Reviewer:** A (fresh context; no coordination with Reviewer B).
**Method:** plan-reviewer SKILL.md methodology (logic / assumptions / efficiency / validation-sufficiency / alternatives). Claims verified against the **live Perl `coverage2cytosine` v0.25.1** at the worktree root on the `phase_b`/`phase_d` fixtures (no oxy needed). Live-Perl checks are cited inline.

---

## TOP-LINE VERDICT: **APPROVE-WITH-CHANGES**

The plan is well-grounded, models the proven `phase_h_se_matrix.sh` house pattern faithfully, and its two-axis design (per-cell Rust≡Perl compare + cross-cell differential to catch both-no-op) is the right shape. The fail-CLOSED discipline is genuine, not cosmetic, and the disk-mitigation triad (gzip heavy cell + stream-compare + purge-on-pass + disk-floor pre-flight) is a sound answer to the oxy ~99 GB crux.

However, there are **two Critical fail-open / false-result holes** and several Important gaps that should be folded before implementation. None is design-blocking; all are concrete and fixable in the harness.

**Finding counts:** Critical 2 · Important 5 · Minor 4 · Nit 3.

---

## CRITICAL

### C1 — gzip stream-compare (`cmp -s <(gzip -dc R) <(gzip -dc P)`) is fail-OPEN on decompression failure (§3.4.2, §6, §12.3)
**Issue.** Process substitution does **not** propagate the exit status of the `gzip -dc` inside `<(...)` to `cmp`. `cmp` only sees the bytes that arrived on the FIFO. I verified this live:

- A corrupt/truncated `.gz` on one side → `gzip -dc` prints an error to its own stderr and exits non-zero, but `cmp` sees a short stream and judges purely on bytes received (cmp rc reflects the byte diff, not the gzip failure).
- **Both sides identically truncated** (e.g. disk fills mid-write while both producers stream output) → `cmp -s` returns **0 = PASS** on two corrupt decompressions. Live demo: `head -c 20 full.gz` copied to both sides → `cmp -s <(gzip -dc a) <(gzip -dc b)` = IDENTICAL = false PASS. `gzip -t` on the same file returns rc=1 ("unexpected end of file").

**Why it matters.** Disk-full is not a hypothetical here — it is **the** failure mode the gate is explicitly engineered to survive (oxy's 99 GB cap is the dominant constraint, §6). The gate must NOT report a green matrix on a run where a heavy gzipped output was truncated by disk exhaustion. This is exactly the `count_mbias_rows` fail-open lesson (`phase_h_se_matrix.sh:387-403`) reincarnated in the gzip path: a check whose whole purpose is fail-closed has a silent fail-open seam.

**Suggested fix.** Before (or as part of) every gzip stream-compare: run `gzip -t "$R"` **and** `gzip -t "$P"` and treat a non-zero rc as a FAIL for that stream (integrity precedes content). `gzip -t` streams (no temp file, no disk cost) and is O(bytes) like the compare. Optionally additionally capture `PIPESTATUS`/use `set -o pipefail` with a piped form, but `gzip -t` on both sides is the clean, disk-cheap closure. Add a V-row asserting a truncated `.gz` input FAILs.

### C2 — Existence+non-empty guard (§3.4.1) will false-FAIL on legitimately-empty per-chromosome reports in the `split` cell on full hg38 (§3.4, §3.5)
**Issue.** §3.4.1 makes "missing or zero-bytes on either side ⇒ FAIL — never a vacuous pass" the universal rule, exempting only the `discordant` file. §3.5 then says for the `split` cell: "Compare **each** per-chr report file byte-for-byte" with no empty-exemption. But a per-chromosome CpG report **can be legitimately zero bytes**. I verified live on the `phase_b` fixture with `--split_by_chromosome`:

```
split.chrscaf_short.CpG_report.txt          = 0 bytes  (scaffold "CG", len 2, all positions filtered by the len<3 guard)
split.chrscaf_short.cytosine_context_summary.txt = 1310 bytes (NON-empty — it was the last-processed chr)
```

Full hg38 has **hundreds** of short/unplaced/alt scaffolds (`chrUn_*`, `*_random`, decoy/short contigs) that will produce **empty `.chr<NAME>.CpG_report.txt`** files. If the §3.4 non-empty guard is applied to per-chr reports, the `split` cell **false-FAILs** the matrix and blocks the v1.0 tag on a correct run.

**Why it matters.** This is a false-FAIL (the inverse of fail-open but still a release-gate defect): a byte-perfect Rust would be rejected. On the tiny fixture it would already trip if the guard were naive.

**Suggested fix.** In the `split` handler (§3.5), the non-empty guard must apply to the **file-name set** and to non-empty files only; an empty per-chr report is valid IFF it is empty on **both** sides (byte-identity still holds: `cmp` of two empty files = PASS). Replace "missing/empty ⇒ FAIL" with "missing ⇒ FAIL; empty-on-one-side-only ⇒ FAIL; empty-on-both ⇒ PASS" for per-chr reports (and apply the same both-sides-empty logic to the per-chr summaries, of which all-but-the-last are empty on both sides — see §3.5). State the rule explicitly so the implementer does not inherit the global §3.4 guard verbatim.

---

## IMPORTANT

### I1 — Merged/discordant stream filenames carry a `.CpG_report` infix the plan's stream names omit (§3.2, §3.4, §3.6.5, §8)
**Issue.** The plan refers to streams as `{stem}.merged_CpG_evidence.cov` and `{stem}.discordant_CpG_evidence.cov`. Live Perl (with `-o md --merge_CpGs --discordance_filter 10`) actually writes:

```
md.CpG_report.merged_CpG_evidence.cov
md.CpG_report.discordant_CpG_evidence.cov
md.CpG_report.txt
md.cytosine_context_summary.txt
```

and with `--gzip`: `mg.CpG_report.merged_CpG_evidence.cov.gz`. The merge pass derives its filename from the **just-written report path** (`{stem}.CpG_report` + `.merged_CpG_evidence.cov`), so the `.CpG_report` infix is present. If the comparison helper hard-codes `${STEM}.merged_CpG_evidence.cov`, it will look for a file that doesn't exist → the §3.4 existence guard fires → **false FAIL** (or, if the helper globs loosely, a possible mismatch).

**Why it matters.** The whole `merge`/`merge_disc`/`merge_gzip` row of the matrix (3 of 9 cells) hinges on locating these files. A wrong assumed name breaks 3 cells.

**Suggested fix.** Pin the exact filename derivation in the plan (verify against local Perl, as the plan already does for `--version`): `{stem}.CpG_report.merged_CpG_evidence.cov[.gz]` and `{stem}.CpG_report.discordant_CpG_evidence.cov[.gz]`. Prefer globbing the directory for `*merged_CpG_evidence.cov*` / `*discordant_CpG_evidence.cov*` and asserting exactly one match per side, rather than constructing the name.

### I2 — `merge_disc` cell can produce an **empty** merged-cov, contradicting the "merge merged-cov non-empty" framing (§3.4, §3.6.5, §8)
**Issue.** §3.4 says "the `merge` merged-cov must be non-empty given the 10M cov has CpG coverage" and §3.6.5 asserts "`merge` merged-cov non-empty." But the **discordance filter routes discordant pairs OUT of the merged file and skips merging them** (SPEC §9). Live on the fixture (`--merge_CpGs --discordance_filter 10`): the single CpG pair was discordant → `md.CpG_report.merged_CpG_evidence.cov` = **0 bytes**, while `md.CpG_report.discordant_CpG_evidence.cov` = 51 bytes. The non-empty invariant is true for the **`merge` cell** (no discordance filter) but is **not guaranteed for `merge_disc`** in principle.

**Why it matters.** On real 10M hg38 data the `merge_disc` merged file will be huge (the vast majority of CpGs are concordant), so this won't bite in practice. But the plan should not *assert* `merge_disc` merged-cov non-empty as a hard guard, or it risks a false-FAIL on a pathological dataset; conversely it should ensure the **`merge_disc` discordant file** is the one exempted from non-empty (it may also legitimately be empty if no pair is discordant — the §3.4 exemption already covers `discordant`, good). Scope the non-empty merged-cov guard to the **`merge`** cell only; for `merge_disc`, assert `merged + discordant` are jointly the partition (or just byte-compare both Rust≡Perl without a non-empty merged guard).

### I3 — No per-cell disk re-check; a mid-matrix FAIL (keep-all) can starve a later heavy cell and revert to the confusing crash the gate promises to prevent (§3.1.7, §3.7)
**Issue.** The disk-headroom gate (§3.1.7) is a **one-shot pre-flight** (default 30 GB floor). Purge-on-pass (§3.7) frees space only on PASS; a FAIL **keeps everything** for investigation (correct for evidence). A sequence of FAILs before the `cx` cell (~20 GB peak for both gzipped sides) could accumulate retained outputs and exhaust disk *mid-`cx`-run* — at which point the gate hits exactly the "oxy ran out of disk mid-run → confusing crash" failure mode it was designed to convert into a clean refusal.

**Why it matters.** The gate's headline value proposition (§3.1.7 comment) is "clear pre-flight refusal, not a confusing crash." A single pre-flight check does not deliver that once cells start retaining FAIL outputs.

**Suggested fix.** Re-check free space **before each cell** (cheap `df -Pk`); if below a per-cell estimate (or the floor), skip remaining cells and exit 2 with a clear "disk exhausted after N retained FAIL(s); free space or re-run with cleaned --out" message. Alternatively, order the heavy `cx` cell **first** so its disk need is validated by the pre-flight floor before any other cell can retain output.

### I4 — `cx > default` line-count differential requires a second full decompression pass over the ~40 GB CX stream; ordering vs purge is underspecified (§3.6.1, §5.8, §3.7)
**Issue.** §3.6.1 compares decompressed CX line count vs CpG report line count. §5.8 correctly says to stash `lines_default`/`lines_cx` during the cell loop "since PASS purges outputs." But computing `lines_cx` means a `gzip -dc cx.CX_report.txt.gz | wc -l` over ~40 GB **in addition to** the byte-compare's `gzip -dc` pass — i.e. the 40 GB CX is decompressed **twice** (once to compare, once to count). On oxy single-threaded this is a meaningful wall-clock cost on the heaviest cell.

**Why it matters.** Efficiency on the binding-constraint cell; also a latent ordering bug if the implementer counts lines *after* purge (the file is gone).

**Suggested fix.** Fold the line-count into the compare pass: `cmp` consumes the streams, so either (a) count during the cell run from the producer's own pass, or (b) use `gzip -dc R | tee >(wc -l >countfile) | cmp - <(gzip -dc P)` to get the count for free in the single decompression. Make the plan state that all stashed line-counts (§5.8) are computed **before** the per-cell purge (§3.7) — call out the ordering as a hard sequence, not just "or compute during the loop."

### I5 — Plain `CX_report.txt` (no gzip) is never byte-asserted on real data; the gate's coverage of the un-gzipped CX writer path rests entirely on tiny fixtures (§3.2 note)
**Issue.** The `cx` cell uses `--CX --gzip`. So on the real-data gate, the **plain (uncompressed) CX writer code path** is never exercised end-to-end against Perl — only the gzipped path is. The plain CpG path is covered (`default`), plain merged (`merge`), gzip CpG (`gzip`), gzip merged (`merge_gzip`), gzip CX (`cx`). The one hole is **plain CX**.

**Why it matters.** A defect that only manifests in the plain-CX writer at genome scale (e.g. a buffering/flush edge that the GzEncoder masks) would slip the release gate. The plan's rationale (disk: plain CX = 40 GB) is legitimate, and the integration fixtures (SPEC §12.2) do diff plain CX — so this is acceptable, but it should be **explicitly acknowledged as a deliberate gap** in the plan + checklist (it currently reads as if `cx` covers "the CX stream" without noting only the gzip variant is real-data-gated).

**Suggested fix.** Add a one-line note to §3.2 / the checklist: "plain `--CX` (no gzip) is covered by the §12.2 integration fixtures only; the real-data gate asserts the gzipped CX path (disk). The gzip differential (§3.6.3) proves gzip ≢ content-altering, so the plain bytes are transitively pinned." Or, if disk allows after the fit-check (Q1), run one plain-CX cell on a **chromosome-subset** genome.

---

## MINOR

### M1 — Perl-version assertion: the c2c provenance string differs from the extractor's; the grep target must be the c2c one (§3.1.5, §5.2)
The plan says to "adapt the extractor's `--version` grep." The extractor prints `Bismark Extractor Version: v0.25.1`; the **c2c** `--version` (verified live) prints:
```
                    Bismark Methylation Extractor Module -
                        coverage2cytosine
                       Version: v0.25.1
```
The greppable token is `Version: v0.25.1` (no "Extractor Version" prefix; leading whitespace; "coverage2cytosine" appears on a separate line). The plan does flag "verify the exact text vs the local binary" (Q-A10, §5.2) — good — but the implementer should grep on a c2c-specific anchor (e.g. `coverage2cytosine` + `Version: v0.25.1`), not copy the extractor's `Bismark Extractor Version:` regex, which would never match and hard-fail the pre-flight on a correct binary.

### M2 — Empty-cov behavior: Perl creates 0-byte report + summary, then dies (exit 255) — relevant to the V2 self-test (§7.6 SPEC, §9 V2)
Verified live: an empty `.cov` → Perl writes `empty.CpG_report.txt` (0 B) and `empty.cytosine_context_summary.txt` (0 B), prints "No last chromosome was defined…" to STDERR, and exits **255**. There is no `empty` cell in the matrix (correct — out of scope), but the V2 self-test ("make a cell produce no report") should be aware that the Perl path can leave **0-byte files plus a non-zero exit**, so V2 must assert on the **harness** verdict (exit 1 "missing/empty required output"), distinguishing the producer's own non-zero exit (recorded + continue, §3.3) from the harness's compare verdict. Worth a sentence in V2.

### M3 — `merge_gzip` cell asserts only ONE stream; its summary (always produced, always uncompressed) is not compared (§3.2)
Live `--merge_CpGs --gzip` produces `mg.cytosine_context_summary.txt` (non-gzipped, 1310 B). The `merge_gzip` row lists only `merged_CpG_evidence.cov.gz`. The summary is already byte-checked in `merge`/`merge_disc`, so this is not a correctness hole, but listing the summary in `merge_gzip` too (it's free — small, uncompressed) would make every cell's summary assertion uniform and guard against a gzip-mode-specific summary regression.

### M4 — `thr < default` differential: safe here, but note threshold also drops **entire uncovered chromosomes** (§3.6.4)
Verified: `thr 5` → 2 lines vs default 18. The invariant holds. Worth noting in the plan that the drop is twofold (uncovered positions **and** uncovered chromosomes are skipped when `threshold>0`, SPEC §7.5), so the `thr` line count is far below `default`, not marginally — the strict `<` is robust. No change needed; just ensures the implementer doesn't write a too-tight bound expecting a small delta.

### M5 — `int()` truncation in the disk-floor awk is conservative (safe), but document the unit (§5.2, §3.1.7)
`df -Pk | awk 'NR==2{print int($4/1024/1024)}'` — verified `df -Pk` (`-P` POSIX) keeps fields on one line (no wrap), `$4`=Available KiB, `/1024/1024`=GiB, `int()` truncates **down** (refuses slightly earlier → safe direction). Sound. Just state in the plan that the floor is compared in **GiB** and truncation is intentional (conservative).

---

## NIT

### N1 — `LC_ALL=C` export (§3.1.8): c2c report ordering is genome-driven, not sort-driven; the only sort-sensitive step is the uncovered-chromosome bytewise sort (SPEC §7.5) and the context-summary key sort (§8). `LC_ALL=C` is still correct + necessary, but the plan's "any sort-dependent step" is vaguer than it needs to be — name the two (uncovered-chr order; summary row order) so the reviewer/operator knows what it actually guards.

### N2 — STDERR exemption is empirically safe to ignore in the compare: verified the producer writes **0 bytes to STDOUT** and all chatter (2232 B) to STDERR. The harness compares files only, so no STDOUT/STDERR capture is needed — worth a one-line confirmation in §3.3 so a future maintainer doesn't add STDOUT diffing.

### N3 — Checklist placement (Q4): the default (separate `RELEASE_CHECKLIST_c2c.md`) is the right call (keeps the two crates' gates independent; the extractor's `RELEASE_CHECKLIST.md` already exists at the worktree root). No change; just confirming the default over the alternative.

---

## Cross-check summary (validation sufficiency)

- **Matrix coverage of v1.0 streams/flags (SPEC §2/§3/§5):** GOOD with two documented gaps — plain CX never real-data-gated (I5, acceptable via fixtures), and the always-on summary not asserted in `merge_gzip` (M3, redundant). Every v1.0 flag is exercised in at least one cell; mutex rules respected. The always-on `cytosine_context_summary.txt` is checked in all non-split cells except `merge_gzip` (M3); in `split` it's handled via the last-chr rule (C2 refinement needed).
- **Fail-CLOSED completeness:** Strong design intent, but **two real holes**: gzip decompression fail-open (C1) and the per-chr empty-report false-FAIL (C2). The V1 deliberate-diff self-test is necessary and well-placed, but V1 as written injects a **content** diff into a plain output — it does NOT exercise the gzip path; add a V1b that corrupts/truncates a `.gz` to prove C1's fix.
- **Cross-cell differentials (§3.6):** All five invariants verified true on the live fixture (CX 25 > CpG 18; zero ≠ default; thr 2 < default 18; merge non-empty for the no-discordance case; split file-count > 1). The `merge` non-empty one needs scoping away from `merge_disc` (I2).
- **Disk math (oxy crux):** The triad is sound and the ~20 GB peak / 30 GB floor is plausible, BUT the floor is one-shot (I3) and the CX line-count double-decompress (I4) inflates the heaviest cell. Process-substitution compare is correct for *content* but fail-open on *decompression failure* (C1).
- **Independent-producer property (SPEC §13):** Genuinely preserved — Perl-`bismark2bedGraph` cov fed to BOTH Perl-c2c and Rust-c2c is a true two-producer test (the c2c producers differ; only the shared upstream cov is Perl). Not a tautology. Good.
- **oxy specifics (Q3):** Handled safely — historical access values flagged "VERIFY first session," disk-floor pre-flight refuses cleanly if the env/paths drifted into a too-small mount. The Perl-version assertion is sound (M1 caveat on the exact string).

## Action items (priority order)
1. **C1** — `gzip -t` both sides before every gzip stream-compare; add V1b (truncated-gz ⇒ FAIL).
2. **C2** — `split` per-chr reports: empty-on-both ⇒ PASS, empty-on-one ⇒ FAIL; do not inherit the global non-empty guard.
3. **I1** — pin/glob the `.CpG_report.merged_CpG_evidence.cov[.gz]` / `.discordant_…` filenames (verify vs local Perl).
4. **I2** — scope the "merged-cov non-empty" guard to the `merge` cell only.
5. **I3** — per-cell disk re-check (or run `cx` first).
6. **I4** — compute stashed line-counts in the single decompress pass, before purge; state the ordering as a hard sequence.
7. **I5** — document the plain-CX real-data gap explicitly.
8. Minors M1–M5, Nits N1–N3 as noted.

**Recommend-only.** No tracked files were modified. Live-Perl checks run in `$TMPDIR` against the `phase_b`/`phase_d` fixtures and the worktree-root Perl `coverage2cytosine` v0.25.1 (`--version` confirmed). This report is the only file written.
