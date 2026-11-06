# Plan Review A — #1152 Docker wrapper version (PLAN.md rev 0)

Reviewer A, 2026-11-05. Read-only review against the worktree `/Users/fkrueger/Github/Bismark-1152` (`fix/1152-docker-wrapper-version` at `da431b7`). No images built.

**Verdict: sound, approve with minor corrections.** The approach (derive the wrapper's version from the binary it wraps, fail the build otherwise) is the right single-source design, and the proposed `RUN` is correct for the real banner. No Critical findings. Two Important items are factual/robustness fixes to the plan text and the guard; the rest are optional.

---

## 1. Logic review

### 1.1 Claims verified

| Plan claim | Evidence | Status |
|---|---|---|
| Builder `ARG`/`ENV` feed `build.rs`; `unknown` counts as unset; falls back to `rust/VERSION` | `Dockerfile:17-18`; `rust/bismark/build.rs:114-138` (`v != "unknown"`, then `../VERSION`) | Correct |
| `rust/VERSION` reaches the build context | `.dockerignore` excludes `**/*.md`, `plans/`, `.git/` etc., not `rust/VERSION`; `Dockerfile:29` `COPY . .`; `rust/VERSION` = `3.1.0` | Correct |
| Runtime `ARG` only used for the wrapper `sed` | `Dockerfile:91-96`; repo-wide grep finds no other runtime-stage use | Correct |
| Banner shape `bismark (Bismark Rust suite) v<ver> (…)` | `rust/bismark/src/meta/mod.rs:49-54` (`version_line`), body `mod.rs:33-39` | Correct |
| `bismark.bin --version` prints `version_line("bismark")` | `src/bin/bismark.rs` → `cli::dispatch()`; `cli.rs` prog=`bismark.bin` is not a classic alias (`run_legacy_alias` returns `None`), argv len 2, `raw[1]=="--version"` → `println!("{}", meta::version_line("bismark"))` (`cli.rs:159-162`) | Correct output, **but the routing description is wrong** — see 1.2 |
| Output on stdout | `println!` in `cli.rs:160` | Correct; the `$( … )` captures it without `2>&1` |
| Release builds pass the build arg | `release.yml:262-263`, `ga-candidate-image.yml:41-42` | Correct |
| Release smoke test asserts both parsers | `release.yml:344-349` | Correct |

### 1.2 Routing description is inaccurate (Important, text only)

PLAN.md Context says "Run as `bismark.bin`, the multicall binary routes to the aligner. The wrapper relies on the same routing when it `exec`s it." For `--version` that is not what happens: `cli::dispatch` intercepts `-V`/`--version` as `argv[1]` at `rust/bismark/src/cli.rs:159-162` and prints the suite banner itself; the aligner's own `--version` path (`aligner/mod.rs:140-142`, `version_string()` at `:106-108`) is only reached via `bismark align --version`. Both print `version_line("bismark")`, so the extracted value is identical — but the plan should cite `cli.rs:159-162` as the source, since that is the code a future change to the banner would touch. (The wrapper relies on the aligner fall-through for *non-version* args, `cli.rs:163-170`, which is a separate path.)

### 1.3 The `sed` expression against the real banner

`s/^bismark (Bismark Rust suite) v\([^ ]*\) .*/\1/p` with `sed -n`:

- BRE: `(` and `)` are literal; `\(…\)` is the group. Correct.
- Hashed build (`abc1234 — linux/x86_64 — built …Z`) and hashless build (`linux/x86_64 — built …Z`, `meta/mod.rs:34-35`) both put a single ASCII space after the version. The Docker context has no `.git` (`.dockerignore`), and the Dockerfile does not set `BISMARK_GIT_HASH`, so the image banner is the **hashless** form; the regex handles it.
- Em-dash/UTF-8: the runtime base is Debian bookworm-slim, so `sed` is GNU sed 4.9, not BusyBox. The em-dash is only in the part matched by the trailing `.*`. In a UTF-8 locale it is a valid multibyte character and `.` matches it; in the C/POSIX locale `.` matches every byte. Either way the substitution covers the whole line and only `\1` is printed. Note that even if `.*` stopped early, `s` with `p` would print `\1` followed by the unmatched suffix — so a garbled value is possible only with *invalid* UTF-8 in a UTF-8 locale, which the banner never contains. Safe.
- Prerelease `3.2.0-beta.1`: no spaces, `[^ ]*` captures it whole. Correct.
- `-n … p` prints only matching lines, so extra banner lines (none today) would be ignored. Correct.

### 1.4 `set -eu` and command-substitution semantics

`v=$(bismark.bin --version | sed -n …)`: the assignment's status is the pipeline's, i.e. **sed's** (no `pipefail`). If `bismark.bin` fails (exec format error, missing loader, panic), `set -e` does **not** fire; `v` is empty and the `case` catches it. So the guard, not `set -e`, is what makes it fail loud — that is fine and the plan's Behavior 2 relies on exactly that. The `unknown` arm correctly catches a banner `vunknown` (`build.rs:137` fallback). The binary's own stderr would appear in the build log above the guard's message.

The guard message is generic; when it fires the first thing anyone needs is what the binary actually printed. See Optional O1.

### 1.5 Removing the runtime-stage `ARG`

- Nothing else in the runtime stage references it (grep: `Dockerfile:91,94` only).
- The build arg is still consumed by the builder stage, so BuildKit will not warn about an unconsumed `--build-arg` in release/GA-candidate builds.
- Layer caching improves: the wrapper `RUN` is now keyed on its parent layers, which include the `COPY --from=builder` binary layer (content-checksummed), so a cached wrapper can only be reused with the identical binary. Previously the wrapper layer was keyed on the ARG value independently of the binary.

No breakage found.

### 1.6 Multi-arch / QEMU

`release.yml:240-246` builds each platform on a **native** runner (`ubuntu-latest` amd64, `ubuntu-24.04-arm` arm64) with one `platforms:` value each; `ga-candidate-image.yml:40` is amd64 on an amd64 runner. So the release path never emulates. A local `--platform linux/amd64` build on the M1 would emulate under QEMU/Rosetta, but the runtime stage already executes target-arch code (`micromamba install`, `Dockerfile:50-54`), so emulation is already a requirement — the new `RUN` adds no new constraint. The Assumptions bullet is correct but could say "release builds are native per arch" instead of implying a multi-platform buildx build.

### 1.7 Byte-identity of release output

For a release build with `BISMARK_SUITE_VERSION=X`, `build.rs` bakes `X` into `SUITE_VERSION`; the banner shows `vX`; the wrapper gets `X`. Same as today, provided `X` is non-empty, not `unknown`, and space-free — all true for `check-release.outputs.version`. Wrapper bytes identical. Correct.

---

## 2. Assumptions

| Assumption | Assessment |
|---|---|
| Banner shape stable | Stated; guarded by build failure. There is a unit test pinning the prefix (`meta/mod.rs:67`, `starts_with("bismark (Bismark Rust suite) v")`), so a banner change would also fail `cargo test` — worth citing as a second guard. |
| Version has no space or `\|` | Stated. Also needs no `&` or `\` (special in a sed replacement) and no `%` (the wrapper's `printf` format string, `bismark-canonical-wrapper.sh:29`). `X.Y.Z[-pre]` satisfies all; could add to the bullet for completeness. |
| `bismark.bin` on `PATH` | True (`/usr/local/bin` in the default PATH; `ENV PATH=/opt/conda/bin:$PATH` at `Dockerfile:55` preserves it). Using the absolute path `/usr/local/bin/bismark.bin` removes the dependency at zero cost (O2). |
| Runs as root, after the binary + symlinks | True: `USER root` at `Dockerfile:44`, binary at `:71`, wrapper block at `:91` comes after. |
| RUN shell | The micromamba base image sets its own `SHELL` (activation wrapper, bash). The plan's syntax is POSIX and the existing `RUN set -eu; …` lines already work under it, so no issue. |
| Unstated: `bismark.bin --version` is dispatcher-handled | See 1.2. |

---

## 3. Efficiency

One extra process start at image build; negligible. No runtime cost. Validation 2 costs a second full cold cargo compile (~20–30 min) because the builder `ENV` changes; that is the honest price of testing the build-arg path and is acceptable. If time matters, Validation 2 could be dropped in favour of the release dry-run (`release.yml` `docker-build` runs on `dry_run` too and builds with the arg), but that does not run the smoke test (`smoke-test-docker` is skipped on dry runs, `release.yml:322`), so the local check is worth keeping.

---

## 4. Validation sufficiency

Covered well: plain build (the bug), explicit prerelease arg (release path), wrapper exec, guard rejection, release-parser equivalence.

Gaps / notes:

- **V4 tests a stub, not the real `RUN` line.** Fine for a one-off, but make sure the stub test runs the *exact* `RUN` text copied from the Dockerfile (including `case` arms), and also exercise the `unknown` arm (stub printing `bismark (Bismark Rust suite) vunknown (…)`), not only "a different shape". Both arms are claimed in Behavior 2.
- **Under the micromamba shell.** Running the stub under the host `sh` on macOS (BSD sed) is not the same sed. Run V4 inside the built image (`docker run --rm --entrypoint sh bismark:1152 -c '…'`) so it uses GNU sed and the real locale.
- **No standing CI coverage of a plain build.** All CI builds pass the arg, so the original bug class (plain build) is never built in CI. The build-time guard now makes a `vunknown` wrapper impossible from any build path, so a regression would need someone to remove the guard; acceptable. An optional cheap smoke assertion would be that the wrapper's version equals the version parsed from `bismark.bin --version` (O3) — it costs one line in `release.yml:330-349`, but the plan says "no workflow changes", so treat as optional.
- **V1 should also record `bismark.bin --version` verbatim** so the notes show the hashless banner shape the regex was validated against.

---

## 5. Alternatives considered

1. **`COPY rust/VERSION` into the runtime stage and read it.** Simpler, but diverges from the binary when a build arg overrides the version (release prereleases), reintroducing two sources. Rejected correctly by the plan.
2. **Keep runtime `ARG`, default to binary-derived when unset/`unknown`.** Q2 resolved against it; agree — an override that can disagree with the binary is the bug class itself.
3. **`awk '{print $4}'` + strip `v`.** Shorter but positional; the anchored sed is stricter and doubles as a shape check. Plan's choice is better.
4. **Have the Rust dispatcher emit the methylseq banner when `argv[0]` is `bismark`** and drop the shell wrapper. Removes the wrapper entirely but changes the binary's `--version` output for non-container users and touches Rust; out of scope for #1152.

---

## 6. Action items

### Critical

None.

### Important

- **I1 — Correct the routing claim in Context.** `bismark.bin --version` is handled by the multicall dispatcher (`rust/bismark/src/cli.rs:159-162`), not the aligner (`aligner/mod.rs:140-142`). Same output; cite the right source.
- **I2 — Make V4 test both guard arms with the real shell + sed.** Run the exact `RUN` text inside the built image (GNU sed, image locale) with stubs for (a) a non-matching shape and (b) `… vunknown (…)`. Record both exit codes and messages.

### Optional

- **O1 — Print the observed banner on failure**, e.g. capture `b=$(/usr/local/bin/bismark.bin --version 2>&1 || true)` then derive `v` from `$b`, and on failure `echo "cannot read the suite version from bismark.bin --version: '$b'" >&2`. Makes a future banner change diagnosable from the build log alone.
- **O2 — Use `/usr/local/bin/bismark.bin`** in the `RUN` rather than relying on `PATH`.
- **O3 — One-line smoke assertion** in `release.yml` that the wrapper's version equals the version parsed from `bismark.bin --version` (only if Felix wants a standing CI guard; the plan currently says no workflow changes).
- **O4 — Wrapper header staleness.** Besides line 19, `docker/bismark-canonical-wrapper.sh:10-11` says the real binary's `--version` is `Bismark Aligner (Rust port) Version: <v>`, which no longer matches `version_line` (`meta/mod.rs:49-54`), and lines 14-17 reference the 3.0.0 cut. Since the plan already edits this header, consider fixing line 10-11 to the current banner — or explicitly leave it out of scope to keep the diff minimal.
- **O5 — Assumptions wording.** Note that release builds are native per arch (`release.yml:240-246`), and extend the version-charset assumption to `&`, `\` and `%` (sed replacement + wrapper `printf`).
- **O6 — Cite the second banner guard:** `meta::tests::version_line_shape` (`meta/mod.rs:64-78`) pins the prefix the regex depends on.
