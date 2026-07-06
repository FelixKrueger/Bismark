//! Embeds this crate's last-modified date (its last git commit date) as
//! `BISMARK_LAST_MODIFIED`, shown in the `--help` footer. Falls back to the
//! build date outside a git checkout (e.g. a crates.io registry build).
fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    println!(
        "cargo:rustc-env=BISMARK_LAST_MODIFIED={}",
        bismark_meta::last_modified_date(manifest_dir)
    );
    // Re-run when this crate's tracked files change (best-effort: the crate dir).
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
}
