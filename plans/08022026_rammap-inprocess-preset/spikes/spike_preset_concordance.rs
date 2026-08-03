//! THROWAWAY measurement for the #1092 follow-up — delete after capturing output.
//!
//! Question: how well does **in-process rammap** agree with **minimap2** per preset, and
//! specifically for `sr`, which #1092 made reachable in-process for the first time with no
//! concordance figure attached (its A4).
//!
//! Two configurations, because they answer different questions:
//!   (1) SAME-CONFIG — rammap `from_seqs(preset)` vs minimap2 `-x preset`. Both build their
//!       index with the preset's own k, so this isolates engine divergence.
//!   (2) PRODUCTION HYBRID — rammap `from_index` over an index built at map-ont's k (standing
//!       in for genome-prep's fixed k) with `sr` mapping options, vs minimap2 `-x sr`. This is
//!       what a real `bismark --rammap --mm2_short_reads` run does, and the plan flagged the
//!       seeding half as un-converged.
//!
//! Run: cargo test -p bismark --features rammap-inprocess \
//!        --test spike_preset_concordance -- --nocapture

#![cfg(feature = "rammap-inprocess")]

use std::io::Write;
use std::process::Command as StdCommand;

use bismark::aligner::inprocess::reconstruct_cigar;
use bismark::aligner::rammap;

const BISMARK_OPTS: [&str; 6] = ["-a", "--MD", "--secondary=no", "-t", "1", "-K"];

#[derive(Debug, PartialEq, Clone)]
struct Hit {
    rname: String,
    pos: i64,
    rev: bool,
    cigar: String,
    score: i64,
}

fn gen_reference(n: usize) -> Vec<u8> {
    let bases = [b'A', b'C', b'G', b'T'];
    let mut x: u64 = 0x0803_2026_C0FF_EE00;
    (0..n)
        .map(|_| {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            bases[((x >> 33) % 4) as usize]
        })
        .collect()
}

fn flip(b: u8) -> u8 {
    match b {
        b'A' => b'C',
        b'C' => b'A',
        b'G' => b'T',
        _ => b'G',
    }
}

fn revcomp(s: &[u8]) -> Vec<u8> {
    s.iter()
        .rev()
        .map(|&b| match b {
            b'A' => b'T',
            b'T' => b'A',
            b'C' => b'G',
            _ => b'C',
        })
        .collect()
}

/// A varied read set: perfect / mismatched / indel / clipped, both strands, several lengths.
fn build_reads(reference: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for (i, len) in [75usize, 100, 150, 250].into_iter().enumerate() {
        let at = 3_000 + i * 1_500;
        let base = &reference[at..at + len];
        out.push((format!("perfect_{len}"), base.to_vec()));
        out.push((format!("rev_{len}"), revcomp(base)));
        for k in [1usize, 3, 6] {
            let mut r = base.to_vec();
            for j in 0..k {
                let p = (j + 1) * len / (k + 1);
                r[p] = flip(r[p]);
            }
            out.push((format!("mm{k}_{len}"), r));
        }
        let mut del = base.to_vec();
        del.drain(len / 2..len / 2 + 3);
        out.push((format!("del3_{len}"), del));
        let mut ins = base.to_vec();
        for b in b"ACGTA".iter().rev() {
            ins.insert(len / 2, *b);
        }
        out.push((format!("ins5_{len}"), ins));
        // Soft-clip bait: 12 bp of unrelated sequence on the 5' end.
        let mut clip = b"TTTTTTTTTTTT".to_vec();
        clip.extend_from_slice(base);
        out.push((format!("clip12_{len}"), clip));
    }
    out
}

fn minimap2_hits(
    ref_fa: &std::path::Path,
    fq: &std::path::Path,
    preset: &str,
) -> std::collections::HashMap<String, Hit> {
    let out = StdCommand::new("minimap2")
        .args(BISMARK_OPTS)
        .arg("250K")
        .arg("-x")
        .arg(preset)
        .arg(ref_fa)
        .arg(fq)
        .output()
        .expect("minimap2");
    assert!(out.status.success(), "minimap2 -x {preset} failed");
    let mut m = std::collections::HashMap::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if line.starts_with('@') {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        let flag: u16 = f[1].parse().unwrap();
        if flag & 0x904 != 0 {
            continue;
        }
        let score = f[11..]
            .iter()
            .find_map(|t| t.strip_prefix("AS:i:").and_then(|v| v.parse::<i64>().ok()))
            .unwrap_or(i64::MIN);
        m.insert(
            f[0].to_string(),
            Hit {
                rname: f[2].to_string(),
                pos: f[3].parse().unwrap(),
                rev: flag & 0x10 != 0,
                cigar: f[5].to_string(),
                score,
            },
        );
    }
    m
}

fn rammap_hits(
    aligner: &rammap::Aligner,
    reads: &[(String, Vec<u8>)],
) -> std::collections::HashMap<String, Hit> {
    let mut m = std::collections::HashMap::new();
    for (name, seq) in reads {
        let res = aligner.map_seq_with(
            name,
            seq,
            rammap::MapOpts {
                cs: None,
                md: Some(true),
            },
        );
        if let Some(p) = res
            .mappings
            .iter()
            .find(|p| p.is_primary && !p.is_supplementary)
        {
            m.insert(
                name.clone(),
                Hit {
                    rname: p.target_name.to_string(),
                    pos: p.target_start as i64 + 1, // rammap is 0-based, SAM is 1-based
                    rev: p.strand == rammap::Strand::Reverse,
                    // PRODUCTION step: rammap's `cigar` is the aligned CORE without soft
                    // clips; `inprocess.rs` rebuilds the SAM CIGAR from query_start/_end.
                    // Comparing the raw core against minimap2's full SAM CIGAR would report
                    // every clipped read as divergent when it is not.
                    cigar: reconstruct_cigar(
                        p.query_start as usize,
                        p.query_end as usize,
                        seq.len(),
                        p.strand == rammap::Strand::Reverse,
                        p.cigar.as_deref().unwrap_or(""),
                    ),
                    score: p.score as i64,
                },
            );
        }
    }
    m
}

