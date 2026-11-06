# Code Review A — #1152 docker wrapper version

Target: uncommitted diff in `/Users/fkrueger/Github/Bismark-1152` (branch `fix/1152-docker-wrapper-version`, base dev `da431b7`): `Dockerfile`, `docker/bismark-canonical-wrapper.sh`, `CHANGELOG.md`.

## Summary

**Verdict: APPROVE.** I found no Critical, High or Medium issues. The new runtime-stage `RUN` is correct under the base image's real shell. The sed regex matches the real banner, and the removed runtime `ARG` has no other consumer. The release smoke test still holds. The comments and the CHANGELOG entry follow the house rules. There are three optional Low nits.

## Verification performed

- **Base image shell.** `docker image inspect bismark:1152` reports `Shell = ["/usr/local/bin/_dockerfile_shell.sh"]`. That script runs `set -ef -o pipefail` itself (no `-u`), then `exec bash -o pipefail -c "$@"`.
  - So the `RUN` body runs under **bash with `pipefail`**, plus the body's own `set -eu`.
  - `noglob` (`-f`) is not inherited, because `SHELLOPTS` is not exported. Nothing in the body depends on globbing anyway.
- **The banner.** `meta::version_line` formats `"{tool} (Bismark Rust suite) v{SUITE_VERSION} ({body})"`. `cli::dispatch` prints `version_line("bismark")` for `--version`/`-V`.
  - Invoked as `/usr/local/bin/bismark.bin`, `argv[0]`'s basename `bismark.bin` is not a classic alias, so it reaches the `--version` arm.
  - `.git/` is in `.dockerignore`, so the build is hashless and the body is `linux/<arch> — built …`. A space always follows the version.
- **Live check on the local images:**
  - `bismark:1152`: `bismark.bin --version` gives `bismark (Bismark Rust suite) v3.1.0 (linux/aarch64 — built 2026-11-05T18:27:26Z)`, and `bismark -v` gives `Bismark Version: v3.1.0` + Copyright + URL.
  - `bismark:1152-beta`: both report `3.2.0-beta.1`.
- **Who used the runtime `ARG`.** I grepped the repo, excluding `target/`, `plans/` and `.git`.
  - `release.yml:113,263` and `ga-candidate-image.yml:42` pass `--build-arg BISMARK_SUITE_VERSION`. The builder-stage `ARG` (`Dockerfile:17`) still consumes it, so buildx raises no "unused build-arg" warning and the binary still gets the release version.
  - No docs mention the runtime ARG. `wiki/usage/install.md:23,41` only references the wrapper's path, which is still correct.
- **Release smoke test** (`release.yml:316-351`). `EXPECT` is the release version, and the builder ARG sets `bismark.bin`'s version to that same value. The wrapper now copies its version from `bismark.bin`, so `p1`/`p2` still equal `EXPECT`.
  - The plan's validation check 5 ran these exact commands with `3.2.0-beta.1`.
  - Both `docker-build` matrix legs use native runners (`ubuntu-latest`, `ubuntu-24.04-arm`), so running the binary at build time needs no QEMU.

## Issues by area

### Logic / shell correctness (`Dockerfile:92-102`)

| Line | Construct | Assessment |
|---|---|---|
| 94 | `banner=$(/usr/local/bin/bismark.bin --version \|\| true)` | Correct. `\|\| true` keeps `set -e` from aborting with no message, so the explicit guard on line 96-97 fires with the banner shown. If the binary cannot run at all, its stderr still reaches the build log. |
| 95 | `sed -n 's/^bismark (Bismark Rust suite) v\([^ ]*\) .*/\1/p'` | Correct BRE: the parentheses are literal and `\(…\)` is the group. It is anchored on the tool name `bismark`, which `cli.rs` hard-codes. With no match, `v` is empty and the pipeline still exits 0 (sed's status), so the guard catches it. |
| 96 | `printf '%s\n' "$v" \| grep -Eqx '…'` under `pipefail` | Safe. `printf` writes one short buffer before `grep -q` can exit, so it cannot get SIGPIPE. It rejects `unknown`, an empty value and Perl-style banners (plan check 4). The allowed character set `[0-9A-Za-z.-]` keeps `\|`, `&` and `\` out of the sed replacement on line 98, so the substitution cannot be injected. |
| 98 | `sed "s\|__SUITE_VERSION__\|$v\|"` | Safe, given the validation above. |
| 101-102 | `[ "$(/usr/local/bin/bismark -v \| head -n1)" = "Bismark Version: v$v" ]` | Correct. The command substitution's exit status is irrelevant inside `[ ]`. `head -n1` exiting early cannot cause SIGPIPE, because the wrapper emits a single small `printf`. It catches a template that lost its placeholder or a `case` arm that broke (plan check 4c). |

`set -eu` semantics: `set -e` does not fire for a failure inside a `||` list, a command substitution inside an assignment's `|| true`, or a `[ ]` test, which is the intent here. `set -u` only matters for `$banner` and `$v`, and both are always assigned.

**The banner is coupled to the Dockerfile.** If `version_line`'s format ever changes, the image build fails loudly rather than silently, which is the right direction. `meta::tests::version_line_shape` already asserts `starts_with("bismark (Bismark Rust suite) v")`, which partly guards the shape at test time.

### Efficiency

One extra binary invocation and one wrapper invocation at build time. Negligible.

### Errors / edge cases

- **Version strings with `+build` metadata** (e.g. `3.2.0+local`) or underscores would fail the build. `build.rs::suite_version` accepts any non-empty string. Today `rust/VERSION` only ever holds `X.Y.Z` or `X.Y.Z-pre.N`, and failing loud is preferable to writing a malformed banner. Low, no action needed.
- **Cross-arch local builds** (`--platform` foreign to the host) now run `bismark.bin` under emulation. This adds no new requirement: the existing `micromamba install` step already runs emulated binaries in this stage.

### Structure / style

- **`Dockerfile:91`** (`# The wrapper's version is read from \`bismark.bin --version\`, so the two always agree.`): one line, describes the current state only, and is placed above the thing it constrains. Good.
- **`Dockerfile:82-88` edits:** accurate. The banner shape quoted, `bismark (Bismark Rust suite) v<suite> (…)`, matches `version_line`. The obsolete `Bismark Aligner (Rust port) Version` claim is gone.
- **`docker/bismark-canonical-wrapper.sh:10-17`:** accurate and shorter. The beta-era/GA/OD-7 history narrative is removed, in line with the "current state only" rule.
- **`CHANGELOG.md:12`:** one sentence, in the file's existing bold-sentence + issue-link format. It sits under `### All tools`. Acceptable, although only the container changes.

## Fixes applied

None. This was a read-only review.

## Recommendations

### Critical
None.

### High
None.

### Medium
None.

### Low
1. **`Dockerfile:102`:** the wrapper-mismatch message does not show what the wrapper actually printed. Showing it would speed up diagnosis, e.g. capture `w=$(/usr/local/bin/bismark -v | head -n1)` and echo `'$w'`, the same way line 97 echoes `'$banner'`. Optional.
2. **`docker/bismark-canonical-wrapper.sh:19`** (pre-existing, not in the diff): "execs the unchanged real aligner" should say "the real binary", to match the multicall reality and the Dockerfile's edited wording on line 87. The diff already touches this header, so this is optional consistency.
3. **`.github/workflows/release.yml:338-339`** (pre-existing, out of scope): the smoke-test comment "at the 3.0.0 cut this is \"3.0.0\" … OD-7" is historical narrative. Leave it for a separate tidy-up rather than widening this PR.
