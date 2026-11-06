# Code Review B — #1152 Docker wrapper version (uncommitted diff on `fix/1152-docker-wrapper-version`)

Reviewer B, 2026-11-05. Scope: `git diff` of `Dockerfile`, `docker/bismark-canonical-wrapper.sh`, `CHANGELOG.md`.

## Summary

**Verdict: APPROVE.** No Critical or High issues. The new runtime `RUN` is correct under the base image's real shell, the sed regex matches the banner `version_line` produces, nothing else used the removed runtime-stage ARG, and the release smoke test still holds. Four Low items, all optional. No fixes applied (read-only review).

## Evidence gathered

- **Base shell, checked in `bismark:1152`:** `Config.Shell = ["/usr/local/bin/_dockerfile_shell.sh"]`, which ends in `exec bash -o pipefail -c "$@"`. So the `RUN` body runs under **bash with `pipefail`**, plus the body's own `set -eu`. (The wrapper script's own `set -ef` applies to the wrapper process only. It does not reach the exec'd `bash -c`.)
- **Banner:** `meta::version_line` = `"{tool} (Bismark Rust suite) v{SUITE_VERSION} ({body})"`. The `--version` branch at `cli.rs:159-162` prints it for argv[0] `bismark.bin`: `bismark.bin` is not a classic alias, so dispatch reaches step 3. In the image: `bismark (Bismark Rust suite) v3.1.0 (linux/aarch64 — built 2026-11-05T18:27:26Z)`. That is the hashless form, and the regex still matches it because a space always follows the version.
- **Images:** `bismark -v` gives `Bismark Version: v3.1.0` plus Copyright and URL lines. `bismark:1152-beta bismark --version` gives `v3.2.0-beta.1`. `/usr/local/bin/bismark` has the substituted literal.
- **The removed ARG:**
  - `release.yml:262-263` and `ga-candidate-image.yml:41-42` still pass `--build-arg BISMARK_SUITE_VERSION`. The builder stage (`Dockerfile:17`) consumes it, so buildx raises no unused-build-arg warning, and the binary and therefore the wrapper still carry the release version.
  - No docs reference the runtime ARG: `git grep` outside `plans/` and `rust/` finds only the builder lines.
- **Release runners:** both are native (`ubuntu-latest` amd64, `ubuntu-24.04-arm` arm64; `release.yml:238-247`). Running `bismark.bin` in the runtime stage needs no QEMU in CI.

## Issues by area

### Logic / shell correctness — no defects

Line numbers are in `Dockerfile:93-102`.

- `banner=$(… || true)` (93): a crashing or non-executable binary yields an empty or odd banner and reaches the explicit error. `set -u` is safe because every variable is assigned before use.
- The sed BRE (94): `(`/`)` are literal in BRE, `\([^ ]*\)` captures up to the first space, and `-n …p` prints only a matching line. The em dash in the trailing `.*` is valid UTF-8, so GNU sed matches it under either locale. The in-image runs confirm this.
- The shape gate (95-96): `grep -Eqx` anchors the whole line. The allowed character set excludes `&`, `\`, `|` and `/`, which keeps the later `s|…|$v|` replacement safe. Under `pipefail`, `printf | grep -q` cannot SIGPIPE in practice: the payload is one tiny builtin write that has finished before grep exits.
- The post-write check (100-101):
  - It runs the real wrapper through its `#!/bin/sh` (dash) shebang, which is good: it exercises the shipped path.
  - The `| head -n1` inside `$(…)` could in theory SIGPIPE the 3-line printf, and `pipefail` would make the substitution's status non-zero. That status is ignored, because the command is `[` and errexit does not fire on substitutions inside a simple command's arguments. The output is also a single ~100-byte write, so there is no real race.
  - It catches a missing placeholder: the plan's check 4(c) confirmed that.
- **Multi-line `$v`** (theoretical only): `grep -x` passes if *any* line matches. If the banner ever printed two matching lines, `$v` would contain a newline. The `sed s|…|` step would then fail on the unterminated `s` command, and the final equality check would fail too. Defended twice, so there is no action.

### Efficiency
- No concern. The step adds one sub-millisecond binary exec per build, and its cache key follows the `COPY bismark.bin` layer, so a version change re-runs it.

### Structure / comments
- The new Dockerfile comment at 91 is one line, states the current state, and is accurate. The edited block at 83-89 is accurate: it describes the suite banner, not the retired `Bismark Aligner (Rust port)` text.
- The wrapper comment at 17 is one line and accurate.

## Recommendations

| Priority | Item | Location |
|---|---|---|
| Low | **The example version will go stale.** "For suite version 3.1.0: Parser-1 → "3.1.0"" goes stale on the next bump (the old text said 3.0.0 and had gone stale the same way). Suggest `For suite version <v>: Parser-1 → "<v>"   Parser-2 → "<v>"`. | `docker/bismark-canonical-wrapper.sh:14-15` |
| Low | **The comment calls `bismark.bin` "the aligner".** It says "execs the unchanged real aligner", but `bismark.bin` is the multicall dispatcher (`bismark --help` shows the composed suite help; `bismark dedup …` routes to dedup). The Dockerfile comment was updated to "real binary" in this diff, but the wrapper comment was not. The text predates this diff, but the paragraph sits right after the edited one. | `docker/bismark-canonical-wrapper.sh:19` |
| Low | **The shape regex rejects SemVer build metadata (`+…`).** It is fine for `rust/VERSION` today. A future `X.Y.Z+meta` version would fail the image build loudly rather than silently, which is the intended direction; just note it. | `Dockerfile:95` |
| Low | **CHANGELOG has no "Reported by".** The entry is one sentence, in the house format, under `### All tools` (Q1), so it passes the one-sentence rule. Neighbouring entries credit external reporters ("Reported by @…"). If #1152 came from an external user, add the credit. I could not check this: `gh` is sandbox-blocked. | `CHANGELOG.md:12` |

## Release smoke test (`release.yml:322-349`)

The smoke test still holds:
- Line 331 `bismark.bin --version` still exists.
- p1 and p2 parse the wrapper. The wrapper now takes the version from the binary, and the binary was built with the same `--build-arg`, so `EXPECT` = `check-release.outputs.version` still matches. The plan's validation check 5 reproduced this with `3.2.0-beta.1`.
- The build-time check at `Dockerfile:100-101` makes the wrapper/binary equality a build invariant, so the smoke test cannot see a wrapper that disagrees with the binary.

## Fixes applied
None (read-only review).
