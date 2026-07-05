//! THROWAWAY spike harness (#797 follow-up; parallel-parse design) — splits the
//! `bismark2bedGraph_rs` **read phase** into decompress / parse+validate /
//! hashmap-insert, then measures the **merge cost** of folding a per-file map
//! into a fresh global map. Decides per-file-parallel (Family A) vs
//! sharded/intra-file (Family B).
//!
//! Method: staged passes, each doing strictly more work than the last, so
//! subtraction isolates each cost:
//!   P1 decompress-only          -> T_decompress
//!   P2 decompress + parse       -> T_parse  = P2 - P1
//!   P3 decompress + parse + ins -> T_insert = P3 - P2   (fresh per-file map)
//!   P4 merge P3's map -> global -> T_merge  (Family-A reduce step)
//!
//! Faithful to the real path: reuses `bismark_bedgraph::validate::validate_call`,
//! mirrors `Aggregator`'s chr-interning + `(u32,pos) -> (meth,unmeth)` map and
//! the input.rs header / `Bismark` / chomp handling.
//!
//! NOT committed to a PR. Run on oxy against /tmp/bg_keep/ext_full/*.gz:
//!   read_phase_split <file.gz> [more files...]

use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::Instant;

use flate2::read::GzDecoder;
use rustc_hash::FxHashMap;

use bismark_bedgraph::validate::validate_call;

fn open(path: &Path) -> Box<dyn BufRead> {
    let f = File::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    if path.extension().and_then(|e| e.to_str()) == Some("gz") {
        Box::new(BufReader::new(GzDecoder::new(f)))
    } else {
        Box::new(BufReader::new(f))
    }
}

