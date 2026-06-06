# PLAN_REVIEW_B — Bismark Rust suite: wiring, packaging & distribution (rev 2)

**Reviewer:** Plan Reviewer B (independent, adversarial, fresh context)
**Plan:** `plans/06062026_rust-suite-packaging/PLAN.md` (rev 2)
**Date:** 2026-06-06
**Verdict:** **REQUEST CHANGES** — the strategic direction is sound and well-grounded in the Trim Galore template, but the plan glosses several mechanics that, if carried out as written, cause an **irreversible crates.io mess (immutable publish)**, a **byte-identity-breaking Docker image (unpinned aligner)**, and a **GPL distribution shipping with no license file**. These must be resolved in the SPEC before any release infra is built.

> This is a *kickoff/framing* plan, not a finalized SPEC — so most findings are "the SPEC must specify X," not "the code is wrong." I have weighted accordingly. But the immutable-publish and Docker-pinning items are decision-level, not implementation-detail, and need to be settled now.

---

## What I independently re-derived (do not trust the plan's claims; these are verified)

| Claim in plan | Verified? | Evidence |
|---|---|---|
| `bismark-io` published on crates.io at `1.0.0-beta.6` | **Yes, but incomplete** | `curl crates.io/api/v1/crates/bismark-io` → published versions are **beta.1, beta.5, beta.6** (NOT contiguous; beta.2/3/4 were never published). `newest=default=beta.6`, none yanked. |
| Local `bismark-io` is `beta.8` (2 ahead) | **Yes** | `rust/bismark-io/Cargo.toml` → `version = "1.0.0-beta.8"`; `rust/Cargo.lock` agrees. |
| beta.7/beta.8 were "internal additive no-bump" | **Yes, and worse than stated** | beta.7 set in `6782021` (#880, *new* public CIGAR-trim API); beta.8 set in `6ab03f7` (NOMe-filtering, *new* public `bismark_io::genome` module). The beta.8 commit message literally says "bismark-io stays =1.0.0-beta.8 … additive." So **beta.8 has accreted at least two rounds of new public API**. |
| 13 crates, `bismark-io` lib + 12 bins | **Yes** | `rust/Cargo.toml` members list = 13. |
| Every binary exact-pins `bismark-io = "=1.0.0-beta.8"` | **Yes, for the 8 that depend on it** | dedup, extractor, methylation-consistency, nome-filtering, bam2nuc, filter-nonconversion, aligner all pin `=1.0.0-beta.8` in both `version` and `path`. (bedgraph, c2c, genome-prep, report, summary do **not** depend on bismark-io.) |
| Inconsistent `_rs` bin names; extractor is the hyphen outlier | **Yes** | `bismark-methylation-extractor-rs` (hyphens) vs all 11 others `_rs` underscores. |
| Per-crate independent versions today | **Yes** | io beta.8, dedup `1.2.1-beta.1`, aligner `1.0.0-alpha.1`, c2c/bedgraph beta.2, rest beta.1. |
| No release infra exists | **Yes** | No `release.yml`/`justfile`/`Dockerfile`/`build.rs` anywhere in repo or `rust/`. Only `rust_ci.yml` + Perl `ci_tests.yml`. |
| TG creates+pushes the tag itself; `master`=GA / `dev`=prerelease guard; `Cargo.lock`==`Cargo.toml` assert | **Yes** | `~/Github/TrimGalore/.github/workflows/release.yml` lines 75–108. |
| `workspace.package.version` does NOT exist yet; no crate inherits it | **Yes** | `[workspace.package]` has edition/rust-version/license/repo/authors but **no `version`**; grep shows only `rust-version.workspace = true`, never `version.workspace`. |

### Two re-derived facts the plan got WRONG or omitted

1. **crates.io history is non-contiguous (beta.1, beta.5, beta.6 — not beta.6 "+ a clean +2").** The plan's OQ-C says "local is beta.8 (2 ahead) → publish the catch-up." But you can't "catch up" beta.7 and beta.8 as if they were the next two steps: beta.2/3/4 don't exist on crates.io and beta.8's content is a *moving target* under the no-bump convention (see C-1). This is more tangled than "2 ahead."
2. **The workspace pulls a C build dependency (`cc` via `libmimalloc-sys`).** TG is pure-Rust. `cargo tree -i cc` → `cc → libmimalloc-sys → mimalloc → {bam2nuc, bedgraph(+extractor), extractor, filter-nonconversion}`. The plan says "matrix builds the workspace once per target" and "adopt TG's machinery verbatim" without noting that the suite has a C toolchain requirement TG never had. (zlib-rs is pure-Rust — that half is fine.)

---

## Critical findings (block the SPEC / cause irreversible or broken state)

### C-1 — The immutable-publish trap: publishing `beta.8` freezes a version that the project's own convention treats as mutable
crates.io publishes are **permanent and immutable** (yank ≠ delete; you cannot re-upload `1.0.0-beta.8` with different bytes — ever). The documented "no-bump-during-beta" convention (confirmed in the beta.8 commit message: "bismark-io stays =1.0.0-beta.8 … additive") means the team has been **adding public API under a fixed version string**. These two policies are in direct contradiction:

- If you `cargo publish` beta.8 **now**, you nail down *today's* API surface forever. The next additive change that the convention says "stays beta.8" can no longer be published — `cargo publish` will reject the duplicate version, and the only escape is a new version (beta.9 / rc) you swore you wouldn't cut during beta.
- Every binary crate exact-pins `= "=1.0.0-beta.8"`. If you publish beta.8 and later need to bump it (you will — the convention guarantees it), you must bump the pin in **8 crates** in lockstep, or the workspace links one version while crates.io serves another.

**The plan hand-waves this as "keep it current; release.yml automates ongoing publishes" (OQ-C).** That does not resolve the contradiction — it institutionalizes it. Automating publishes of a version that's supposed to be mutable just means CI will refuse the second publish.

**Required SPEC decision (one of):**
- **(a)** Abandon the no-bump convention for `bismark-io` *the moment it goes to crates.io*. Every additive change = a real semver bump (beta.9, beta.10, …) + a synchronized pin bump in all 8 dependents (ideally automated/scripted). This is the honest answer and matches how crates.io actually works.
- **(b)** Do NOT publish `bismark-io` during the beta track at all (defer with the rest, per D2 option (a)). The lib is consumed only by path within the workspace; nobody external needs beta.8. crates.io is a *commitment*, not a CI convenience. Given Felix already deferred bioconda + full crates.io to GA, deferring `bismark-io` too is the consistent choice.

The plan must NOT proceed with "publish beta.8 and keep current" as written.

### C-2 — Batteries-included Docker does not pin the aligner → it ships an image that breaks the project's central guarantee
The entire faithful-port epic is **byte-identical only against pinned Bowtie 2 2.5.5** (verified: `rust/bismark-aligner/README.md` "driving the pinned **Bowtie 2 2.5.5**"; test fixtures hard-code `version 2.5.5`). The plan's Docker section (§3.2, §4a.3, D5) says "+ samtools + Bowtie 2" with **no version pin**. `apt-get install bowtie2` on `debian:bookworm-slim` will install whatever Debian ships (almost certainly NOT 2.5.5), and samtools likewise.

**Consequence:** the "batteries-included" image — the artifact most likely to be used for reproducible/CI pipelines — would produce output that **does not match** the Perl oracle the whole project is validated against, *inside the very container meant to showcase the Rust suite*. A user comparing the Docker image's output to a published Perl Bismark result would see spurious diffs and conclude the port is broken.

**Required:** pin Bowtie 2 to **2.5.5** (and samtools to a chosen version) in the Dockerfile — download a fixed release/build, not `apt-get install bowtie2`. Record the pinned versions in the image labels and README. Note that bowtie2 binaries are not in stock Debian at a controllable version anyway, so this also fixes a likely build failure.

### C-3 — GPL-3.0 tarball + Docker ship with NO license file (TG's `cp LICENSE` silently no-ops here)
TG's `release.yml` packaging does `cp README.md LICENSE "staging/..." 2>/dev/null || true`. Bismark has **no `LICENSE` and no `rust/LICENSE`** — the license file is `license.txt` (lowercase) at the repo root. If the plan adopts TG's packaging verbatim (it says it will), the `cp LICENSE` fails silently and **every tarball and the Docker image ship with the GPL-3.0 license text absent**, despite `license = "GPL-3.0-only"` in every manifest and a Docker `LABEL ... licenses="GPL-3.0-only"`.

For a GPL-3.0 work that **also redistributes** GPL software (Bowtie 2 is GPL) and MIT software (samtools), shipping the binaries with no license text — and no attribution/copyright notices for the bundled aligners — is a genuine GPL §4/§5 compliance gap, not cosmetics.

**Required SPEC items:**
- Add a `LICENSE` (or fix the packaging path to `license.txt`) and ensure it lands in every tarball + the image.
- For Docker: include the GPL-3.0 text **and** the bundled aligners' own license + copyright notices (Bowtie 2 GPL, samtools MIT). MIT requires the copyright/permission notice travel with the binary; GPL requires the source-offer / license for the GPL components. Decide and document the offer-of-source posture for the bundled Bowtie 2 (link to upstream tagged source at the pinned version is the simplest compliant route).

---

## Important findings (will cause confusion, rework, or a broken first release if unaddressed)

### I-1 — D1 "hybrid" versioning: the `LOCK_VERSION` assert has no single package to key on, and flattening to `version.workspace = true` silently erases divergent crate lineages
TG's lock-assert is `awk '/^name = "trim-galore"/{...}'` — it works because there's ONE package whose name is the project. The suite has **13 packages and no package named `bismark-rust`**. The plan's §4a.4 copies "asserts `Cargo.lock` == `workspace.package.version`" without saying **which crate's lock entry** is the source of truth. If all 12 bins inherit `version.workspace = true`, they all share the suite version, so any one of them works as the assert key — but the SPEC must pick one explicitly and the assert regex must change from TG's. Also: `bismark-io` (the override) needs its *own* separate lock-assert if it's published, against its independent version.

Separately, the moment §4a.5 sets `version.workspace = true` on the 12 bins, the **independent versions are erased**: dedup loses its `1.2.x` lineage, the aligner loses `alpha.1` (it's the least-mature crate — folding it into a shared "beta" or "2.0" suite version overstates its maturity). The plan's D1 resolution never states (a) the **initial suite version number**, (b) that this discards dedup 1.2.x / aligner alpha, or (c) whether `--version` on each binary now reports the suite version + the `build.rs` git hash rather than a per-tool semver. Decide and write it down.

### I-2 — Releasing from `rust/iron-chancellor` defeats TG's `master`-GA / `dev`-prerelease guard, and the branch has none of the infra
TG's safety model (`release.yml` lines 75–82) hard-codes: GA only from `master`, prerelease only from `dev`. The plan (OQ-B) cuts from `rust/iron-chancellor`, a long-lived integration branch that is **neither**. If the plan ports TG's branch guard verbatim, the workflow will **refuse to run** on `iron-chancellor` (it's not `master` or `dev`). So the plan must *rewrite* that guard — but the rewrite is exactly the safety check, and the plan doesn't acknowledge it needs changing. Options to specify:
- Add `rust/iron-chancellor` (or a dedicated `rust-release` branch) to the allowed-branch list, with a prerelease-only constraint (the suite version must contain `-` during the beta track) mirroring TG's `dev` rule.
- The tag-collision guard (`git tag -l "$VERSION"`) and the "never push v* manually; workflow owns the tag" invariant (TG header) are **load-bearing** and must be preserved. The plan mentions "workflow-owned tag" — good — but should explicitly carry over the half-created-release warning, especially since cutting from an active integration branch means more humans are pushing to it.

Mitigant I verified: `rust_ci.yml` DOES run on every push touching `rust/**` (including `iron-chancellor`), so the "commits were already CI-validated" assumption TG relies on partly holds. **But** the `perl-oracle` byte-identity job only runs **10 oracle tests across 2 crates** (genome-prep + methylation-consistency) — it does NOT byte-validate the other 10 tools per release. The plan inherits TG's "release cuts from already-validated commits" posture without noting that the Bismark CI's validation coverage is far narrower than TG's (whose `validation` job runs ~25 byte-identity scenarios). Flag this gap; consider broadening the oracle gate before treating any cut as "validated."

### I-3 — Beta→GA `_rs`→canonical rename collides head-on with installed Perl binaries of the same names; the "Perl frozen as legacy" mechanism is undecided AND already contradicts the rust/README
§6 says at GA the `_rs` binaries take the canonical Perl names (`bismark`, `deduplicate_bismark`, …) and "Perl Bismark v0.25.x is frozen — a final tagged legacy release." Two problems:

1. **The transition is a PATH footgun for users with both installed.** During beta, `_rs` coexists safely. At GA, a user who has Perl Bismark on PATH and installs the Rust `bismark` 2.0 now has **two `bismark` on PATH** — whichever comes first wins, silently. The plan asserts "byte-identity makes it a drop-in" but a *drop-in replacement* and *two binaries with the same name on one PATH* are different problems. The SPEC needs a concrete migration story (the Docker image sidesteps it; the tarball/bioconda install does not).

2. **"Frozen as legacy" has no concrete mechanism in the plan AND conflicts with the existing repo doc.** The plan says "a final tagged legacy release" (a git tag). But `rust/README.md` already documents a *different* mechanism: "the Perl scripts move to a `legacy/` directory." Tag vs directory-move vs branch are mutually-exclusive choices with different consequences (which becomes the repo *default branch*? does `master` carry Perl or Rust at GA?). The plan must reconcile with `rust/README.md` and pick one. This is exactly the kind of "which is the repo default" question that, left open, produces an inconsistent GA.

### I-4 — The plan smoke-tests like TG (one binary), but the suite has 12 — a per-binary smoke gate is the whole point of a suite tarball
TG's `release.yml` smoke job runs `trim_galore --help`/`--version` on the **one** linux binary. The plan §4a.4 says "smoke tests (`--help`/`--version` per bin)" — good intent — but §3.1 also says it builds one suite tarball, and the plan never specifies the smoke job iterates all 12 (and on which targets). A suite tarball where **one of 12 bins is missing or segfaults on `--help`** would still pass a TG-shaped single-binary smoke test and ship broken. Specify: smoke-test **all 12** binaries (`--help` AND `--version`) from the actual packaged tarball, on at least linux-x86_64, and the Docker image's 12 bins + the bundled `bowtie2 --version` / `samtools --version` (to catch a missing/unpinned aligner — ties to C-2).

### I-5 — `build.rs` "adopted verbatim" cannot be workspace-shared as TG has it; provenance per-binary needs a plan
TG's `build.rs` lives in a single-crate root and stamps `env!("VERSION_BODY")` into the one binary. A **workspace has no workspace-level `build.rs`** — `build.rs` is per-package. §4a.1 says "`rust/build.rs` (workspace-shared) … wire into every binary's `--version`." There is no such thing as a workspace `build.rs` that Cargo runs for all members. Options the SPEC must pick:
- A small shared crate (e.g. `bismark-build-meta`) that each binary takes as a build-dependency / re-exports the env stamps; or
- A per-crate `build.rs` (12 copies, or a symlink/include) — duplicative; or
- A workspace `build-dependencies` pattern via a shared module.

Also: the reproducibility (`reproduce`) target and the `SOURCE_DATE_EPOCH` bit-identity check must contend with the **mimalloc C build** (`cc`) — C compilers are a classic source of non-reproducibility (embedded paths, `__DATE__`). TG's `reproduce` worked because it was pure-Rust. The plan claims bit-identity "verbatim from TG" — verify mimalloc doesn't break it before promising a `reproduce` target, or scope the bit-identity claim to exclude the mimalloc-linked binaries.

---

## Optional / smaller findings

- **O-1 — `bismark-io` has no `exclude` field** (TG excludes `test_files/`, `plans/`, `docs/`, `.github/`, `CHANGELOG.md`). If you do publish it (against C-1's advice), add `exclude` so the `test_files/` + `tests/` + `DESIGN.md` don't bloat the crate. (Current crate dir is only 356K, so not urgent — but it's a publish-hygiene gap the "crates.io-ready" claim overlooks.) Also `bismark-io` sets `repository` but not `homepage`/`documentation` (TG sets `homepage`) — cosmetic.
- **O-2 — unsigned macOS aarch64 binaries → Gatekeeper quarantine.** The plan ships `macos-aarch64` tarballs but never mentions that an unsigned, un-notarized binary downloaded via browser gets the `com.apple.quarantine` xattr and refuses to run without a right-click-open or `xattr -d`. TG has the identical issue, so it's "template parity," but the suite has 12 binaries to clear, multiplying the friction. At minimum document the `xattr -dr com.apple.quarantine <dir>` workaround in the release notes / install docs. (Codesigning is GA-tier; just acknowledge it.)
- **O-3 — extractor's path-only deps on bedgraph/c2c** (`{ path = "..." }`, no version) are fine for the workspace and do NOT block a `bismark-io`-only publish (bismark-io depends on neither). But note for the GA full-crates.io publish: path-only deps **cannot** be published — every inter-crate dep needs a `version` at that point. Out of scope now, but the SPEC's GA section should flag it so it isn't a surprise.
- **O-4 — Docker layer-cache trick won't work for a 13-crate workspace as-is.** TG's Dockerfile's "copy manifests, `echo 'fn main(){}'`, build deps, then copy real source" cache trick assumes one `src/main.rs`. A 13-member workspace needs all 13 manifests + 13 dummy `src/` stubs copied first for the dep-cache layer to be valid. Minor, but "adopt TG's Dockerfile" → it will not cache correctly without rework.

---

## Validation sufficiency assessment

The plan's own validation surface (what it proposes to *check* before shipping) is **thin for the highest-risk failure modes**:
- **No dry-run-first requirement is stated** even though TG's whole safety model centers on `dry_run: true`. The plan mentions dry-run as a feature to port but does not mandate "first real exercise = a dry-run from `iron-chancellor`." Make the first milestone a green dry-run.
- **No check that the published `bismark-io` version is consumable** — e.g., a post-publish job that creates a throwaway crate depending on the published `=X` and builds it. Without this, a broken/incomplete publish (C-1) is discovered by an external user, not by CI.
- **Smoke = `--help`/`--version` only.** That catches "binary missing / won't start," not "binary runs but the bundled aligner is the wrong version" (C-2). A minimal end-to-end smoke (align a 10-read toy against a tiny index inside the image, diff against a checked-in expected) is the only thing that catches the unpinned-aligner footgun. Strongly recommend for the Docker smoke job.

---

## Alternatives worth weighing

1. **Defer `bismark-io` from crates.io entirely during beta (D2 → (a) for the lib too).** Cleanest resolution of C-1; consistent with deferring bioconda+full-crates.io to GA. The lib is path-consumed internally; no external consumer needs the beta. Publish it *once*, cleanly, at GA when the API is actually frozen.
2. **Cut releases from a dedicated short-lived `rust-release` branch off `iron-chancellor`, not `iron-chancellor` itself.** Lets you keep TG's branch-guard shape (a named release branch with a prerelease-only rule) without weakening the guard to "any branch," and isolates the release commit (version bump + lock) from ongoing integration churn.
3. **Per-aligner *variant* Docker images keyed to the pinned aligner version** (D5 option (c)) rather than one "batteries" image — e.g. `bismark-rust:beta-bowtie2.5.5`. Makes the byte-identity-vs-aligner-version contract explicit in the tag, which is exactly the reproducibility property the project sells.

---

## Action items (prioritized)

**Critical (resolve in SPEC before building any infra):**
- **C-1** Decide `bismark-io` crates.io policy that respects immutability: either (a) every additive change = real bump + synchronized 8-crate pin bump (drop the no-bump convention at publish time), or (b) defer `bismark-io` to GA. Do NOT "publish beta.8 and keep current."
- **C-2** Pin Bowtie 2 to **2.5.5** (and samtools to a fixed version) in the Dockerfile; do not `apt-get install bowtie2`. Record pins in labels + README. Add an end-to-end Docker smoke that diffs a toy alignment.
- **C-3** Add/route a `LICENSE` into every tarball + image; include bundled-aligner license + copyright notices and a source-offer posture for GPL Bowtie 2.

**Important:**
- **I-1** Specify the suite's initial version number, which crate keys the `LOCK_VERSION` assert (and a separate assert for `bismark-io` if published), and acknowledge that `version.workspace = true` erases dedup 1.2.x / aligner alpha lineages.
- **I-2** Rewrite (don't verbatim-copy) TG's branch guard for `iron-chancellor`/a release branch with a prerelease-only constraint; preserve the tag-collision + workflow-owned-tag invariants. Note CI's narrow byte-identity coverage (2 crates) vs the "validated commits" assumption.
- **I-3** Reconcile §6 "frozen tagged legacy" with `rust/README.md` "Perl moves to `legacy/` dir"; pick one mechanism; decide the GA default branch and the same-name-on-PATH migration story.
- **I-4** Smoke-test ALL 12 binaries (`--help` + `--version`) from the packaged tarball, plus the image's bundled `bowtie2`/`samtools --version`.
- **I-5** Replace "workspace-shared `build.rs`" with a concrete per-binary provenance mechanism; verify mimalloc's `cc` build doesn't void the `reproduce` bit-identity claim.

**Optional:**
- **O-1** Add `exclude` + `homepage` to `bismark-io` if published.
- **O-2** Document the macOS Gatekeeper `xattr` workaround for the 12 unsigned binaries.
- **O-3** Note that GA full-crates.io publish requires versions on the extractor's path-only bedgraph/c2c deps.
- **O-4** Rework the Docker dep-cache layer for a 13-member workspace.

---

### Highest-risk single finding
**C-1 (immutable publish).** crates.io publishes cannot be undone; the plan proposes publishing `bismark-io` beta.8 while the project's own documented convention treats beta versions as mutable, "additive-without-bump" — these are mutually exclusive, and the plan's "keep current / automate publishes" answer institutionalizes the contradiction. The next additive change strands the convention against an immutable registry and forces an unplanned bump cascade across 8 exact-pinning crates. Resolve before any publish step is wired.
