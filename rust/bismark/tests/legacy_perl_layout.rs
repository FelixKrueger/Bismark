// Layout gate for the byte-identity oracles: unconditional, no tooling needed, cannot skip.
#[test]
fn legacy_perl_toolchain_is_present() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../legacy_perl");
    // ci_tests.yml runs these as ./legacy_perl/<name>, so the execute bit is load-bearing.
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
    ] {
        let p = dir.join(f);
        assert!(
            p.exists(),
            "legacy_perl/{f} missing — layout assumption broken"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode();
            assert!(
                mode & 0o111 != 0,
                "legacy_perl/{f} lost its execute bit (mode {mode:o})"
            );
        }
    }
    for f in [
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
