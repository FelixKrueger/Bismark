# Public datasets for validating the 5-Base (5mC→T) path (#787)

Searched SRA (NCBI eutils), ENA (portal + free-text), and GEO on 2026-06-23 for the
**target technology (Illumina 5-Base DNA Prep)** and the nearest analogs.

## The target tech: Illumina 5-Base — NO public raw data

- Exact-phrase SRA search `"5-Base DNA Prep"` → **0 records**. ENA/GEO study searches for
  "Illumina 5-base" / "5-base solution" / "5-base methylation" → no true hits (only
  token-match false positives: bisulfite/histone-methylation studies).
- The only known Illumina 5-Base data is the **BaseSpace demo** (NA12878/HG002, NovaSeq X,
  DRAGEN v4.4.6), which is **gated behind a free Illumina account** (BaseSpace CLI + auth
  token), not a plain SRA/ENA download. So the actual target tech cannot be pulled
  programmatically. (Launched 2025-10-15; ~8 months on, still no public deposit.)

## Conversion DIRECTION is what matters for the inverted caller

`--illumina_5base` needs **5mC→T** data (methylated C converts; unmethylated C stays C).
Picking a public surrogate by *direction*:

| Public data | Direction | Applicable to `--illumina_5base`? |
|---|---|---|
| **TAPS** (academic; GSE112520 / SRP136786) | **5mC(+5hmC)→T** | **YES** — same direction; used in `VALIDATION_REAL_DATA.md` |
| Watchmaker **TAPS+** (commercial) | 5mC(+5hmC)→T | YES in principle (no clean public run located) |
| **biomodal duet evoC / "6base"** (commercial; e.g. GSE271401 / SRP517759) | **unmodified C→T** (bisulfite-like; modified C protected) | **NO** — opposite direction → use the standard Bismark bisulfite path, not the 5-Base path |
| WGBS / EM-seq | unmodified C→T | NO (bisulfite path) |

**Key correction:** biomodal evoC **is** public, but its chemistry deaminates the
*unmodified* cytosine (→T) and protects modified C — the **bisulfite direction**, the
OPPOSITE of Illumina 5-Base. It is therefore NOT a surrogate for the inverted 5-Base
caller (the standard bisulfite spine handles it). Only TAPS/TAPS+ share the 5-Base
direction.

## Conclusion

The most representative *publicly downloadable* data for the Illumina 5-Base **conversion
direction** is **TAPS GSE112520** (same E14 mESC sample also has matched WGBS), which is
exactly what the real-data validation uses (`VALIDATION_REAL_DATA.md`). A run on the
*actual* Illumina 5-Base kit awaits either a public deposit or BaseSpace-credentialed
access to the NA12878/HG002 demo.
