//! #1148 guard: no hard-coded Perl version in non-test source or the HTML templates.

use std::fs;
use std::path::{Path, PathBuf};

const PERL_VERSION: &str = "0.25.1";

fn walk(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk(&path, exts, out);
        } else if path
            .extension()
            .is_some_and(|e| exts.iter().any(|x| e == *x))
        {
            out.push(path);
        }
    }
}

/// Non-comment lines outside `#[cfg(test)]` items that contain [`PERL_VERSION`].
fn hits_in_rust(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut hits = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some(rest) = line.trim_start().strip_prefix("#[cfg(test)]") {
            i = end_of_item(&lines, i, rest);
            continue;
        }
        if !line.trim_start().starts_with("//") && line.contains(PERL_VERSION) {
            hits.push(line.to_string());
        }
        i += 1;
    }
    hits
}

/// The line index after the item that the `#[cfg(test)]` on line `at` annotates;
/// `rest` is the text following the attribute on that line.
fn end_of_item(lines: &[&str], at: usize, rest: &str) -> usize {
    let (mut depth, mut opened, mut j, mut cur) = (0i64, false, at, rest);
    loop {
        depth += cur.matches('{').count() as i64 - cur.matches('}').count() as i64;
        opened |= cur.contains('{');
        if (opened && depth <= 0) || (!opened && cur.trim_end().ends_with(';')) {
            return j + 1;
        }
        j += 1;
        if j >= lines.len() {
            return j;
        }
        cur = lines[j];
    }
}

#[test]
fn no_perl_version_in_non_test_source() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    walk(&root.join("src"), &["rs"], &mut files);
    assert!(files.len() > 50, "walked too few files: {}", files.len());

    let mut hits = Vec::new();
    for f in &files {
        for line in hits_in_rust(&fs::read_to_string(f).unwrap()) {
            hits.push(format!("{}: {}", f.display(), line.trim()));
        }
    }
    assert!(
        hits.is_empty(),
        "hard-coded Perl version; use meta::VERSION_TAG:\n{}",
        hits.join("\n")
    );
}

#[test]
fn no_perl_version_in_templates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    walk(&root.join("src"), &["html"], &mut files);
    walk(&root.join("assets"), &["tpl"], &mut files);
    assert!(!files.is_empty(), "no templates found");
    for f in &files {
        let text = fs::read_to_string(f).unwrap();
        assert!(
            !text.contains(PERL_VERSION),
            "{} carries {PERL_VERSION}",
            f.display()
        );
    }
}

#[test]
fn guard_catches_an_embedded_literal() {
    let src = "pub const H: &str = \"Bismark methylation extractor version v0.25.1\\n\";\n\
               // matches Perl v0.25.1\n\
               #[cfg(test)]\nmod tests { const X: &str = \"v0.25.1\"; }\n";
    assert_eq!(hits_in_rust(src).len(), 1);
}

#[test]
fn guard_scans_past_an_early_cfg_test_item() {
    let src = "impl A {\n\
               \x20   #[cfg(test)]\n\
               \x20   fn f() { let _ = \"v0.25.1\"; }\n\
               \x20   #[cfg(test)]\n\
               \x20   mod t {\n\
               \x20       const X: &str = \"v0.25.1\";\n\
               \x20   }\n\
               \x20   fn g() -> &'static str { \"v0.25.1\" }\n\
               }\n\
               #[cfg(test)]\n\
               use x::v0_25_1;\n\
               pub const Y: &str = \"0.25.1\";\n\
               #[cfg(test)] mod tests { const Z: &str = \"v0.25.1\"; }\n";
    let hits = hits_in_rust(src);
    assert_eq!(hits.len(), 2, "{hits:?}");
    assert!(hits[0].contains("fn g()") && hits[1].contains("pub const Y"));
}
