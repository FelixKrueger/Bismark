//! Phase B byte-identity golden tests + edge integration.
//!
//! Goldens in `tests/data/nome_filtering/phase_b/*.golden` are the **decompressed** output of
//! the repo's Perl `NOMe_filtering` v0.25.1 (`tests/data/nome_filtering/phase_b/generate_goldens.sh`).
//! The Rust binary's `.manOwar.txt.gz` output, once decompressed, must be
//! raw-byte-identical to each golden (the gzip container is impl-dependent, so
//! comparison is post-decompression — SPEC §6 / pitfall P8).

use std::io::Read;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use bismark::nome_filtering::filename::derive_manowar_name;
use flate2::read::MultiGzDecoder;

const HEADER: &[u8] = b"ReadID\tChr\tStart\tEnd\tmeth_CG\tunmeth_CG\tmeth_GC\tunmeth_GC\n";

fn data() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/nome_filtering/phase_b")
}

fn gunzip(p: &Path) -> Vec<u8> {
    let mut d = MultiGzDecoder::new(std::fs::File::open(p).unwrap());
    let mut v = Vec::new();
    d.read_to_end(&mut v).unwrap();
    v
}

/// Copy a committed yacht fixture into a fresh tempdir, run the binary with that
/// dir as `--dir`, and return the DECOMPRESSED output bytes.
fn run_case(yacht: &str) -> Vec<u8> {
    let d = data();
    let tmp = tempfile::tempdir().unwrap();
    std::fs::copy(d.join(yacht), tmp.path().join(yacht)).unwrap();
    Command::cargo_bin("NOMe_filtering")
        .unwrap()
        .arg("-g")
        .arg(d.join("genome"))
        .arg("--dir")
        .arg(tmp.path())
        .arg(yacht)
        .assert()
        .success();
    gunzip(&tmp.path().join(derive_manowar_name(yacht)))
}

#[test]
fn golden_main_multi_context() {
    // chr1 read exercises ACG-CpG (accept), TCG-CpG (accept), GCG-CpG (reject),
    // GpC-CHG, GpC-CHH, and a trailing ACG-CpG → meth_CG=1, unmeth_CG=2,
    // meth_GC=1, unmeth_GC=1 (Perl-verified).
    assert_eq!(
        run_case("main.yacht.txt"),
        std::fs::read(data().join("main.golden")).unwrap()
    );
}

#[test]
fn golden_ncontext() {
    // chr2 read over CNG/CNN (N-context) positions: only the TCG CpG counts.
    assert_eq!(
        run_case("ncontext.yacht.txt"),
        std::fs::read(data().join("ncontext.golden")).unwrap()
    );
}

#[test]
fn golden_edge_asymmetry() {
    // forward start≤3 → NO line; reverse end∈{1,2} → all-zero line (P1).
    assert_eq!(
        run_case("edge.yacht.txt"),
        std::fs::read(data().join("edge.golden")).unwrap()
    );
}

#[test]
fn golden_gz_input_matches_plain() {
    // A gzipped yacht input decompresses to the same output as the plain input.
    assert_eq!(
        run_case("main.yacht.txt.gz"),
        std::fs::read(data().join("main.golden")).unwrap()
    );
}

#[test]
fn vs_empty_leaves_header_only_gz_and_exits_nonzero() {
    // D4/P11: empty input → exit 1, but a header-only `.gz` lands on disk.
    let d = data();
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("empty.yacht.txt"), "").unwrap();
    Command::cargo_bin("NOMe_filtering")
        .unwrap()
        .arg("-g")
        .arg(d.join("genome"))
        .arg("--dir")
        .arg(tmp.path())
        .arg("empty.yacht.txt")
        .assert()
        .failure()
        .code(1);
    let got = gunzip(&tmp.path().join("empty.yacht.manOwar.txt.gz"));
    assert_eq!(got, std::fs::read(d.join("empty.golden")).unwrap());
    assert_eq!(got, HEADER); // the golden IS exactly the header
}

#[test]
fn vs_crlf_input_matches_lf_golden() {
    // Perl `chomp` leaves `\r` on the unused col-8; Rust `lines()` strips `\r\n`.
    // Both yield the same output → a CRLF copy of main must equal main.golden.
    let d = data();
    let lf = std::fs::read_to_string(d.join("main.yacht.txt")).unwrap();
    let crlf = lf.replace('\n', "\r\n");
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("crlf.yacht.txt"), crlf).unwrap();
    Command::cargo_bin("NOMe_filtering")
        .unwrap()
        .arg("-g")
        .arg(d.join("genome"))
        .arg("--dir")
        .arg(tmp.path())
        .arg("crlf.yacht.txt")
        .assert()
        .success();
    let got = gunzip(&tmp.path().join("crlf.yacht.manOwar.txt.gz"));
    assert_eq!(got, std::fs::read(d.join("main.golden")).unwrap());
}

#[test]
fn unknown_chromosome_read_yields_header_only_no_data_line() {
    // A read on a chr absent from the genome → no data line (guard fails on
    // chr_len 0), but `last_read` WAS defined → exit 0 with a header-only
    // report (distinct from the empty-input `EmptyInput` path).
    let d = data();
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("u.yacht.txt"),
        "rZ\t+\tchrZ\t6\tz\t4\t12\t+\n",
    )
    .unwrap();
    Command::cargo_bin("NOMe_filtering")
        .unwrap()
        .arg("-g")
        .arg(d.join("genome"))
        .arg("--dir")
        .arg(tmp.path())
        .arg("u.yacht.txt")
        .assert()
        .success();
    let got = gunzip(&tmp.path().join("u.yacht.manOwar.txt.gz"));
    assert_eq!(got, HEADER);
    assert!(!String::from_utf8_lossy(&got).contains("chrZ"));
}

#[test]
fn golden_reverse_strand_counts_g_strand_call() {
    // A reverse read (col6 > col7) that is suitable and whose calls land on a
    // forward-C (pos9, TCG → unmeth_CG) AND a reverse-G (pos10, ACG → meth_CG):
    // both count, and Start/End are ascending (8,10). Locks the G-strand tally
    // path (code-review L2) against live Perl.
    assert_eq!(
        run_case("rev.yacht.txt"),
        std::fs::read(data().join("rev.golden")).unwrap()
    );
}

#[test]
fn golden_multichromosome_emission_order_not_sorted() {
    // chr2 read THEN chr1 read in the input → emitted in that order, NOT
    // chr-sorted (code-review L2). The golden encodes ra(chr2) before rb(chr1).
    let got = run_case("multichr.yacht.txt");
    assert_eq!(got, std::fs::read(data().join("multichr.golden")).unwrap());
    let s = String::from_utf8(got).unwrap();
    let ra = s.find("ra\tchr2").unwrap();
    let rb = s.find("rb\tchr1").unwrap();
    assert!(
        ra < rb,
        "emission must follow input read order (chr2 before chr1):\n{s}"
    );
}
