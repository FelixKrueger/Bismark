# IMPL — Phase C: real-data byte-identity gate (the v1.0 release gate)

**Source plan:** `SPEC.md` (rev 3) §11 row C + §12 "Real-data gate". Goal: prove the Rust `NOMe_filtering_rs` is **byte-identical to Perl `NOMe_filtering` v0.25.1** on a *real* `--yacht` input + genome, then tag `bismark-nome-filtering-v1.0.0-beta.1`. Mirrors the `coverage2cytosine` Phase-E / RELEASE_CHECKLIST pattern.

**Mode:** implementation-first (an ops/driver harness + release checklist — no new Rust code; Phases A+B are the implementation). **Runs on a remote machine with the benchmark data; cannot be executed from the dev box.**

> **Machine: oxy** (Felix directive 2026-05-31 — overrides the SPEC's earlier colossal default). oxy is where the `coverage2cytosine` Phase-E gate ran (2026-05-30), so it's a proven host for these gates; its ~99 GB home cap (the reason c2c needed a stream-compare workaround for its genome-wide multi-GB report) is a **non-issue here** — NOMe output is **per-read and small**, so a plain `cmp` is fine. ⚠️ oxy access was marked *deprecated* 2026-05-28 (`reference_colossal_access`) then used again for c2c's Phase E on 2026-05-30 — **re-verify connection + env + paths first session.** Env: **micromamba** env `bismark-test` (Perl v0.25.1). Data: `~/bismark_benchmarks/` (verify exact subpath). **Rust toolchain likely NOT pre-installed** — `rustup` via curl (Step 0). Dev-box `ssh`/`cargo` calls need `dangerouslyDisableSandbox: true`.

> **Why this gate is real:** the two pipelines do NOT share a NOMe producer — Perl `NOMe_filtering` vs Rust `NOMe_filtering_rs`, on a **common, independently-produced `--yacht` input**. (The `--yacht` producer upstream is shared and irrelevant to the comparison.)

---

## Scope of the matrix
`NOMe_filtering` has **no behaviour-affecting options** beyond the core filter (all of `--zero_based`/`--CX`/`--GC`/`--gzip`/`--nome-seq`/`--merge_CpGs` are inert; SPEC §4). So the gate is not a flag cross-product — it is **"run the core filter on real data, two input forms, and `cmp`"**:

| Cell | Input | Assert |
|------|-------|--------|
| C1 | a real `--yacht` file (plain `.txt`), full sample | decompressed output byte-identical Perl≡Rust |
| C2 | the same input gzipped (`.txt.gz`) | decompressed output byte-identical Perl≡Rust **and** identical to C1 |
| C3 | (if available) a **single-cell NOMe-Seq** SE sample (the tool's intended data) | byte-identical Perl≡Rust |

Single-end only (NOMe_filtering + `--yacht` are SE-only). If no native NOMe-Seq SE sample is on oxy, generate the `--yacht` input from a benchmark SE alignment (or R1-as-SE of the PE WGBS set) — the gate compares Perl-NOMe vs Rust-NOMe on a common input, so any real `--yacht` file is a valid stressor.

## Step 0 — toolchain + build (oxy)
```bash
ssh oxy        # re-verify the exact access method first session (access was deprecated 2026-05-28; c2c used oxy 2026-05-30)
# Rust toolchain (likely not pre-installed):
command -v cargo || curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
# Clone/fetch this branch + build the release binary:
cd ~/Bismark-nome 2>/dev/null || git clone <repo> ~/Bismark-nome
cd ~/Bismark-nome && git fetch origin && git checkout rust/nome-filtering && git pull
cargo build --release -p bismark-nome-filtering   # → rust/target/release/NOMe_filtering_rs
cargo test  -p bismark-nome-filtering              # sanity: synthetic goldens pass on oxy too
RUST_NOME=~/Bismark-nome/rust/target/release/NOMe_filtering_rs
PERL_NOME=~/Bismark/NOMe_filtering                 # the repo Perl v0.25.1 (verify path on oxy)
perl "$PERL_NOME" --version                        # confirm "v0.25.1"
```

## Step 1 — produce a real `--yacht` input
```bash
micromamba activate bismark-test
GENOME=$HOME/bismark_benchmarks/genome             # dir of .fa (verify exact subpath on oxy)
SE_BAM=<a single-end Bismark BAM>                  # or R1-as-SE
mkdir -p ~/nome_yacht
# --yacht writes a single `any_C_context_<basename>` per-call file (SE only):
bismark_methylation_extractor -s --yacht --genome_folder "$GENOME" -o ~/nome_yacht "$SE_BAM"
YACHT=$(ls ~/nome_yacht/*any_C_context* | head -1)   # confirm the file
wc -l "$YACHT"                                        # record the call count
```

## Step 2 — the byte-identity driver
Save as `~/nome_gate.sh` on oxy (also committed at `tests/data/phase_c/nome_gate.sh` for the record):
```bash
#!/usr/bin/env bash
# Phase C real-data byte-identity gate: Perl NOMe_filtering vs Rust NOMe_filtering_rs.
set -euo pipefail
export LC_ALL=C
PERL_NOME=${PERL_NOME:?}; RUST_NOME=${RUST_NOME:?}
GENOME=${1:?genome dir}; YACHT=${2:?yacht input}
TS=$(date +%Y%m%d_%H%M%S)
OUT=$HOME/nome_byte_identity_$TS            # distinct out-dir (Felix directive)
mkdir -p "$OUT/perl" "$OUT/rust"
base=$(basename "$YACHT")
cp "$YACHT" "$OUT/perl/$base"; cp "$YACHT" "$OUT/rust/$base"

perl "$PERL_NOME" -g "$GENOME" --dir "$OUT/perl" "$base"
"$RUST_NOME"      -g "$GENOME" --dir "$OUT/rust" "$base"

# Output name derivation: NOMe strips one .gz + one .txt, appends .manOwar.txt.gz.
out=$(python3 - "$base" <<'PY'
import sys
s=sys.argv[1]
if s.endswith('.gz'): s=s[:-3]
if s.endswith('.txt'): s=s[:-4]
print(s+'.manOwar.txt.gz')
PY
)
echo ">>> comparing decompressed $out"
if cmp <(gunzip -c "$OUT/perl/$out") <(gunzip -c "$OUT/rust/$out"); then
  md5=$(gunzip -c "$OUT/rust/$out" | md5sum | awk '{print $1}')
  lines=$(gunzip -c "$OUT/rust/$out" | wc -l)
  echo "PASS  byte-identical  md5=$md5  lines=$lines"
  rm -rf "$OUT"                              # purge on pass
else
  echo "FAIL  outputs differ — preserved at $OUT"; exit 1
fi
```
Run it for C1 and C2:
```bash
chmod +x ~/nome_gate.sh
PERL_NOME=$PERL_NOME RUST_NOME=$RUST_NOME ~/nome_gate.sh "$GENOME" "$YACHT"          # C1 plain
gzip -kf "$YACHT"
PERL_NOME=$PERL_NOME RUST_NOME=$RUST_NOME ~/nome_gate.sh "$GENOME" "$YACHT.gz"       # C2 gz
```
Both must print `PASS byte-identical`. (No sort step — NOMe output is emission-ordered; `LC_ALL=C` set defensively.)

## Step 3 — record + (optional) perf note
- Record the `md5` + line count of the decompressed report and the input call count in `RELEASE.md` (and here).
- Wall-clock Perl vs Rust (`/usr/bin/time -v`) — **advisory only** (perf is not a v1.0 gate, SPEC §2). Report fairly (NOMe is tiny vs extraction; do not over-claim).

## Step 4 — RELEASE_CHECKLIST → tag
- [ ] oxy: `rustup` installed; `cargo build --release -p bismark-nome-filtering` clean; `cargo test -p bismark-nome-filtering` green on oxy.
- [ ] Perl `NOMe_filtering --version` == `v0.25.1`.
- [ ] Real `--yacht` input generated (record path + call count).
- [ ] **C1 (plain) PASS** byte-identical (record md5 + lines).
- [ ] **C2 (gz input) PASS** byte-identical, and md5 == C1.
- [ ] **C3** (native single-cell NOMe-Seq SE sample) PASS — or documented as "no NOMe-Seq sample on oxy; gated on benchmark SE `--yacht`".
- [ ] `CHANGELOG.md` + crate `README.md` added/updated for `bismark-nome-filtering` (binary name `NOMe_filtering_rs`, usage, the `.manOwar.txt.gz` output, the two-plain-suffix `.fa`/`.fasta` genome footgun).
- [ ] Commit the driver (`tests/data/phase_c/nome_gate.sh`) + `RELEASE.md` (the recorded md5/lines/host).
- [ ] **Tag `bismark-nome-filtering-v1.0.0-beta.1`** (matches the c2c beta-tag cadence); push.
- [ ] `/progress update` + memory update (Phase C GREEN, tagged).

## Open items / coordination
- **`-o`/output-name:** confirm the extractor hands NOMe a bare filename + `--dir` (the path contract, SPEC §4) on the real invocation; the driver uses `--dir` + bare basename to match.
- **Inline switch:** wiring `NOMe_filtering_rs` into a future Rust NOMe-Seq pipeline (replacing the Perl subprocess) is **out of scope** for v1.0 — a later coordination item, like the c2c extractor inline switch.
- **Pre-folded from Phase-B review (already in the crate):** reverse-strand + multi-chr goldens, `GzEncoder<BufWriter<File>>`, accepted-divergences doc — nothing outstanding for Phase C beyond the gate itself.

## Notes
This phase is **executed by Felix / an oxy session** — it cannot run from the dev box (remote data + no local benchmark). The deliverable here is the harness + checklist; the gate result + tag are recorded back into `PROGRESS.md` + `RELEASE.md` + memory once run.
