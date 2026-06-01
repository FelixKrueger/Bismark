# bismark-report

A Rust port of the Perl `bismark2report` — the Bismark per-sample **graphical HTML report** generator.

It reads a Bismark **alignment** report (mandatory) plus up to four optional companion reports — **deduplication**, methylation-extractor **splitting**, **M-bias**, and **nucleotide-coverage** — and fills a single self-contained HTML template (with the ~3 MB plotly.js library and two logos inlined) to produce one graphical report per sample.

* **Binary:** `bismark2report_rs` (drop-in for `bismark2report`)
* **No BAM I/O** — does not depend on `bismark-io`.
* **Acceptance gate:** the generated HTML is **byte-for-byte identical** to Perl Bismark v0.25.1, modulo the single `localtime` timestamp line.

## Usage

```
bismark2report_rs [OPTIONS]

    --alignment_report <FILE>    Bismark alignment report (mandatory data). If omitted,
                                 auto-detect *E_report.txt in the current directory
                                 (one HTML per match).
    --dedup_report <FILE>        Deduplication report; "none" to skip; auto-detect if omitted.
    --splitting_report <FILE>    Methylation-extractor splitting report; "none" to skip; auto-detect.
    --mbias_report <FILE>        M-bias report; "none" to skip; auto-detect.
    --nucleotide_report <FILE>   Nucleotide-coverage report; "none" to skip; auto-detect.
    --dir <DIR>                  Output directory (default: current directory).
-o, --output <FILE>              Output filename (single alignment report only).
    --verbose                    Extra diagnostics.
-V, --version                    Print version and exit.
-h, --help / --man               Print help and exit.
```

If no `--alignment_report` is given, every `*E_report.txt` in the current directory produces its own HTML (named `<report>.html`), and the matching companion reports are auto-detected by basename.

## Design notes

* It is mechanically a parser + a string-substitution templating engine — there is essentially **no numeric reformatting**: values are injected verbatim (only a `%`-strip, a `\s.*`-trim on the dedup counts, and one integer subtraction for the dedup "leftover" fallback).
* The four `plotly/` assets are embedded with `include_str!` (manifest-relative), then line-normalized exactly like Perl's `read_report_template` (strip all `\r`, re-append `\n`).
* The document is assembled as raw bytes (`Vec<u8>`) so report-derived values such as `{{filename}}` round-trip byte-for-byte even if non-UTF8.
* The only non-determinism is the timestamp line; `--__test_timestamp <UNIX_EPOCH>` (hidden, UTC) pins it for byte-stable golden tests, while the default uses local time.

## Testing

```
cargo test -p bismark-report
```

* Unit tests cover each parser, the template helpers, the asset normalizer, and the timestamp math.
* `tests/perl_vs_rust.rs` is the **byte-identity gate**: it runs the live Perl `bismark2report` and the Rust binary over the fixtures in `tests/fixtures/`, normalizes the timestamp line, and asserts byte equality. It auto-skips if `perl` is unavailable.
