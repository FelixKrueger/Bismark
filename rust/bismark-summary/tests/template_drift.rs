//! Drift guard: the committed `summary_template.html` MUST equal the inline
//! single-quoted heredoc in the Perl `bismark2summary` source. If the Perl
//! template is ever edited, this fails loudly so the embedded copy is
//! refreshed. Auto-skips when the Perl source is unavailable.

use std::path::Path;

const TEMPLATE: &str = include_str!("../src/summary_template.html");

#[test]
fn embedded_template_matches_perl_heredoc() {
    let src_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bismark2summary");
    if !src_path.exists() {
        eprintln!(
            "skipping: Perl source unavailable at {}",
            src_path.display()
        );
        return;
    }
    let src = std::fs::read_to_string(&src_path).unwrap();

    // Heredoc body = everything between the opener line and the terminator
    // line `HTMLTEMPLATESTRING`. Each body line keeps its trailing `\n`,
    // including the last (`</html>\n`).
    let open = "<<'HTMLTEMPLATESTRING';";
    let open_idx = src.find(open).expect("heredoc opener present in source");
    let body_start = open_idx
        + src[open_idx..]
            .find('\n')
            .expect("newline after opener line")
        + 1;
    let rest = &src[body_start..];
    let term_rel = rest
        .find("\nHTMLTEMPLATESTRING\n")
        .expect("heredoc terminator present");
    // Include the `\n` that terminates the last body line.
    let body = &rest[..=term_rel];

    assert_eq!(
        TEMPLATE, body,
        "src/summary_template.html has drifted from the Perl heredoc — re-extract it"
    );
}
