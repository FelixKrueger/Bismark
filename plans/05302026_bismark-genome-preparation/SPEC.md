# SPEC — `bismark-genome-preparation` (Rust port of Perl `bismark_genome_preparation`)

**Status:** DRAFT rev 3 (2026-05-30) — scope decisions resolved (§9); **dual plan-review findings folded in** (`PLAN_REVIEW_A.md`/`PLAN_REVIEW_B.md`). Awaiting implementation trigger. Do **not** implement.
**Date:** 2026-05-30
**Rev 1 changes:** all four §9 open decisions resolved — minimap2 **kept** (external subprocess, all 3 indexers); `--slam` **in v1.0** but marked deprecated; `--genomic_composition` **deferred**; indexer builds run **concurrently**. Added the alanhoyle-port reference (incl. the byte-divergences to *avoid*), the slam header-suffix fidelity rule, the raw-line transform approach, and the rammap forward note (§3/§8/§9).
**Rev 2 changes:** added the optional **combined-genome** Bismark-Rust extension (§10, new `--combined_genome` flag) — *additive* (standard CT/GA outputs unchanged), opt-in (default OFF), produces a single combined CT+GA reference FASTA **and** a combined index for the upcoming aligner rewrite to consume. No Perl counterpart → not byte-identity gated; alignment-correctness validation is explicitly deferred to the aligner rewrite. **Note:** all committed/public artifacts (this SPEC, plans, code, docs, issues) intentionally avoid naming external tools — design rationale is tracked in internal memory only.
**Rev 3 changes (dual plan-review folded in):** corrected the **chromosome-name extraction contract** — a bare `>` first line is **NOT** an error (Perl yields an empty name → `>_CT_converted`); a header with **leading whitespace** after `>` yields an **empty** name (Perl `split /\s+/` keeps the leading empty field — use a split that does **not** skip it, **not** `split_whitespace()`); only a first line whose first byte is **not** `>` is an error (§2.5, §8.7, §8.9). Combined-genome byte-oracle redefined to be built **directly from the converted sequence stream** (independent of `--single_fasta`), not by concatenating MFA files that may not exist (§10). Indexer: **always emit `--threads N`** (N=1 default, Perl-faithful) and adopt the extractor's **`BISMARK_BIN → which → current_exe`** discovery tier; **`--path_to_aligner` validated early (Step I)**, no `which`-fallback when an explicit path is given (§2.6, §4.7). Glob sort keyed on **`file_name()` bytes** (C-locale; §8.1). Added CR-only-line-ending and non-ASCII→N gotchas (§8); **Perl script is the primary test oracle from Phase A** (§7).
**Branch / worktree:** `rust/genome-preparation` @ `~/Github/Bismark-genomeprep` (off `rust/iron-chancellor` @ `3715703`)
**Perl source of truth:** `bismark_genome_preparation` (repo root, 848 lines, `$bismark_version = 'v0.25.1'`, `$last_modified = "19 May 2022"`)
**Acceptance gate:** **byte-identical bisulfite-converted FASTA vs the Perl original** (the CT + GA converted files), defined precisely in §7. Index equivalence is a **secondary** check (re-run the same deterministic indexer; do not reproduce index bytes).

---

## 1. Purpose & one-paragraph summary

`bismark_genome_preparation` turns a reference genome into the two in-silico bisulfite-converted references that Bismark aligns against. For every input FASTA sequence it writes (a) a **C→T converted** copy — the top/forward-strand index — and (b) a **G→A converted** copy — the bottom/reverse-strand index — under `<genome>/Bisulfite_Genome/CT_conversion/` and `GA_conversion/`. It then **forks two processes** to run an external indexer (`bowtie2-build` by default, or `hisat2-build` / `minimap2 -d`) over each converted reference. The Rust port must reproduce the **converted FASTA files byte-for-byte** (§7); the index build is delegated to the same external indexer (a required subprocess) and validated only for equivalence, not byte-reproduced.

This is a **different shape** from every prior post-alignment port (dedup/extractor/methcons/bedgraph/c2c are all BAM tools). There is **no BAM I/O**, so `bismark-io`'s noodles BAM/SAM/CRAM machinery is **not** the reuse base. The algorithm is trivial (`uc` → map non-`ATCGN` to `N` → `tr/C/T/` and `tr/G/A/`); *all* the difficulty is in faithfully matching Perl's FASTA byte layout — exact line wrapping, header rewriting, file/chromosome ordering, and line-ending handling.

---

## 2. Perl behavior — the contract (derived from source)

### 2.1 Inputs
- **One mandatory positional argument:** the genome folder (`my $genome_folder = shift @ARGV;`, line 88). Missing → `die "Please specify a genome folder to be used for bisulfite conversion"`.
- The folder is `chdir`-ed into and made **absolute** via `getcwd()` with a guaranteed trailing `/` (lines 92–104).
- **FASTA discovery (extension precedence, lines 610–626):** `<*.fa>`; if none, `<*.fa.gz>`; if none, `<*.fasta>`; if none, `<*.fasta.gz>`. **The first non-empty extension group wins — extensions are never mixed.** No FASTA at all → `die`.
- **File ordering:** Perl `glob`/`<...>` returns results **lexically (ASCII) sorted** by default (File::Glob, no `GLOB_NOSORT`). This order defines (a) the chromosome order inside the combined MFA outputs and (b) the comma-joined `file_list` handed to the indexer. **Load-bearing for byte-identity** (see §8.1).
- Each input FASTA may be plain or gzipped; Perl reads gzipped input via a `gunzip -c $filename |` pipe (lines 393–398).

