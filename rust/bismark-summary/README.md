# bismark-summary

Rust port of Bismark Perl's `bismark2summary` — the **project-level,
multi-sample** aggregator (distinct from the per-sample `bismark2report`).

It scans a run folder for Bismark BAM files (by filename only — it never opens
a BAM), locates each one's text report files (alignment report, and optionally
the deduplication and methylation-extractor splitting reports), parses
per-sample metrics, and emits one project summary:

- `bismark_summary_report.txt` — a 15-column tab-delimited table, one row per
  sample.
- `bismark_summary_report.html` — a self-contained plot.ly report (stacked-area
  alignment graphs + per-context methylation graphs).

The binary is installed as `bismark2summary_rs`. Output is byte-identical to
Perl Bismark v0.25.1 (the `.txt` fully; the `.html` modulo the single
`localtime` timestamp line).

```
bismark2summary_rs [OPTIONS] [BAM_FILES]...

  -o, --basename <NAME>   Output basename (default: bismark_summary_report)
      --title <STRING>    HTML report title (default: "Bismark Summary Report")
      --verbose           Extra diagnostics
  -V, --version           Print version and exit
  -h, --help / --man      Print help and exit
```

If no BAM files are given, the current directory is scanned for
`*bismark_{bt2,hisat2}[_pe].bam`.

See `plans/06012026_bismark2summary/SPEC.md` for the full design contract.
