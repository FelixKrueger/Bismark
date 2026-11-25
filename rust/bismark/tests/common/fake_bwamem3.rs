//! A hermetic stand-in for `bwa-mem3 mem` that places each read pair where a table says, so
//! the 5-Base driver can be tested on known placements without the real aligner.
//!
//! Include with `#[path = "common/fake_bwamem3.rs"] mod fake_bwamem3;`.

#![allow(dead_code)]

use std::fs;
use std::path::Path;

/// Where one pair maps: the contig, the 0-based leftmost starts of R1 and R2, and whether R1 is
/// the reverse mate (FLAGs 83/163 instead of 99/147). Pairs not in the table come out unmapped.
pub struct Placement {
    pub qname: String,
    pub contig: String,
    pub r1: usize,
    pub r2: usize,
    pub r1_reverse: bool,
}

/// The fake. It reads both FASTQs in lockstep, cuts the QNAME at the first space as the real
/// aligner does, and writes each mate with SEQ/QUAL from the FASTQ (reverse-complemented for a
/// reverse mate), a `{len}M` CIGAR and FLAG 0x2.
const SCRIPT: &str = r#"#!/bin/sh
[ "$1" = version ] && { echo 0.18.0; exit 0; }
n=$#
eval prefix=\${$((n-2))}
eval r1=\${$((n-1))}
eval r2=\${$n}
cat "$prefix.fake_header"
awk -v R2="$r2" -v TABLE="$prefix.fake_placements" '
function rc(s,   i, o, c) {
  o = ""
  for (i = length(s); i > 0; i--) {
    c = substr(s, i, 1)
    o = o (c == "A" ? "T" : c == "C" ? "G" : c == "G" ? "C" : c == "T" ? "A" : c)
  }
  return o
}
function rev(s,   i, o) {
  o = ""
  for (i = length(s); i > 0; i--) o = o substr(s, i, 1)
  return o
}
function emit(   l1, l2, a, b, lo, hi, t, f1, f2, x1, x2, y1, y2, t1, t2) {
  l1 = length(s1); l2 = length(s2)
  if (!(id in contig)) {
    print id "\t77\t*\t0\t0\t*\t*\t0\t0\t" s1 "\t" q1
    print id "\t141\t*\t0\t0\t*\t*\t0\t0\t" s2 "\t" q2
    return
  }
  a = p1[id]; b = p2[id]
  lo = (a < b) ? a : b
  hi = (a + l1 > b + l2) ? a + l1 : b + l2
  t = hi - lo
  if (a <= b) { t1 = t; t2 = -t } else { t1 = -t; t2 = t }
  if (r1rev[id] == "1") {
    f1 = 83; f2 = 163; x1 = rc(s1); y1 = rev(q1); x2 = s2; y2 = q2
  } else {
    f1 = 99; f2 = 147; x1 = s1; y1 = q1; x2 = rc(s2); y2 = rev(q2)
  }
  print id "\t" f1 "\t" contig[id] "\t" a "\t60\t" l1 "M\t=\t" b "\t" t1 "\t" x1 "\t" y1 "\tAS:i:" l1
  print id "\t" f2 "\t" contig[id] "\t" b "\t60\t" l2 "M\t=\t" a "\t" t2 "\t" x2 "\t" y2 "\tAS:i:" l2
}
BEGIN {
  while ((getline line < TABLE) > 0) {
    split(line, f, "\t")
    contig[f[1]] = f[2]; p1[f[1]] = f[3]; p2[f[1]] = f[4]; r1rev[f[1]] = f[5]
  }
}
NR % 4 == 1 { id = $1; sub(/^@/, "", id); sub(/\/1$/, "", id); getline h2 < R2 }
NR % 4 == 2 { s1 = $0; getline s2 < R2 }
NR % 4 == 3 { getline plus2 < R2 }
NR % 4 == 0 { q1 = $0; getline q2 < R2; emit() }
' "$r1"
"#;

/// Write the fake `bwa-mem3` into `bin_dir`, and at `prefix` a stub meth index for `contigs`
/// plus the header and placement table the fake reads.
#[cfg(unix)]
pub fn install(bin_dir: &Path, prefix: &Path, contigs: &[(&str, usize)], placements: &[Placement]) {
    use std::os::unix::fs::PermissionsExt;
    let p = |suffix: &str| format!("{}.{suffix}", prefix.display());
    for s in [
        "amb",
        "pac",
        "meth.amb",
        "meth.ann",
        "meth.pac",
        "meth.bwt.2bit.64",
    ] {
        fs::write(p(s), b"").unwrap();
    }
    let total: usize = contigs.iter().map(|c| c.1).sum();
    let mut ann = format!("{total} {} 11\n", contigs.len());
    let mut header = String::from("@HD\tVN:1.5\n");
    let mut off = 0;
    for (name, len) in contigs {
        ann.push_str(&format!("0 {name} (null)\n{off} {len} 0\n"));
        header.push_str(&format!("@SQ\tSN:{name}\tLN:{len}\n"));
        off += len;
    }
    header.push_str("@PG\tID:bwa-mem3\tPN:bwa-mem3\tVN:0.18.0\n");
    fs::write(p("ann"), ann).unwrap();
    fs::write(p("fake_header"), header).unwrap();
    let table: String = placements
        .iter()
        .map(|pl| {
            format!(
                "{}\t{}\t{}\t{}\t{}\n",
                pl.qname,
                pl.contig,
                pl.r1 + 1,
                pl.r2 + 1,
                u8::from(pl.r1_reverse)
            )
        })
        .collect();
    fs::write(p("fake_placements"), table).unwrap();

    let fake = bin_dir.join("bwa-mem3");
    fs::write(&fake, SCRIPT).unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    // A test thread forking while the script was open for writing makes exec fail briefly.
    for _ in 0..100 {
        match std::process::Command::new(&fake).arg("version").output() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            _ => break,
        }
    }
}