/// P1 — decompress only: read every line into a reused buffer, count lines and
/// uncompressed bytes. No parsing, no allocation per line.
fn pass_decompress(path: &Path) -> (u64, u64, f64) {
    let mut r = open(path);
    let mut buf = String::new();
    let (mut lines, mut bytes) = (0u64, 0u64);
    let t = Instant::now();
    loop {
        buf.clear();
        let n = r.read_line(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        lines += 1;
        bytes += n as u64;
    }
    (lines, bytes, t.elapsed().as_secs_f64())
}

/// P2 — decompress + parse + validate (discard). Mirrors input.rs: drop the
/// first (header) line, skip `Bismark` lines, chomp `\n` only, split on tab,
/// parse pos as u32, `validate_call`. Returns valid-call count.
fn pass_parse(path: &Path) -> (u64, f64) {
    let mut r = open(path);
    let mut buf = String::new();
    let mut valid = 0u64;
    let mut first = true;
    let t = Instant::now();
    loop {
        buf.clear();
        let n = r.read_line(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        let line = buf.strip_suffix('\n').unwrap_or(buf.as_str());
        if first {
            first = false;
            continue; // header (default mode: drop first line)
        }
        if line.starts_with("Bismark") {
            continue;
        }
        let mut it = line.split('\t');
        let _id = it.next();
        let strand = it.next().unwrap_or("");
        let _chr = it.next().unwrap_or("");
        let pos_field = it.next().unwrap_or("");
        let call = it.next().unwrap_or("");
        let _pos: u32 = match pos_field.parse() {
            Ok(p) if p >= 1 => p,
            _ => continue,
        };
        if validate_call(strand, call) {
            valid += 1;
        }
    }
    (valid, t.elapsed().as_secs_f64())
}

type Counts = FxHashMap<(u32, u32), (u32, u32)>;

/// P3 — decompress + parse + insert into a FRESH per-file map (the Family-A
/// per-file accumulator). Returns the built map + distinct-position count + time.
fn pass_insert_fresh(path: &Path) -> (Counts, usize, f64) {
    let mut r = open(path);
    let mut buf = String::new();
    let mut chr_ids: FxHashMap<Box<str>, u32> = FxHashMap::default();
    let mut counts: Counts = FxHashMap::default();
    let mut first = true;
    let t = Instant::now();
    loop {
        buf.clear();
        let n = r.read_line(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        let line = buf.strip_suffix('\n').unwrap_or(buf.as_str());
        if first {
            first = false;
            continue;
        }
        if line.starts_with("Bismark") {
            continue;
        }
        let mut it = line.split('\t');
        let _id = it.next();
        let strand = it.next().unwrap_or("");
        let chr = it.next().unwrap_or("");
        let pos_field = it.next().unwrap_or("");
        let call = it.next().unwrap_or("");
        let pos: u32 = match pos_field.parse() {
            Ok(p) if p >= 1 => p,
            _ => continue,
        };
        if !validate_call(strand, call) {
            continue;
        }
        // intern chr (mirror Aggregator::intern)
        let id = match chr_ids.get(chr) {
            Some(&id) => id,
            None => {
                let id = chr_ids.len() as u32;
                chr_ids.insert(chr.into(), id);
                id
            }
        };
        let e = counts.entry((id, pos)).or_insert((0, 0));
        if strand == "+" {
            e.0 += 1;
        } else {
            e.1 += 1;
        }
    }
    let elapsed = t.elapsed().as_secs_f64();
    let distinct = counts.len();
    (counts, distinct, elapsed)
}

/// P4 — merge a per-file map into a fresh global map (the Family-A reduce). This
/// is ~`map.len()` global inserts: the cost the parallel design must pay back
/// serially after the parallel parse.
fn pass_merge(src: &Counts) -> (usize, f64) {
    let mut global: Counts = FxHashMap::with_capacity_and_hasher(src.len(), Default::default());
    let t = Instant::now();
    for (&k, &v) in src {
        let e = global.entry(k).or_insert((0, 0));
        e.0 += v.0;
        e.1 += v.1;
    }
    (global.len(), t.elapsed().as_secs_f64())
}

fn main() {
    let files: Vec<String> = env::args().skip(1).collect();
    if files.is_empty() {
        eprintln!("usage: read_phase_split <file.gz> [more files...]");
        std::process::exit(2);
    }

    for f in &files {
        let path = Path::new(f);
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or(f);
        eprintln!("\n==== {name} ====");

        let (lines, ubytes, t_dec) = pass_decompress(path);
        let (valid, t_par) = pass_parse(path);
        let (map, distinct, t_ins) = pass_insert_fresh(path);
        let (gdistinct, t_mrg) = pass_merge(&map);
        drop(map);

        let parse_only = (t_par - t_dec).max(0.0);
        let insert_only = (t_ins - t_par).max(0.0);
        let est_bytes = distinct as f64 * 40.0; // ~40 B/entry (hashbrown (u32,u32)->(u32,u32))

        eprintln!("lines (incl header)     : {lines}");
        eprintln!("uncompressed bytes      : {ubytes}  ({:.2} GB)", ubytes as f64 / 1e9);
        eprintln!("valid calls             : {valid}");
        eprintln!("distinct positions      : {distinct}");
        eprintln!(
            "calls : positions       : {:.2} : 1",
            valid as f64 / distinct.max(1) as f64
        );
        eprintln!("est. map footprint      : {:.1} GB (~40 B/entry)", est_bytes / 1e9);
        eprintln!("--- read-phase split (full file) ---");
        eprintln!("P1 decompress           : {t_dec:>7.1}s   ({:>4.1}% of P3)", 100.0 * t_dec / t_ins.max(1e-9));
        eprintln!("P2 = decompress+parse   : {t_par:>7.1}s   -> parse alone  {parse_only:>7.1}s ({:>4.1}%)", 100.0 * parse_only / t_ins.max(1e-9));
        eprintln!("P3 = +insert (fresh)    : {t_ins:>7.1}s   -> insert alone {insert_only:>7.1}s ({:>4.1}%)", 100.0 * insert_only / t_ins.max(1e-9));
        eprintln!("P4 merge -> global      : {t_mrg:>7.1}s   (global distinct {gdistinct}; ~{:.0}% of P3)", 100.0 * t_mrg / t_ins.max(1e-9));
        eprintln!(
            "SPLIT % (of P3)         : decompress {:.0}% | parse {:.0}% | insert {:.0}%",
            100.0 * t_dec / t_ins.max(1e-9),
            100.0 * parse_only / t_ins.max(1e-9),
            100.0 * insert_only / t_ins.max(1e-9),
        );
    }
}
