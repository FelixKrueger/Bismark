//! Function-level micro-benchmark for `SamRecord::parse` (06222026 perf epic).
//!
//! The parse byte-split's win is real but below the end-to-end noise floor
//! (the aligner is ~90% bowtie2-bound under `-p`; GATE_00). This bench isolates
//! it: it compares the current byte-scan field split against the pre-epic
//! `str::split('\t')` (`CharSearcher`, per-char decode), and times the full
//! `SamRecord::parse`, on a representative bismark/bowtie2 PE output line.
//!
//! Run: `cargo bench -p bismark-aligner --bench parse_bench`.

use criterion::{Criterion, black_box, criterion_group, criterion_main};

use bismark_aligner::align::SamRecord;

/// A representative directional PE mate-1 line: qname with `/1`, FLAG 99,
/// CT-converted RNAME, 50M CIGAR, 50bp SEQ/QUAL, and the AS/XS/MD/… tag tail.
const SAM_LINE: &str = "SRR24766921.1_1/1\t99\t1_CT_converted\t3010512\t40\t50M\t=\t3010700\t238\t\
TGGTTGATTTGGTAGTAGTAGTTGGAGTTGGTTTAGTAGTTGGAGTAGTT\t\
IIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIII\t\
AS:i:0\tXS:i:-12\tXN:i:0\tXM:i:0\tXO:i:0\tXG:i:0\tNM:i:0\tMD:Z:50\tYS:i:-6\tYT:Z:CP";

/// Current byte-scan field split (what `SamRecord::parse` now does internally).
fn split_byte_scan(line: &str) -> usize {
    let lb = line.as_bytes();
    let mut end = lb.len();
    while end > 0 && (lb[end - 1] == b'\n' || lb[end - 1] == b'\r') {
        end -= 1;
    }
    let trimmed = &line[..end];
    let mut f: Vec<&str> = Vec::with_capacity(16);
    let mut start = 0usize;
    for (i, &b) in trimmed.as_bytes().iter().enumerate() {
        if b == b'\t' {
            f.push(&trimmed[start..i]);
            start = i + 1;
        }
    }
    f.push(&trimmed[start..]);
    black_box(&f);
    f.len()
}

/// Pre-epic char-based field split (`CharSearcher`), for comparison only.
fn split_char_searcher(line: &str) -> usize {
    let trimmed = line.trim_end_matches(['\n', '\r']);
    let f: Vec<&str> = trimmed.split('\t').collect();
    black_box(&f);
    f.len()
}

fn bench_parse(c: &mut Criterion) {
    let mut g = c.benchmark_group("sam_field_split");
    g.bench_function("byte_scan (current)", |b| {
        b.iter(|| split_byte_scan(black_box(SAM_LINE)))
    });
    g.bench_function("char_searcher (pre-epic)", |b| {
        b.iter(|| split_char_searcher(black_box(SAM_LINE)))
    });
    g.finish();

    c.bench_function("SamRecord::parse (full, current)", |b| {
        b.iter(|| SamRecord::parse(black_box(SAM_LINE)).unwrap())
    });
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