### 2.2 CLI options (`GetOptions`, lines 50–63; help text lines 756–848)

| Perl option | Type / default | Behavior |
|---|---|---|
| `<genome_folder>` | positional, **mandatory** | dies if absent |
| `--bowtie2` | flag | **default ON**; produces `bowtie2-build` indices |
| `--hisat2` | flag | `hisat2-build` indices; mutually exclusive with `--bowtie2` and `--minimap2` (→ `die`) |
| `--minimap2` / `--mm2` | flag | `minimap2 -d` indices; excludes `--bowtie2` (→ `die`), and excludes `--single_fasta`, `--slam`, `--large-index` (→ `die`) |
| `--path_to_aligner <dir>` | string (`:s`) | folder (not executable) prefixed to the indexer binary; validated by `chdir` (lines 589–604) |
| `--parallel <int>` | int (`:i`) | must be **≥2** if given (else `die`); passes `--threads N` (bt2/hisat2) or `-t N` (mm2) to each indexer. "uses `parallel*2` cores in total" |
| `--single_fasta` | flag | per-chromosome output files instead of one combined MFA; not with `--minimap2` (→ `die`) |
| `--slam` | flag | **experimental**; T→C / A→G transitions instead of C→T / G→A; warns + `sleep(3)`; not with `--minimap2` (→ `die`) |
| `--large-index` | flag | forces `--large-index` on the bt2/hisat2 command; not with `--minimap2` (→ `die`) |
| `--genomic_composition` | flag | additionally writes `genomic_nucleotide_frequencies.txt` (see §2.7) |
| `--verbose` | flag | extra STDOUT/STDERR diagnostics |
| `--help` / `--man` | flag | both print the `__DATA__` help block and exit |
| `--version` | flag | prints the version banner (contains `v0.25.1`) and exits |

**Aligner selection (lines 124–151):** `--hisat2` wins if set (errors if combined with bowtie2/mm2); else `--minimap2` (errors if combined with bowtie2); else **bowtie2 is the default**.

### 2.3 Control flow (top-level, lines 184–194)
1. `create_bisulfite_genome_folders()` — **Step I**: resolve aligner path, glob FASTA, build the `Bisulfite_Genome/{CT,GA}_conversion/` tree, return the file list.
2. *(optional)* if `--genomic_composition`: `get_genomic_frequencies()` then **reset** `%chromosomes`.
3. `process_sequence_files()` — **Step II**: the bisulfite conversion (the byte-identity core).
4. `launch_indexer()` — **Step III**: fork + run the external indexer twice.

### 2.4 Step I — `create_bisulfite_genome_folders` (lines 585–663)
- If `--path_to_aligner` given: append trailing `/`, then `chdir` into it to validate (dies on failure).
- `chdir` into the genome folder; glob FASTA with the §2.1 extension precedence.
- Emit (STDERR) `Bisulfite Genome Indexer version v0.25.1 (last modified: 19 May 2022)` and `sleep(1)`.
- `mkdir <genome>/Bisulfite_Genome/` **unless it already exists** — if it exists, print a **warning that existing converted sequences / indices will be overwritten** (no error; overwrite proceeds).
- `mkdir` the `CT_conversion/` and `GA_conversion/` subfolders (each guarded by `unless -d`).
- Returns the globbed `@filenames`.

