# Wiki schema

Template version: 0.8 (2026-10-07).

## Local rules

- The Rust suite in `rust/bismark/src/` is the source of truth. The Perl scripts in `legacy_perl/` appear only as a `legacy` row in `overview.md`.
- Every `sources` entry names a file under `rust/bismark/src/` or a build file at the repo root (`Dockerfile`, `.github/workflows/release.yml`, `rust/bismark/Cargo.toml`).
- The wiki describes the `dev` branch, where the wiki is maintained. Features on `dev` can be ahead of the latest crates.io / Bioconda release.
