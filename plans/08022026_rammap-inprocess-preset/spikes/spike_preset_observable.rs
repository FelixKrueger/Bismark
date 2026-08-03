//! THROWAWAY SPIKE for #1092 — delete after capturing output.
//!
//! Question: is a `rammap::Preset` difference observable through `Aligner::from_seqs` on a
//! small synthetic reference, so a HERMETIC feature-gated test can fail if the in-process
//! site (`mod.rs:968`) uses the wrong preset?
//!
//! Run: cargo test -p bismark --features rammap-inprocess \
//!        --test spike_1092_preset_observable -- --nocapture

#![cfg(feature = "rammap-inprocess")]

/// Deterministic pseudo-random ACGT reference (fixed LCG).
fn gen_reference(n: usize) -> Vec<u8> {
    let bases = [b'A', b'C', b'G', b'T'];
    let mut x: u64 = 0x1092_1092_5EED_5EED;
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

#[test]
fn spike_is_preset_observable_via_from_seqs() {
    let reference = gen_reference(20_000);

    // Reads cut from the interior so both flanks are extendable.
    let at = 5_000usize;
    let mut cases: Vec<(String, Vec<u8>)> = Vec::new();
    for len in [30usize, 60, 100, 150] {
        let base = &reference[at..at + len];
        cases.push((format!("perfect_{len}"), base.to_vec()));
        let mut one = base.to_vec();
        one[len / 2] = flip(one[len / 2]);
        cases.push((format!("mm1_{len}"), one));
        let mut three = base.to_vec();
        for i in [len / 4, len / 2, 3 * len / 4] {
            three[i] = flip(three[i]);
        }
        cases.push((format!("mm3_{len}"), three));
        // A 2 bp deletion relative to the reference (gap_open differs 4 vs 12).
        let mut del = base.to_vec();
        del.drain(len / 2..len / 2 + 2);
        cases.push((format!("del2_{len}"), del));
    }
    // Below sr's k=21 seed length; map-ont's k=15 should still seed.
    for len in [16usize, 18, 20, 22] {
        cases.push((
            format!("short_{len}"),
            reference[at..at + len].to_vec(),
        ));
    }

    let presets = [
        ("MapOnt", rammap::Preset::MapOnt),
        ("MapPb", rammap::Preset::MapPb),
        ("Sr", rammap::Preset::Sr),
    ];

    // name -> preset -> rendered outcome
    let mut table: Vec<(String, Vec<String>)> = Vec::new();
    for (name, read) in &cases {
        let mut row = Vec::new();
        for (_, preset) in &presets {
            let aligner = rammap::Aligner::from_seqs(
                vec![("chr1_CT_converted".to_string(), reference.clone())],
                *preset,
            );
            let res = aligner.map_seq_with(
                name,
                read,
                rammap::MapOpts {
                    cs: None,
                    md: Some(true),
                },
            );
            let primary = res
                .mappings
                .iter()
                .find(|m| m.is_primary && !m.is_supplementary);
            row.push(match primary {
                None => "UNMAPPED".to_string(),
                Some(m) => format!(
                    "AS={} cig={} mapq={}",
                    m.score,
                    m.cigar.as_deref().unwrap_or("-"),
                    m.mapq
                ),
            });
        }
        table.push((name.clone(), row));
    }

    println!("\n{:<14} {:<28} {:<28} {:<28}", "case", "MapOnt", "MapPb", "Sr");
    let mut discriminating = Vec::new();
    for (name, row) in &table {
        let differs = row[0] != row[2]; // MapOnt vs Sr — the pair the fix must distinguish
        println!(
            "{:<14} {:<28} {:<28} {:<28}{}",
            name,
            row[0],
            row[1],
            row[2],
            if differs { "  <== MapOnt != Sr" } else { "" }
        );
        if differs {
            discriminating.push(name.clone());
        }
    }

    println!("\nMapOnt-vs-Sr discriminating cases: {discriminating:?}");
    println!("count: {}/{}", discriminating.len(), table.len());

    // Determinism: re-run one discriminating case 3x and confirm identical output.
    if let Some(name) = discriminating.first() {
        let (_, read) = cases.iter().find(|(n, _)| n == name).unwrap();
        let mut seen = Vec::new();
        for _ in 0..3 {
            let aligner = rammap::Aligner::from_seqs(
                vec![("chr1_CT_converted".to_string(), reference.clone())],
                rammap::Preset::Sr,
            );
            let res = aligner.map_seq_with(
                name,
                read,
                rammap::MapOpts {
                    cs: None,
                    md: Some(true),
                },
            );
            let p = res
                .mappings
                .iter()
                .find(|m| m.is_primary && !m.is_supplementary);
            seen.push(p.map(|m| (m.score, m.cigar.clone(), m.mapq)));
        }
        println!(
            "determinism on {name}: 3 runs identical = {}",
            seen.windows(2).all(|w| w[0] == w[1])
        );
    }
}