### 2.5 Step II — `process_sequence_files` (lines 360–516) — **the byte-identity core**
- **MFA mode (default):** open **once** `"$CT_dir/genome_mfa.CT_conversion.fa"` and `"$GA_dir/genome_mfa.GA_conversion.fa"`; every sequence is appended to these two handles.
- **`--single_fasta` mode:** for each chromosome, open `"$CT_dir/<chr>.CT_conversion.fa"` and `"$GA_dir/<chr>.GA_conversion.fa"`.
- For each input file (in glob order):
  1. Open (gunzip pipe if `.gz`).
  2. Read the **first line**, `chomp` (removes a trailing `\n`; a `\r` from CRLF survives the chomp but is consumed by name-splitting).
  3. `extract_chromosome_name` (lines 572–582): **die only if the first byte is not `>`**; otherwise strip the leading `>`, `split /\s+/`, take the **first field**. **Exact Perl semantics (rev-3, both reviewers):**
     - A **bare `>`** first line is **NOT** an error — `s/^>//` succeeds, `split /\s+/, ""` yields no fields, name = `""` → Perl writes a `>_CT_converted` header and proceeds. (The plan must **not** test "bare `>` → error".)
     - A header with **leading whitespace** after `>` (e.g. `>  chr1 desc`) yields an **empty** name: Perl `split /\s+/` keeps the leading empty field, so `($name) = split` assigns `""`. The faithful Rust form is `post_gt.split(<Perl-\s set>).next().unwrap_or("")` — **NOT** `str::split_whitespace()` (which skips leading whitespace → `chr1`, a divergence; the alanhoyle reference gets this wrong).
     - Normal `>chr1 description` → `chr1`. (Mirrors how Bowtie names sequences.)
  4. **Uniqueness check** across *all* files via `%chromosomes`: a duplicate name → `die`.
  5. Write the converted header: `>` + `<chr>` + `_CT_converted\n` (and `_GA_converted\n`). **Always plain `\n`.** Any FASTA-header description after the first token is **dropped.**
     - **SLAM fidelity (load-bearing):** even in `--slam` mode the headers stay **`_CT_converted` / `_GA_converted`** — Perl never changed them (the source carries a literal `### TODO: Change this for GrandSlam` comment at lines 427–429 that was never acted on). Only the *sequence transliteration* changes for slam, **not** the header suffix. (alanhoyle's port emits `_TC_converted`/`_AG_converted` here — a byte-identity divergence we must **not** copy; see §8.)
  6. For each subsequent line:
     - If it starts with `>` (a new in-file header): (verbose: print per-chr stats), reset counters, extract the new name, (single_fasta: open the next pair of files), write the two new converted headers.
     - **Else (a sequence line):**
       ```perl
       my $sequence = uc $_;                  # uppercase; $_ still carries its trailing \n (and \r if CRLF)
       $sequence =~ s/[^ATCGN\n\r]/N/g;       # any byte not in {A,T,C,G,N,\n,\r} → N
       ($CT = $sequence) =~ tr/C/T/;          # CT file: C→T   (slam: tr/T/C/)
       ($GA = $sequence) =~ tr/G/A/;          # GA file: G→A   (slam: tr/A/G/)
       print CT_CONVERT $CT;                  # written verbatim, incl. original wrapping + newline
       print GA_CONVERT $GA;
       ```
- Close both handles; print conversion totals to STDOUT.

### 2.6 Step III — `launch_indexer` (lines 196–357)
- Resolve indexer binary: `bowtie2-build` (default) / `hisat2-build` / `minimap2`, optionally prefixed by `--path_to_aligner`.
- `fork()`:
  - **parent** → `chdir` `CT_conversion/`, re-glob `<*.fa>`, join with commas, run:
    - bt2/hisat2: `<bin> --threads N [--large-index] -f <file_list> BS_CT`
    - mm2: `<bin> -k 20 -t N -d BS_CT.mmi <file_list>`
  - **child** → same for `GA_conversion/` → `BS_GA` / `BS_GA.mmi`.
  - fork-unsupported fallback → run the two builds **sequentially**.
- minimap2 uses a fixed `-k 20` k-mer (reduced-alphabet recommendation, issue #446).
- **Threads (rev-3, Perl-faithful):** Perl defaults `$parallel = 1` and **always** emits the threads flag — `--threads N` (bt2/hisat2) / `-t N` (mm2) — even at **N=1** (lines 114, 251–258). The Rust port does the same: always pass the threads flag, defaulting to 1. (Not byte-gated — the index isn't — but matches Perl's invocation.)
- **Discovery tier (rev-3):** adopt the extractor's `BISMARK_BIN → which → current_exe` resolution for the indexer binary. **Exception:** when `--path_to_aligner` is given, use exactly `{path}/{binary}` and do **NOT** `which`-fallback (an explicit path that's wrong must fail, mirroring Perl's `chdir`-validate).
- **Validation timing (rev-3):** `--path_to_aligner` is validated **early, in Step I (before conversion)** — Perl validates it at line 589 before any FASTA is written, so a bad path must not leave a fully-converted-but-unindexed genome. See §4.7.

### 2.7 Optional — `--genomic_composition` (lines 518–570, 665–751)
- `read_genome_into_memory`: re-globs FASTA (same precedence), **skips the legacy hardcoded `Mus_musculus.NCBIM37.fa`** (line 694), strips `\r`, uppercases, and slurps each chromosome into `%chromosomes` (whole genome in memory).
- `process_sequence`: for each chromosome, counts **mono-** and **di-nucleotide** occurrences, **skipping any k-mer containing `N`**.
- Writes `<genome>/genomic_nucleotide_frequencies.txt` (in the **genome folder**, not `Bisulfite_Genome/`), one `"$key\t$count\n"` per key, **sorted lexically by key** (so `A`, `AA`, `AC`, …, `AT`, `C`, `CA`, … interleave mono/di alphabetically).
- Known historical fragility: issue **#74** (`--genomic_composition` failure).

---

## 3. Reuse map — what comes from the existing workspace

`bismark-genome-preparation` is **largely standalone** — it does *not* depend on `bismark-io` (no BAM/SAM/CRAM). Reuse is at the dependency + convention level only:

| Need | Reuse / source | Notes |
|---|---|---|
| CLI parsing, `--version`, exit codes | clap derive (`=4.5.30`), mirror `bismark-dedup`/`bismark-extractor` `cli.rs` + `main.rs` | Keep Perl's flag spellings (underscores: `--single_fasta`, `--path_to_aligner`, `--large-index`, `--genomic_composition`). |
| gzip **input** (`.fa.gz` / `.fasta.gz`) | `flate2 = "=1.1.9"` `MultiGzDecoder` (already in lock) | Replaces Perl's `gunzip -c` pipe with in-process decompression (multi-member safe). The *indexer* subprocess is separate and still required. |
| Indexer discovery on PATH | `which = "=7.0.3"` (extractor precedent, Phase G) | Locate `bowtie2-build` / `hisat2-build` / `minimap2`; honor `--path_to_aligner` prefix. |
| Indexer invocation | `std::process::Command` | Two builds; concurrency optional (see §4). |
| Errors / diagnostics | `anyhow` + `thiserror`; STDERR logger mirroring `bismark-extractor/src/logging.rs` | `--verbose` toggles extra diagnostics; STDERR text is **not** byte-matched (§4). |
| FASTA parsing | **raw line-streaming, NOT `noodles-fasta`** | See §8.2: noodles normalizes records and discards original line breaks, which would break the line-wrapping byte-identity contract. Stream bytes/lines and transform in place. |
| Workspace wiring | add `bismark-genome-preparation` to `rust/Cargo.toml` `members` | Current members: `bismark-io`, `bismark-dedup`, `bismark-extractor`, `bismark-methylation-consistency`. |

**Crate name:** `bismark-genome-preparation`. **Binary name:** `bismark_genome_preparation_rs` (Perl-name + `_rs`, matching dedup's `deduplicate_bismark_rs`; drop-in for `bismark_genome_preparation`). *(Confirm in review.)*

### 3.1 Prior-art reference — alanhoyle's parallel port
`~/Github/alanhoyle-bismark-rustport` (branch `rust-port`, crate `bismark-genome-prep`, single `src/main.rs`) is an independent Rust port worth reading for structure. It **confirms the external-subprocess model** for all three indexers (`std::process::Command`), runs CT in a spawned thread + GA on the main thread (concurrent, mirroring Perl's `fork`), uses `flate2::MultiGzDecoder` for gzip, and line-streams the conversion. **However it has three byte-identity divergences from Perl that this port must NOT replicate** (each independently re-confirms a §8 gotcha):
1. It does `trim_end_matches(['\n','\r'])` then re-emits `\n` ⇒ **CRLF input loses its `\r`** (Perl preserves it in sequence lines) and a **final line lacking a trailing newline gains one** (Perl preserves its absence).
2. **SLAM header suffix bug:** it writes `_TC_converted` / `_AG_converted`; Perl writes `_CT_converted` / `_GA_converted` even in slam mode (§2.5).
3. It accepts `--genomic_composition` as a flag but **never implements it** (no-op) — we *deliberately* defer it (§9), which is a documented decision, not a silent gap.

### 3.2 rammap (forward note — NOT a v1.0 dependency)
`jwanglab/rammap` (pure-Rust, MIT, v1.0.0; **not** Heng Li — common misattribution) is a minimap2 reimplementation usable as a library. It is **out of scope for this port** and tracked for the *future Rust Bismark aligner*: genome_prep only *produces* an index that the aligner *consumes*, and the index file is the interface between them — so swapping the minimap2 producer to rammap is incoherent without also moving the consumer. Additional blockers for in-tree use today: `save_index` emits rammap's **RMMI** format (not a minimap2 `.mmi`), it's **not on crates.io** (git/path dep), and **macOS/ARM64 requires Rust nightly**. v1.0 therefore keeps `minimap2 -d` as an external subprocess (§2.6, §9). See memory `reference_rammap_rust_minimap`.

This connects to the **optional combined-genome output** this port adds now (§10): the future Rust aligner initiative is expected to consume a single combined CT+GA index (with a Rust mapping engine + native threading), so genome_prep produces that artifact ahead of time. Generating it is trivial here; validating the *alignment* strategy belongs to the aligner rewrite. See memory `project_concatenated_genome_experiment`.

---

## 4. Known divergences from Perl (documented & accepted — for reviewers to accept or challenge)

1. **gzip input via `flate2`, not a `gunzip -c` subprocess.** Pure-Rust decompression; output bytes are identical. (The *indexer* subprocess remains — it is external and not reimplementable.)
2. **STDERR/STDOUT diagnostics** mirror Perl's `warn`/`print` in spirit but are **not** byte-matched; `--verbose` gates extra detail. `sleep(1)`/`sleep(3)` UX pauses are dropped.
3. **`--help` / `--man` / `--version` text** is clap/Rust-generated, not byte-identical to Perl's `__DATA__` block / banner. Not part of the acceptance gate (dedup/methcons precedent). `--man` aliases `--help` (as in Perl).
4. **`Getopt::Long` behaviors not replicated:** `auto_abbrev` (unambiguous prefixes like `--single`), and the `:s`/`:i` "optional value" subtleties. Only the documented flags are accepted; types are enforced by clap.
5. **Indexer concurrency (`fork`)** — *resolved (rev 1): run the two builds CONCURRENTLY*, mirroring Perl's `fork` (CT in a spawned thread + GA on the main thread, or two spawned threads; join both, propagate either failure). Affects wall-time only, **never the converted FASTA** (the gate) nor the produced indices. (alanhoyle uses the thread + main-thread pattern; we follow it.)
6. **Legacy `Mus_musculus.NCBIM37.fa` skip** (genomic-composition path, line 694): preserved only if `--genomic_composition` is in scope (see §9); otherwise N/A.
7. **`--path_to_aligner` validation (rev-3: early, no fallback)**: Perl `chdir`s into it in Step I (line 589) **before** conversion; the Rust port validates the directory **early (Step I, before any FASTA is written)** and resolves the binary within it, and does **NOT** `which`-fallback when an explicit path is given (a wrong explicit path must fail — equivalent to Perl's `chdir`-die). No effect on FASTA output, but the *timing* matters so a bad path fails before work is done.
8. **`Bisulfite_Genome/` overwrite semantics** preserved: if the dir exists, warn and overwrite (no error); converted files are truncated and rewritten.
9. **Hardcoded version string** in the Step I banner (`v0.25.1`, `19 May 2022`) and the version banner: reproduced in **diagnostic text only** (not gated), using a single Bismark-version constant. Not injected into any FASTA bytes (FASTA carries no version).

---

## 5. Output contract — exact bytes

### 5.1 Directory tree (created under the genome folder)
```
<genome>/Bisulfite_Genome/
├── CT_conversion/
│   ├── genome_mfa.CT_conversion.fa        # MFA (default)   — OR  <chr>.CT_conversion.fa per chr (--single_fasta)
│   └── BS_CT.*                            # indexer output (secondary; bt2/hisat2)  — OR BS_CT.mmi (mm2)
└── GA_conversion/
    ├── genome_mfa.GA_conversion.fa        # MFA (default)   — OR  <chr>.GA_conversion.fa per chr (--single_fasta)
    └── BS_GA.*                            # indexer output (secondary)              — OR BS_GA.mmi (mm2)
```
*(plus, only with `--genomic_composition`: `<genome>/genomic_nucleotide_frequencies.txt`.)*

### 5.2 Converted FASTA bytes (the HARD gate)
For each input sequence, the output is, in order:
1. A header line `>` + `<chr>` + `_CT_converted` + `\n` (GA: `_GA_converted`). `<chr>` = first whitespace-delimited token after `>`; the original header description is dropped; **always LF**. **This suffix is fixed regardless of `--slam`** (§2.5).
2. The sequence lines copied **verbatim except** for the in-place transform: `uc` → `s/[^ATCGN\n\r]/N/g` → `tr/C/T/` (CT) or `tr/G/A/` (GA) — i.e. **exact original line wrapping and trailing newline preserved** (including a final line with no trailing `\n`, and `\r` within sequence lines for CRLF input).
   - **Implementation approach (preserves bytes for free):** read each sequence line as **raw bytes including its terminator** (e.g. `read_until(b'\n')`), then transform byte-wise: `u = b.to_ascii_uppercase(); keep u if u ∈ {A,T,C,G,N,\r,\n} else N; then C→T (CT) / G→A (GA)`. Because `\r` and `\n` are in the keep-set and the line is never re-terminated, CRLF stays CRLF and a final line without `\n` keeps none. **Do NOT** `trim` the line ending and re-emit `\n` (that is alanhoyle's divergence #1, §3.1/§8).

**MFA mode:** all sequences concatenated into the single `genome_mfa.{CT,GA}_conversion.fa` in **glob order × in-file header order**.
**`--single_fasta` mode:** one file per chromosome, `<chr>.{CT,GA}_conversion.fa`.

### 5.3 `genomic_nucleotide_frequencies.txt` (only if `--genomic_composition` in scope)
`"$key\t$count\n"` per key, **sorted lexically by key**, N-containing k-mers excluded. Byte-identity target if the feature ships (see §9).

### 5.4 Index files (SECONDARY — not byte-gated)
`BS_CT.*`/`BS_GA.*` (bt2/hisat2) or `BS_CT.mmi`/`BS_GA.mmi` (mm2) are produced by the external indexer. The indexer is deterministic given the same version + identical input FASTA, so the contract is **"the same indexer, run on the byte-identical converted FASTA, builds successfully and equivalently"** — not byte-for-byte index reproduction (§7).

---

## 6. CLI surface (clap derive)

Keep Perl's flag spellings exactly (underscores) for drop-in compatibility.
```
bismark_genome_preparation_rs [OPTIONS] <GENOME_FOLDER>

<GENOME_FOLDER>            path to the folder containing the genome FASTA(s) (required)
    --bowtie2             build Bowtie 2 indices (default ON)
    --hisat2              build HISAT2 indices (conflicts with --bowtie2/--minimap2)
    --minimap2 / --mm2    build minimap2 indices (conflicts with --bowtie2; excludes --single_fasta/--slam/--large-index)
    --path_to_aligner <D> folder containing the indexer binary (not the executable itself)
    --parallel <N>        threads per indexing process; N ≥ 2 (uses 2N cores total)
    --single_fasta        per-chromosome output files instead of a combined MFA
    --slam                DEPRECATED (slated for removal): T→C / A→G instead of C→T / G→A
    --large-index         force a large index (bt2/hisat2)
    --genomic_composition write genomic_nucleotide_frequencies.txt
    --combined_genome     ALSO build a single combined CT+GA reference + index (Bismark-Rust extension; default OFF; §10)
    --verbose             extra diagnostics
-V, --version             print version and exit
-h, --help / --man        print help and exit
```
### 6.1 Validation (mirror Perl → same error → nonzero exit)
- Conflicting aligners (`--bowtie2`+`--hisat2`, `--bowtie2`+`--minimap2`, `--hisat2`+`--minimap2`) → error.
- `--minimap2` with any of `--single_fasta` / `--slam` / `--large-index` → error.
- `--parallel` given but `< 2` → error.
- Missing `<genome_folder>` → error with the Perl usage hint.
- Genome folder contains no FASTA (`.fa`/`.fa.gz`/`.fasta`/`.fasta.gz`) → error.
- Duplicate chromosome name across inputs → error.
### 6.2 `--version`: `version_string()` from lib.rs using `env!("CARGO_PKG_VERSION")` (dedup precedent); the Bismark `v0.25.1` constant appears only in diagnostic banner text, never in FASTA bytes.
### 6.3 `--slam` deprecation: when `--slam` is given, emit a STDERR deprecation warning (e.g. *"`--slam` is deprecated and slated for removal in a future release"*) in addition to Perl's existing experimental warning. This is diagnostic text only (not gated); the slam conversion + the fixed `_CT_converted`/`_GA_converted` headers (§2.5/§5.2) remain byte-identical to Perl.
### 6.4 `--combined_genome` (Bismark-Rust extension; default OFF): *additive* — the standard CT/GA outputs and their two indices are produced exactly as today; this flag **adds** the combined reference + combined index (§10). Composes with any indexer (`--bowtie2`/`--hisat2`/`--minimap2`) and with `--slam`. It is **independent of `--single_fasta`** (the combined output is always a single MFA + single index, even when the standard outputs are per-chromosome). No new validation errors are introduced.

---

## 7. Acceptance / definition of "byte-identical output"

**HARD gate (must be byte-for-byte identical to Perl Bismark v0.25.1):**
1. `Bisulfite_Genome/CT_conversion/genome_mfa.CT_conversion.fa` — byte-for-byte.
2. `Bisulfite_Genome/GA_conversion/genome_mfa.GA_conversion.fa` — byte-for-byte.
3. In `--single_fasta` mode: every `<chr>.CT_conversion.fa` / `<chr>.GA_conversion.fa` — byte-for-byte, and the **set of files** matches.
4. If `--genomic_composition` ships: `genomic_nucleotide_frequencies.txt` — byte-for-byte.
5. The directory structure (`Bisulfite_Genome/{CT,GA}_conversion/`) matches.

**SECONDARY (validated, not byte-gated):**
6. Re-running the **same** indexer version on the byte-identical converted FASTA builds successfully; index equivalence is checked by re-build determinism (and, where practical, by a functional check), **not** by diffing index bytes.

**Test oracle (rev-3, both reviewers):** the **Perl script is the primary oracle from Phase A onward**, not hand-authored fixtures — mirror methcons's `perl_vs_rust_*` pattern (run the actual `bismark_genome_preparation` on the same synthetic input and `diff` the CT/GA FASTA; **auto-skip if `perl` is absent**). Hand-authored expected fixtures are a *secondary* convenience and would not, on their own, have caught the bare-`>` / leading-whitespace name-extraction divergences (§2.5). Where feasible, also keep a few hand-checked fixtures for the subtle edges (CRLF, final-no-newline, empty record).

**NOT in the gate:** index file bytes; STDERR/STDOUT diagnostics; `--help`/`--version` text; subprocess timing/concurrency.

**Real-data validation (later, on `oxy`, `#[ignore]`):** run Perl `bismark_genome_preparation` and `bismark_genome_preparation_rs` on **copies** of the same genome dir (each writes its own `Bisulfite_Genome/` next to the input) and `diff` the converted CT/GA FASTA byte-for-byte. Gate on the (fast) conversion; validate the (slow, possibly hours) index build separately. **Verify oxy's env on arrival** (genome data path, mamba env, `~/.cargo/bin`, `bowtie2-build`/`hisat2-build` availability) — oxy is a distinct host. Use a fresh work dir; ask before destructive ops.

---

## 8. Gotchas & candidate spikes (call-outs)

1. **Glob sort order parity (load-bearing + PLATFORM-DEPENDENT).** Perl `<*.fa>` (`File::Glob`) returns a sorted list; this fixes the MFA concatenation order and the indexer `file_list`. **RESOLVED (rev-3 → rev-4): byte-identity to "Perl" here is platform-dependent for mixed-case names, and the target is Linux = BYTEWISE.** `File::Glob` sets `GLOB_NOCASE` only on Windows/VMS/OS2/DOS/RISCOS (File::Glob.pm:69) — *not* darwin/linux — so Perl never requests case-folding on either. On **Linux/glibc** `glob(3)` sorts **bytewise** (C locale); on **macOS** Darwin's libc `GLOB_CSH` path case-*folds* as a quirk (empirically: macOS `glob<>`→`aa,ab,Ba,ZZ` but `bsd_glob(0)`→bytewise `Ba,ZZ,aa,ab`). Bismark deploys on Linux (clusters; oxy/colossal validation; Linux CI), so the contract is **Linux-Perl = bytewise**: `discovery::fasta_name_cmp` sorts on the **`file_name()` bytes bytewise** (not the full `PathBuf`). Still digit-vs-letter lexical (`chr1, chr10, chr2`). The `perl_vs_rust_mixed_case_glob_order` oracle is **`#[cfg(target_os="linux")]`-gated** (on macOS the system Perl folds, so the oracle would mismatch there — it confirms parity on the deployment target + CI). Real genomes use all-lowercase `chrN.fa`, where bytewise == fold, so the platform split never bites them. **History:** the original SPEC said bytewise (correct for Linux); a rev-3 "fix" flipped it to case-insensitive based on a *macOS-only* code-review observation; rev-4 reverted to bytewise after tracing the libc/platform root cause (`CODE_REVIEW_A2.md`).
2. **Use raw line-streaming, not `noodles-fasta`.** The noodles reader concatenates a record's sequence and discards original line breaks; reconstructing arbitrary per-file wrapping to regain byte-identity would be fragile. Stream lines and transform in place → wrapping preserved for free. (Settled recommendation; confirm in plan-review.)
3. **Line-ending asymmetry.** Converted **headers always use `\n`** (even for CRLF input); **sequence lines preserve their original ending**, including `\r` (CRLF stays CRLF, because `\r` is in the allowed char class). A CRLF input therefore yields LF headers + CRLF sequence lines — faithfully reproduce this.
4. **Whitespace inside a sequence line → `N`.** A stray space/tab is not in `[ATCGN\n\r]`, so it becomes `N` (then possibly `tr`-converted). Surprising but contractual.
5. **IUPAC ambiguity codes → `N`** (R,Y,S,W,K,M,B,D,H,V), after `uc`; lowercase is uppercased first.
6. **Final line without trailing newline** must be preserved exactly (affects the file's last byte).
7. **Header rewriting drops the description** — only the first `\s`-delimited field survives, suffixed `_CT_converted` / `_GA_converted`. **Edge (rev-3, §2.5):** a bare `>` or a `>`-then-leading-whitespace header → **empty** name → `>_CT_converted`; faithfully reproduce via a split that keeps the leading empty field (not `split_whitespace()`).
8. **`tr` is per-line and counts conversions** in Perl (used only for verbose stats) — the Rust port need not surface counts, but the byte transform order (`uc` → `N`-map → `tr`) must match exactly.
9. **Empty / pathological inputs:** empty sequence lines (`\n`) pass through unchanged; a file whose first byte **is not** `>` → die (but a **bare `>`** does NOT die — §2.5/§8.7); an empty genome dir → die; duplicate chromosome names → die; a **zero-sequence record** (header immediately followed by another header, or header at EOF) → emits just the converted header(s), no sequence — **a real Perl path; cover in Phase A** (rev-3); a **0-byte FASTA file** → cover.
15. **CR-only (old-Mac) line endings (rev-3).** A file using bare `\r` (no `\n`) is read by Perl as a **single line** (Perl reads on `\n`), so the whole content becomes one "header" line → only a header is emitted. Happens to agree with a `read_until(b'\n')` Rust impl, but add a fixture documenting it.
16. **Non-ASCII / high bytes → `N` (rev-3, confirmed).** Any byte not in `{A,T,C,G,N,\r,\n}` after uppercasing maps to `N` — including high/non-ASCII bytes. Reviewer B verified Rust agrees with Perl here; add a fixture to lock it.
10. **`--genomic_composition` format details** (if in scope): sorted-by-key, N-skipping, the legacy `Mus_musculus.NCBIM37.fa` skip, and the historical bug #74. **Candidate Spike B** (format byte-validation against a real Perl run).
11. **gzip multi-member safety:** use `MultiGzDecoder` (some genome `.gz` files are multi-member); a plain single-member decoder can truncate.
12. **Large genomes (human ~3 GB):** stream, never slurp, for the conversion path. (The `--genomic_composition` path *does* slurp in Perl — a memory consideration if that feature ships.)
13. **SLAM header suffix stays `_CT_converted`/`_GA_converted`** (not `_TC_`/`_AG_`) — Perl never changed it (`### TODO: Change this for GrandSlam`, lines 427–429). Only the transliteration differs in slam mode. This is precisely where alanhoyle's port diverges (§3.1) → pin it with a slam-mode byte-identity unit test.
14. **Raw-line byte transform, not trim-and-re-emit.** The transform must operate on raw line bytes *including* the terminator so CRLF/`\r` and final-no-newline survive (§5.2). The naive `read_line` → `trim_end_matches(['\n','\r'])` → write + `\n` pattern silently diverges on both (alanhoyle divergence #1) → cover with CRLF-input and no-trailing-newline fixtures.

---

## 9. Scope for v1.0 (resolved — rev 1, 2026-05-30)

The byte-identity gate is the **converted FASTA**, which is **identical regardless of which indexer** is selected — so FASTA correctness is fully exercised by the default bowtie2 path, and adding other indexers is "emit a different subprocess command." Final scope:

| Feature | Cost | Risk to byte-identity | Verdict |
|---|---|---|---|
| Core: genome arg, glob+precedence, `Bisulfite_Genome/{CT,GA}` tree, MFA, C→T/G→A conversion, gzip input | — | the whole gate | **v1.0 (mandatory)** |
| `--single_fasta` | low (output filenames + per-chr handles; same conversion) | low | **v1.0** |
| `bowtie2-build` indexer + `--path_to_aligner` + `--parallel` + `--large-index` | low–med (subprocess wiring) | none (FASTA is the gate) | **v1.0** |
| `--hisat2` indexer | low (different command string; same FASTA) | none | **v1.0** — heavy index-equivalence validation gated on bowtie2 |
| `--minimap2` indexer (external subprocess) | med (different cmd + `.mmi` names + extra exclusions; same FASTA) | none | **v1.0 — KEPT as external `minimap2 -d`** (rammap deferred to the aligner layer; §3.2) |
| `--slam` (T→C / A→G) | low (conversion-direction toggle; headers stay `_CT_`/`_GA_`) | a **separate** byte-identity target (slam-converted genome) | **v1.0 — INCLUDED but marked DEPRECATED** (§6.3) |
| `--genomic_composition` | med (whole-genome slurp + freq counting + sorted output + legacy skip + a 2nd byte-target) | orthogonal new output; historical bug #74 | **DEFERRED to a follow-up** (mirrors c2c deferring niche features) — accepted-and-ignored with a one-line note in v1.0 |
| `--combined_genome` (Bismark-Rust extension; §10) | low (concatenate CT+GA → 1 FASTA; 1 extra index build) | none — additive, no Perl counterpart, not byte-gated | **v1.0 — INCLUDED, opt-in (default OFF)**; alignment-correctness validation deferred to the aligner rewrite |

### Resolved decisions (rev 1–2)
1. **`--minimap2`:** **kept in v1.0** as an external `minimap2 -d` subprocess (all three indexers external). rammap is an aligner-layer choice and is deferred — see §3.2 / memory `reference_rammap_rust_minimap`. *Follow-up:* file a forward-looking issue "adopt rammap as the Rust minimap2 engine" scoped to the future Rust Bismark aligner.
2. **`--slam`:** **included in v1.0**, marked deprecated (STDERR warning; §6.3), with the fixed `_CT_converted`/`_GA_converted` headers (§2.5) — covered by a slam byte-identity test.
3. **Indexer concurrency:** **concurrent** (mirror Perl's fork; §4.5).
4. **`--genomic_composition`:** **deferred** to a follow-up; v1.0 accepts the flag and ignores it with a one-line note (and does **not** silently claim coverage). *Follow-up:* its own sub-issue.
5. **`--combined_genome` (rev 2):** **included in v1.0** as an *additive, opt-in* Bismark-Rust extension (default OFF; §10) — produces a single combined CT+GA reference FASTA **and** a combined index for the upcoming aligner rewrite to consume. Alignment-correctness validation is deferred to that rewrite; here it is generated and structurally checked only.

### Out of scope for v1.0 (regardless)
- Importing **rammap** / any in-process aligner engine (aligner-layer work; §3.2).
- Reproducing index **bytes** (secondary check only; §7).
- Byte-matching STDERR/STDOUT, `--help`/`--version` text.
- `Getopt::Long` `auto_abbrev`.
- `mimalloc`/threading micro-optimizations beyond `--parallel` passthrough.

---

## 10. Bismark-Rust extension — optional combined genome (`--combined_genome`)

**Status:** in v1.0, *additive*, opt-in (default OFF). Not present in Perl Bismark. Generated here so the upcoming Rust-aligner work can consume a single combined reference + index; the *alignment* strategy (and its concordance/ambiguity validation) belongs to that rewrite, **not** this port. (Design rationale tracked in internal memory; committed/public artifacts name no external tools.)

### 10.1 Behavior (additive — current behavior untouched)
When `--combined_genome` is given, after the standard Step II conversion + Step III indexing of the two split references, the port **additionally**:
1. Writes a single **combined reference FASTA** = the C→T-converted sequences (all `*_CT_converted`, in glob order) **followed by** the G→A-converted sequences (all `*_GA_converted`, in glob order). Sequence names are already unique (`_CT_converted` vs `_GA_converted`) — **no extra prefixing needed**. Always a single MFA, **independent of `--single_fasta`**.
   - **Byte source (rev-3, both reviewers):** the combined FASTA is built **directly from the converted sequence stream** (run the conversion into the combined writer: all CT records, then all GA records), **NOT** by concatenating the standard `genome_mfa.*` files — those do **not exist** in `--single_fasta` mode. In MFA mode the result is byte-equal to `cat genome_mfa.CT_conversion.fa genome_mfa.GA_conversion.fa`; in `--single_fasta` mode it is the same bytes assembled from the per-chromosome conversions. This makes the §10.4 oracle well-defined in **both** modes.
2. Builds **one combined index** over it with the selected indexer (`bowtie2-build` / `hisat2-build` / `minimap2 -d`), reusing the §2.6 machinery.

Without the flag, output is byte-for-byte the current Bismark layout (the §7 gate is unaffected).

### 10.2 Output location & names (proposed; confirm at review)
```
<genome>/Bisulfite_Genome/Combined/
├── genome_mfa.combined.fa     # CT block ++ GA block (glob order)
└── BS_combined.*              # combined index (bt2/hisat2)   — OR  BS_combined.mmi (mm2)
```

### 10.3 Notes
- **Large index is automatic for mammalian genomes.** A combined human reference (~6.2 Gbp) exceeds Bowtie2's 4 Gbp small-index ceiling, so `bowtie2-build` auto-promotes to the large (`.bt2l`) format — no need to force `--large-index`. (`--large-index` still forces large for smaller references, as for the split indices.)
- **`--slam` composes:** the combined reference uses the slam-converted sequences, with the same fixed `_CT_converted`/`_GA_converted` headers (§2.5).
- **Concurrency:** the combined index build is an additional indexing job; it may run after or alongside the standard two (output-equivalent either way; §4.5).
- **Strand-restricted search, ambiguity behavior, and methylation concordance are out of scope here** — those are aligner-rewrite concerns evaluated when the combined index is actually used for mapping.

### 10.4 Acceptance for the combined output (NOT byte-identity vs Perl — there is no Perl counterpart)
- **Structural check (mode-independent, rev-3):** `genome_mfa.combined.fa` bytes == (all CT-converted records) ++ (all GA-converted records), where each record's bytes equal what the gated §5.2 conversion produces. Practically: build an **expected** combined buffer from the same converted sequence stream and assert byte-equality. **In MFA mode** this is additionally checked as `genome_mfa.CT_conversion.fa` ++ `genome_mfa.GA_conversion.fa`; **in `--single_fasta` mode** it is checked against the freshly-assembled expected buffer (the MFA files don't exist). The combined output thus inherits correctness from the already-gated conversion in both modes.
- The combined index **builds successfully** with the selected indexer (secondary check, like §7.6).
- No alignment/methylation assertions here (deferred to the aligner rewrite).

---

## 11. Next steps (workflow)
1. **Manual review of this SPEC rev 2** (you) — §9 decisions + the §10 combined-genome extension are resolved; confirm or adjust, and raise any remaining ideas.
2. Coordinate a new GitHub **`epic(genomeprep)`** + real sub-issues (spec/impl/test/docs), plus the forward-looking follow-up issues (rammap-aligner; `--genomic_composition`) — *after* scope is confirmed, so the breakdown matches.
3. **Phased implementation PLAN** (mirror methcons `PLAN.md`), then **dual plan-review**.
4. Implement **only on an explicit trigger** (`implement` / `/code-implementation`).
5. **Dual code-review + plan-manager coverage audit**, then real-data byte-identity on `oxy`, docs/CHANGELOG, PR.