fn compare(
    label: &str,
    reads: &[(String, Vec<u8>)],
    ram: &std::collections::HashMap<String, Hit>,
    mm2: &std::collections::HashMap<String, Hit>,
) {
    let (mut both, mut agree_locus, mut agree_all, mut only_ram, mut only_mm2, mut neither) =
        (0, 0, 0, 0, 0, 0);
    let mut diffs = Vec::new();
    for (name, _) in reads {
        match (ram.get(name), mm2.get(name)) {
            (Some(a), Some(b)) => {
                both += 1;
                let locus = a.rname == b.rname && a.pos == b.pos && a.rev == b.rev;
                if locus {
                    agree_locus += 1;
                }
                if locus && a.cigar == b.cigar && a.score == b.score {
                    agree_all += 1;
                } else {
                    diffs.push(format!(
                        "    {name}: ram {}:{}{} {} AS={} | mm2 {}:{}{} {} AS={}",
                        a.rname,
                        a.pos,
                        if a.rev { "-" } else { "+" },
                        a.cigar,
                        a.score,
                        b.rname,
                        b.pos,
                        if b.rev { "-" } else { "+" },
                        b.cigar,
                        b.score
                    ));
                }
            }
            (Some(_), None) => { only_ram += 1; diffs.push(format!("    {name}: RAMMAP-ONLY (minimap2 did not map it)")); }
            (None, Some(_)) => { only_mm2 += 1; diffs.push(format!("    {name}: MINIMAP2-ONLY")); }
            (None, None) => neither += 1,
        }
    }
    let n = reads.len();
    println!(
        "  {label:<34} n={n:<3} both={both:<3} locus_agree={agree_locus:<3} \
         full_agree={agree_all:<3} ram_only={only_ram} mm2_only={only_mm2} neither={neither}"
    );
    if agree_locus > 0 {
        println!(
            "  {:<34} locus concordance = {:.1}% of co-mapped; full = {:.1}%",
            "",
            100.0 * agree_locus as f64 / both.max(1) as f64,
            100.0 * agree_all as f64 / both.max(1) as f64
        );
    }
    for d in diffs.iter().take(6) {
        println!("{d}");
    }
    if diffs.len() > 6 {
        println!("    … {} more differing cells", diffs.len() - 6);
    }
}

#[test]
fn spike_per_preset_concordance() {
    let reference = gen_reference(30_000);
    let reads = build_reads(&reference);
    let tmp = tempfile::TempDir::new().unwrap();

    let ref_fa = tmp.path().join("ref.fa");
    let mut fa = std::fs::File::create(&ref_fa).unwrap();
    writeln!(fa, ">chrS").unwrap();
    for c in reference.chunks(60) {
        fa.write_all(c).unwrap();
        fa.write_all(b"\n").unwrap();
    }
    drop(fa);

    let fq_path = tmp.path().join("reads.fq");
    let mut fq = std::fs::File::create(&fq_path).unwrap();
    for (n, s) in &reads {
        writeln!(fq, "@{n}").unwrap();
        fq.write_all(s).unwrap();
        writeln!(fq, "\n+\n{}", "I".repeat(s.len())).unwrap();
    }
    drop(fq);

    println!("\n=== (1) SAME-CONFIG: rammap from_seqs(preset) vs minimap2 -x preset ===");
    for (name, preset) in [
        ("map-ont", rammap::Preset::MapOnt),
        ("map-pb", rammap::Preset::MapPb),
        ("sr", rammap::Preset::Sr),
    ] {
        let a = rammap::Aligner::from_seqs(vec![("chrS".to_string(), reference.clone())], preset);
        compare(
            &format!("{name}"),
            &reads,
            &rammap_hits(&a, &reads),
            &minimap2_hits(&ref_fa, &fq_path, name),
        );
    }

    println!("\n=== (2) PRODUCTION HYBRID: rammap sr options over a map-ont-k index ===");
    let idx = tmp.path().join("hybrid.mmi");
    rammap::Aligner::from_seqs(
        vec![("chrS".to_string(), reference.clone())],
        rammap::Preset::MapOnt,
    )
    .save_index(idx.to_str().unwrap())
    .unwrap();
    let hybrid = rammap::Aligner::from_index(idx.to_str().unwrap(), rammap::Preset::Sr).unwrap();
    compare(
        "sr-over-mapont-index vs mm2 sr",
        &reads,
        &rammap_hits(&hybrid, &reads),
        &minimap2_hits(&ref_fa, &fq_path, "sr"),
    );
    // The same index read back with map-ont, as the control.
    let plain = rammap::Aligner::from_index(idx.to_str().unwrap(), rammap::Preset::MapOnt).unwrap();
    compare(
        "control: mapont-index+mapont",
        &reads,
        &rammap_hits(&plain, &reads),
        &minimap2_hits(&ref_fa, &fq_path, "map-ont"),
    );
}
