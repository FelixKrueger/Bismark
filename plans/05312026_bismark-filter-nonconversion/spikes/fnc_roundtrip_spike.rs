//! THROWAWAY SPIKE (filter_non_conversion port) — NOT shipped code.
//!
//! Question: does a pure noodles `RecordBuf` -> BAM round-trip preserve the
//! alignment-record body byte-identically vs samtools' own reference, so a
//! verbatim-passthrough filter can rely on it for byte-identity?
//!
//! Run: cargo run -p bismark-io --example fnc_roundtrip_spike -- <in.bam> <out.bam>
//! Then compare `samtools view <out.bam>` against `samtools view <in.bam>`.

use std::fs::File;
use std::io::BufReader;

use noodles_bam as bam;
use noodles_sam::alignment::io::Write as _;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: fnc_roundtrip_spike <in.bam> <out.bam>");
        std::process::exit(2);
    }
    let input = &args[1];
    let output = &args[2];

    let mut reader = bam::io::Reader::new(BufReader::new(File::open(input)?));
    let header = reader.read_header()?;

    let mut writer = bam::io::Writer::new(File::create(output)?);
    writer.write_header(&header)?;

    let mut n: u64 = 0;
    for result in reader.record_bufs(&header) {
        let record = result?;
        writer.write_alignment_record(&header, &record)?;
        n += 1;
    }
    writer.try_finish()?;
    eprintln!("noodles round-trip wrote {n} records");
    Ok(())
}
