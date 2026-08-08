// Layout gate for the byte-identity oracles: unconditional, no tooling needed, cannot skip.
#[test]
fn legacy_perl_toolchain_is_present() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../legacy_perl");
    for f in [
        "bam2nuc",
        "bismark",
        "bismark2bedGraph",
        "bismark2report",
        "bismark2summary",
        "bismark_genome_preparation",
        "bismark_methylation_extractor",
        "coverage2cytosine",
        "deduplicate_bismark",
        "filter_non_conversion",
        "methylation_consistency",
        "NOMe_filtering",
        "plotly/plot.ly",
        "plotly/plotly_template.tpl",
        "plotly/bismark.logo",
        "plotly/bioinf.logo",
    ] {
        assert!(
            dir.join(f).exists(),
            "legacy_perl/{f} missing — layout assumption broken"
        );
    }
}
