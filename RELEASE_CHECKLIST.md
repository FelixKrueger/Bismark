# Bismark Rust suite — release checklist (SUPERSEDED)

> **Superseded by the single-binary consolidation (3.0.0).** The Rust suite is now one
> `bismark` crate that builds one multicall binary, so the old per-crate publish DAG and
> the `rust/iron-chancellor` integration-branch flow this file described no longer apply.
>
> **The release is automated by [`.github/workflows/release.yml`](.github/workflows/release.yml)** —
> dispatch it from `master` for a GA, or from a branch with `dry_run=true` to validate. It
> builds the one `--bin bismark` for all platforms, smoke-tests the 12 tool names,
> `cargo publish -p bismark`, pushes the multi-arch GHCR image (`:latest` + `:<version>`),
> tags `bismark-rust-v<version>`, and drafts then finalizes the GitHub release.
>
> The suite version is the single-source `rust/VERSION`, which must equal
> `rust/bismark/VERSION` **and** `rust/bismark/Cargo.toml`'s `version` (the value
> `cargo publish` registers) — enforced by the `meta` version guards. See the Phase-6 plan
> under `plans/07062026_single-binary-suite/phase6-byte-identity-gate-release/` for the cut runbook.
