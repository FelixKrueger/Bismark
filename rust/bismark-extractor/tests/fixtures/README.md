# Phase G test fixtures

Shell-script fakes for `bismark2bedGraph` / `coverage2cytosine` used by
`tests/phase_g_realrunner.rs`. Each fixture exercises a specific
`RealRunner` behaviour without depending on a real Bismark install on
PATH.

| Fixture | What it does |
|---------|--------------|
| `fake_bismark2bedgraph_success.sh` | Writes a single line to stderr; exits 0. |
| `fake_bismark2bedgraph_failure.sh` | Writes an error line to stderr; exits 7. |
| `fake_bismark2bedgraph_high_stderr.sh` | Emits ~1 MiB to stderr then exits 0 — exercises ring-buffer eviction. |
| `fake_bismark2bedgraph_burst_then_exit.sh` | Writes a 128 KiB stderr burst then fails — guards against pipe-buffer deadlock (rev 1 I6). |
| `fake_bismark2bedgraph_non_utf8_stderr.sh` | Writes non-UTF-8 bytes to stderr; the drain thread must NOT panic (rev 1 C5). |

## Argv-parity goldens

`tests/phase_g_argv_parity.rs` checks the argv builder output against
inline expected lists rather than checked-in golden files. The Perl
print-and-exit shim for regenerating those expectations (in case the
extractor's flag-push order ever needs auditing against a live Perl
install) is:

```bash
# In a one-shot scratch tree (not committed):
perl -i -pe 's/^(\s*)system \(/$1print join(" ", "\$RealBin\/bismark2bedGraph", \@args), "\n"; exit 0; system (/' \
    bismark_methylation_extractor
# Then run with the canonical configs (default, --gzip, --cytosine_report --CX)
# and capture stdout as the expected argv.
```

The Rust tests in `phase_g_argv_parity.rs` then compare
`build_bismark2bedgraph_argv` / `build_coverage2cytosine_argv` outputs
against the captured expected argv lists (modulo long-form vs
prefix-abbreviated flag names — Perl's `--remove` becomes Rust's
`--remove_spaces`; documented in SPEC §6.6 and Phase G plan §2.4.4).
