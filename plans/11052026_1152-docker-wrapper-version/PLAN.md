# Docker `bismark` wrapper reports the binary's version (#1152)

- **Rev 1 (2026-11-05):** folds in the dual plan review (`PLAN_REVIEW_A.md`, `PLAN_REVIEW_B.md`); the changes are listed under "Revision notes".
- **Rev 0 (2026-11-05):** approved by Felix with the Q1/Q2 defaults.

## Goal

A container built with a plain `docker build .` (no `--build-arg BISMARK_SUITE_VERSION=…`) prints the true suite version from `bismark -v` / `bismark --version`, not `vunknown`. The wrapper's version comes from the binary it wraps, so the two can never disagree, whatever build arguments are used.

Issue: [#1152](https://github.com/FelixKrueger/Bismark/issues/1152), filed after #1148. Worktree: `/Users/fkrueger/Github/Bismark-1152`, branch `fix/1152-docker-wrapper-version`, from `dev` at `da431b7`.

## Context

- **`Dockerfile`, builder stage (lines 14–18):**
  - `ARG BISMARK_SUITE_VERSION=unknown` + `ENV`;
  - `build.rs` `suite_version()` reads that variable. Since #1148 it treats `unknown` as unset (`build.rs:114-138`) and falls back to `rust/VERSION`. `COPY . .` brings that file in, and `.dockerignore` does not exclude it.
  - So a plain build's binary already reports `v3.1.0`.
- **`Dockerfile`, runtime stage (lines 91–96):**
  - repeats `ARG BISMARK_SUITE_VERSION=unknown`;
  - `sed`s it into `docker/bismark-canonical-wrapper.sh` as `__SUITE_VERSION__`;
  - installs the result as `/usr/local/bin/bismark`.
  - On a plain build that gives `Bismark Version: vunknown`. Nothing else in the runtime stage uses the argument.
- **Wrapper contract (`docker/bismark-canonical-wrapper.sh`):** methylseq's two parsers must capture the bare suite version:
  - Parser-1: `bismark --version | grep Version | sed -e 's/Bismark Version: v//' | xargs`
  - Parser-2: `echo $(bismark -v 2>&1) | sed 's/^.*Bismark Version: v//; s/Copyright.*$//'`

  The release smoke test (`release.yml:345-349`) asserts that both parsers give the release version, after the image has been pushed.
- **The binary's banner:**
  - `bismark.bin --version` is handled by the multicall dispatcher's own `--version` branch (`rust/bismark/src/cli.rs:159-162`). It prints `meta::version_line("bismark")` to stdout and exits 0.
  - The shape is `bismark (Bismark Rust suite) v<SUITE_VERSION> (<body>)` (`meta/mod.rs:42`). A Docker build has no `.git` (excluded by `.dockerignore`), so `<body>` is `<target> — built <ts>`, with no hash.
  - `meta::tests::version_line_shape` (`meta/mod.rs:64-78`) pins the `bismark (Bismark Rust suite) v` prefix that this plan parses.
- **Base image:**
  - `mambaorg/micromamba:1.5.8-bookworm-slim` (Debian), so `sed` is GNU sed.
  - Its environment puts `/opt/conda/bin` first on `PATH`, so the plan calls `/usr/local/bin/bismark.bin` and `/usr/local/bin/bismark` by absolute path.
- **Release builds:**
  - `release.yml` (each architecture on its own native runner, `:240-246`) and `ga-candidate-image.yml` (amd64 on amd64) pass the build argument.
  - The wrapper is correct there today, and this change keeps what it prints there identical.

## Behavior

1. **Version from the binary.** In the runtime stage, after `bismark.bin` is installed, read the wrapper's version from `/usr/local/bin/bismark.bin --version`: the token after `(Bismark Rust suite) v`, up to the next space.
2. **Shape check.** Fail the build unless that token matches `X.Y.Z` with an optional `-<pre>` (`[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?`). This rejects:
   - an empty token (the banner shape changed, or `bismark.bin` failed);
   - `unknown`;
   - any character that would corrupt the `sed` replacement (`&`, `\`, `|`) or the wrapper's `printf` (`%`).

   The error message includes the actual banner.
3. **Output check.** After writing the wrapper, fail the build unless the first line of `/usr/local/bin/bismark -v` is exactly `Bismark Version: v<token>`. This catches a renamed or missing `__SUITE_VERSION__` placeholder.
4. **Remove the runtime-stage `ARG BISMARK_SUITE_VERSION`.** The builder-stage `ARG`/`ENV` stays; it is how release builds pin the version.
5. **Outputs:**
   - **plain build:** the wrapper prints `Bismark Version: v3.1.0`, both parsers give `3.1.0`, and that equals `bismark.bin --version`.
   - **`--build-arg BISMARK_SUITE_VERSION=<v>`:** the wrapper prints `v<v>`, exactly as today.
   - **prerelease, e.g. `3.2.0-beta.1`:** the wrapper and the binary both print `v3.2.0-beta.1`.
6. **Unchanged:** the wrapper script's runtime behaviour (the `case` on `-v`/`--version`, the `exec bismark.bin "$@"`), every other image layer, and the release workflows.

## Implementation outline

1. **`Dockerfile`, runtime stage.** Replace the `ARG` + `sed` block with:
   ```dockerfile
   COPY docker/bismark-canonical-wrapper.sh /tmp/bismark-wrapper.sh
   RUN set -eu; \
       banner=$(/usr/local/bin/bismark.bin --version || true); \
       v=$(printf '%s\n' "$banner" | sed -n 's/^bismark (Bismark Rust suite) v\([^ ]*\) .*/\1/p'); \
       printf '%s\n' "$v" | grep -Eqx '[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?' \
           || { echo "cannot read a suite version from bismark.bin --version: '$banner'" >&2; exit 1; }; \
       sed "s|__SUITE_VERSION__|$v|" /tmp/bismark-wrapper.sh > /usr/local/bin/bismark; \
       chmod +x /usr/local/bin/bismark; \
       rm /tmp/bismark-wrapper.sh; \
       [ "$(/usr/local/bin/bismark -v | head -n1)" = "Bismark Version: v$v" ] \
           || { echo "the bismark wrapper does not report v$v" >&2; exit 1; }
   ```
2. **Comments, current state only:**
   - **Dockerfile block comment:** rewrite the sentence about the aligner's `Bismark Aligner (Rust port) Version: …` shape (lines 79–80). The point is that the binary's banner is not the `Bismark Version: v…` shape methylseq parses. Add one line: the wrapper's version is read from `bismark.bin --version`.
   - **Wrapper header** (`docker/bismark-canonical-wrapper.sh`):
     - lines 10–11: the binary's own `--version` is the suite banner `bismark (Bismark Rust suite) v<v> (…)`, not the methylseq shape;
     - lines 14–18: drop the 3.0.0-cut narrative;
     - line 19: `__SUITE_VERSION__` is filled in from `bismark.bin --version` at image build.
3. **CHANGELOG:** one sentence under `### All tools` in Unreleased, linking #1152.
4. **No Rust, test or workflow changes.**

## Efficiency

One `bismark.bin --version` call and one `bismark -v` call at image build (milliseconds); no runtime cost.

## Integration

- **Changes:** what the wrapper prints on builds without the build argument; the image build now fails loudly on a malformed version or wrapper.
- **Release images:** unchanged, because the builder argument still sets the binary's version and the wrapper now reads it from there.
- **Downstream:** methylseq parsers are unaffected; the banner text is the same.

## Assumptions

- The banner shape is `bismark (Bismark Rust suite) v<ver> (…)`, pinned by `meta::tests::version_line_shape`. If it changes, Behavior 2 fails the image build rather than shipping a wrong wrapper.
- `bismark.bin` runs in the runtime stage. Release builds are native per architecture. Only ad-hoc local multi-arch builds would run it under QEMU, which is slower but works.
- Suite versions are `X.Y.Z[-pre]`, with `pre` made of `[0-9A-Za-z.]`. Behavior 2 enforces this.

## Validation

1. **Plain local build:**
   - **How:** `docker build -t bismark:1152 .` on this Mac (Docker 27, aarch64), then run `bismark -v`, `bismark --version`, `bismark.bin --version` and both methylseq parser one-liners.
   - **Expected:** the wrapper prints `Bismark Version: v3.1.0`; Parser-1 = Parser-2 = `3.1.0`; the same version as `bismark.bin --version`.
2. **Build with an explicit argument:**
   - **How:** `docker build --build-arg BISMARK_SUITE_VERSION=3.2.0-beta.1 -t bismark:1152-beta .`. The argument changes the builder's `ENV`, so cargo rebuilds from scratch (a second full compile).
   - **Expected:** the wrapper and the binary both say `v3.2.0-beta.1`, and both parsers give `3.2.0-beta.1`. This is the only check that shows the wrapper followed the binary rather than `rust/VERSION`.
3. **Wrapper still execs the binary:** `docker run … bismark --help` exits 0 and prints the suite's composed help (`cli.rs:149-151`).
4. **Guard arms, run inside the built image with its GNU sed:**
   - **How:** run the `RUN` script body under `sh -eu` against stubs that print:
     - (a) a different banner shape;
     - (b) `bismark (Bismark Rust suite) vunknown (…)`;
     - (c) a valid banner, with a wrapper template that lacks `__SUITE_VERSION__`.
   - **Expected:** (a) and (b) exit 1 with the version message, and (c) exits 1 with the wrapper message. Also run the unmodified body against the real binary and template: it exits 0.
   - **Record:** in the notes.
5. **Release smoke equivalence:** run `release.yml`'s exact `p1`/`p2` commands against the image from check 2, with `EXPECT=3.2.0-beta.1`. Both match.

## Questions or ambiguities

- **Q1 (Resolved 2026-11-05, Felix approved the default):** the CHANGELOG entry goes under `### All tools`.
- **Q2 (Resolved 2026-11-05, Felix approved the default):** no runtime-stage override; the wrapper always uses the binary's version.

## Self-Review

- **Logic:**
  - The version has one source, the binary.
  - Both the input (the version token) and the output (the written wrapper) are checked inside the same `RUN`.
  - The builder argument still controls releases.
- **Edge cases:**
  - a failing binary, an empty or `unknown` version, unsafe characters: Behavior 2;
  - a missing placeholder: Behavior 3;
  - prerelease strings: Validation 2.
- **Integration:** release images print the same as before, and the release smoke test still covers them on the next cut.
- **Remaining risk:** the local build validates aarch64 only; amd64 is exercised by the release workflow.

## Revision notes (rev 0 → rev 1)

| Review item | Source | Change |
|---|---|---|
| `--version` comes from the dispatcher, not the aligner | A-I1, B-O3 | Context now cites `cli.rs:159-162`. |
| Call the binary by absolute path (`/opt/conda/bin` comes first on `PATH`) | A-O2, B-O2 | Absolute paths in step 1. |
| Check the written wrapper | B-I1 | New Behavior 3; Validation 4(c) tests it. |
| Validate the version's shape (`&`, `\`, `|`, `%`) | B-O1, A-O5 | Behavior 2 uses a regex in place of `""\|unknown`. |
| Run the guard test in-image, with both arms | A-I2 | Validation 4 now has three stub cases and runs in the image. |
| Show the actual banner on failure | A-O1 | Included in the error message. |
| Stale wrapper and Dockerfile comments | A-O4, B-O4 | Step 2 covers wrapper lines 10–19 and Dockerfile lines 79–80. |
| Releases build natively per architecture; cite `version_line_shape` | A-O5, A-O6, B-O3 | Context and Assumptions. |
| `bismark --help` shows the composed suite help | B-O3 | Validation 3 wording. |
| Release-smoke check that wrapper = binary | A-O3 | Not adopted: Behavior 3 checks it at every image build. |

## Implementation notes (2026-11-05)

Implemented rev 1 as written: the `Dockerfile` runtime-stage block, comments in the `Dockerfile` and `docker/bismark-canonical-wrapper.sh`, and the CHANGELOG entry under `### All tools`. Uncommitted on `fix/1152-docker-wrapper-version`.

### Deviation

1. **The validation images were built from a copy of the `Dockerfile` with only the `micromamba install` step (bowtie2, hisat2, minimap2) removed.** This laptop's network intercepts TLS, so `conda.anaconda.org` fails inside Docker (`SSL certificate problem: unable to get local issuer certificate`). The full build failed at that step, before reaching the change.
   - The step is unchanged by this PR, and the wrapper does not use those tools.
   - Everything else is byte-identical to the real `Dockerfile`, including the new `RUN`, the `ENV PATH=/opt/conda/bin:$PATH` line, the base image and the builder stage.
   - The full `Dockerfile` is exercised by the release workflow on the next cut.

### Validation results (Docker 27.4, aarch64)

| # | Check | Result |
|---|---|---|
| 1 | Plain build (no build argument) | `bismark -v` gives `Bismark Version: v3.1.0`; Parser-1 = Parser-2 = `3.1.0`; `bismark.bin --version` gives `bismark (Bismark Rust suite) v3.1.0 (linux/aarch64 — built …)`. PASS |
| 2 | `--build-arg BISMARK_SUITE_VERSION=3.2.0-beta.1` | the wrapper gives `v3.2.0-beta.1`, the same as the binary. PASS |
| 3 | `bismark --help` | exit 0, composed suite help. PASS |
| 4 | Guard arms: the exact `RUN` body, extracted from the `Dockerfile` and run in the image (GNU sed) against stubs | (a) Perl-style banner: exit 1, version message. (b) `vunknown`: exit 1, version message. (c) template without `__SUITE_VERSION__`: exit 1, wrapper message. (d) `v3.1.0&x`: exit 1, version message. Valid prerelease stub: exit 0. Real binary and template: exit 0, `Bismark Version: v3.1.0`. PASS |
| 5 | `release.yml`'s exact `p1`/`p2` commands on the image from check 2, `EXPECT=3.2.0-beta.1` | both match. PASS |

### Iteration log

- #1: The first plain build failed at the micromamba step on laptop TLS interception. Rebuilt from the validation copy described above.
- #2: The first harness run could not see its files, because Docker Desktop does not share the scratchpad under `/private/tmp`. Streamed the files in with `tar` instead.

### Review fixes (2026-11-06, after `CODE_REVIEW_A.md`, `CODE_REVIEW_B.md`, `COVERAGE.md`)

Both reviewers approved, with no Critical/High/Medium findings, and the coverage audit is COMPLETE. Three Low items were applied:

- **`docker/bismark-canonical-wrapper.sh`:**
  - "execs the unchanged real aligner" → "execs the real binary" (both reviewers);
  - the 3.1.0 parser example → `<v>`, so it does not go stale (B).
- **`Dockerfile`:** the wrapper-mismatch error now shows the wrapper's actual first line (A).

Re-checked after the fixes:
- the validation image rebuilt, and `bismark -v` gives `v3.1.0`;
- the guard harness, regenerated from the edited `Dockerfile`, gives the same results. Case (c) now prints `'Bismark Version: vXX'`.

Not changed:
- `release.yml:338-339`'s 3.0.0/OD-7 comment, which is outside this PR;
- `+build` SemVer, which the shape check rejects loudly and which is not used;
- a CHANGELOG credit: #1152 was filed by the maintainer.
