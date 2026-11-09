# Plan Review B — #1152 Docker wrapper version (PLAN.md rev 0)

Reviewer B, 2026-11-05. Read-only review against the worktree at `da431b7` (`fix/1152-docker-wrapper-version`).

**Verdict:** sound. The approach is correct, and the `sed` works against the real banner. One Important gap: the guard checks the extracted version but not the written wrapper, so a placeholder regression would still ship silently. Two small factual slips in the plan text should be corrected.

---

## 1. Logic review

### 1.1 Does `bismark.bin --version` print `meta::version_line("bismark")`? Yes, via the dispatcher, not the aligner

- `src/bin/bismark.rs` calls `bismark::cli::dispatch()`.
- `cli.rs:129-140`: `prog` = `file_name(argv[0])` = `"bismark.bin"`. `run_legacy_alias` (`cli.rs:36-51`) returns `None` for that name.
- `cli.rs:149-155`: `argv[1] == "--version"` prints `crate::meta::version_line("bismark")` with `println!` (**stdout**), then exits 0. This happens in the dispatcher. The aligner's `run_dispatch` is never reached.
- The aligner's own path (`aligner/mod.rs:106-108` `version_string()`, `:140-143`) prints the same string. So the plan's output claim is right. Its wording ("routes to the aligner", citing `aligner/mod.rs:106`) is not: `--version` and `-V` are handled at `cli.rs:152`.
- Nothing else reaches stdout. No notice or allocator output comes before it.

### 1.2 The `sed` against the real banner: correct

The banner is `meta/mod.rs:42-47`: `"{tool} (Bismark Rust suite) v{SUITE_VERSION} ({body})"`.

- Docker builds have no `.git` (`.dockerignore` excludes `.git/`), so `GIT_SHORT_HASH` is normally `unknown`.
- `version_body` (`meta/mod.rs:31-37`) then drops the hash. The line becomes `bismark (Bismark Rust suite) v3.1.0 (linux/aarch64 — built 2026-…Z)`.
- After the version there is still a space and then `(`, so `v\([^ ]*\) .*` matches either shape.

How the pattern behaves:

- **BRE parentheses:** in POSIX BRE, `(` and `)` are literals and `\(…\)` is the group, so the pattern is correct as written. No `-E`.
- **Em-dash / UTF-8:** the runtime base is `mambaorg/micromamba:1.5.8-bookworm-slim`, which is Debian, so the `sed` is GNU sed, not BusyBox.
  - In a C/POSIX locale, `.` matches any byte.
  - In C.UTF-8, `.` matches the em-dash, which is valid UTF-8 (U+2014).
  - Either way `.*` consumes the rest of the line.
  - The one theoretical failure is invalid UTF-8 in a UTF-8 locale. There `.*` stops early and the unmatched tail is *appended* after `\1`. The `case` guard would not catch that. It cannot happen with this banner, but see action item O1.
- **`-n` + `p`:** only a matching line is printed, so any future extra stdout line is ignored. Good.
- **The `v` prefix:** the literal `v` must directly precede the version (`v{SUITE_VERSION}`). It does.

### 1.3 `set -eu` and command-substitution failure

- `v=$(bismark.bin --version | sed …)`: without `pipefail`, the exit status is `sed`'s, which is 0. So if `bismark.bin` fails, the assignment itself does not trip `set -e`. That covers a missing binary, `exec format error` under a missing binfmt/QEMU, or a panic.
- `sed` then prints nothing, `v=""`, and the `case` fires `exit 1`. The empty-string guard is what makes the failure loud, and it does.
- `bismark.bin`'s stderr still reaches the build log, so the cause is visible.
- `set -u` is satisfied because `v` is always assigned.
- **Unstated assumption:** the RUN shell. The micromamba base sets `SHELL ["/usr/local/bin/_dockerfile_shell.sh"]`, which activates the env and runs the command under **bash**, not dash `/bin/sh`.
  - The block above is POSIX and behaves the same under bash or dash, with or without `pipefail`. With `pipefail`, a failing `bismark.bin` fails at the assignment instead, which is also loud.
  - The existing RUNs already use `set -eu; …` the same way (`Dockerfile:70`, `:92`).
  - So this is harmless. The plan should still say "bash via the micromamba SHELL wrapper" rather than imply plain `sh`.
- **PATH:** under activation, `/opt/conda/bin` comes first. No bioconda package installed here (`bowtie2`, `hisat2`, `minimap2`, `Dockerfile:51-55`) ships a `bismark.bin`, so `/usr/local/bin/bismark.bin` resolves. Calling it by absolute path would remove even that dependency (O2).

### 1.4 Removing the runtime-stage `ARG`

- `grep` finds only two consumers of `BISMARK_SUITE_VERSION` in the Dockerfile: the builder (`Dockerfile:17-18`) and the runtime wrapper `sed` (`:91,94`). Removing the second breaks nothing.
- Both workflows pass the build arg (`release.yml:262-263`, `ga-candidate-image.yml:41-42`). The builder stage still consumes it, so BuildKit gives no "unconsumed build-arg" warning.
- **Release-path equivalence:** `build.rs:114-138` returns the trimmed env value when it is non-empty and not `unknown`. The binary therefore carries exactly the version the release passes, and the wrapper now reads it back. This holds as long as `release.yml`'s `needs.check-release.outputs.version` has no surrounding whitespace.
  - Previously the wrapper got the raw arg and the binary got the trimmed arg. Any whitespace would now be *normalised*, not broken.
  - The smoke test (`release.yml:345-349`) compares against the same untrimmed value with `xargs`, so it is unaffected.
- **Cache:** the wrapper layer is now keyed on the `bismark.bin` layer instead of the ARG value. It reruns whenever the binary changes, which costs milliseconds. No concern.

### 1.5 Multi-arch / QEMU

- `release.yml:240-246` builds each platform on a **native** runner (`ubuntu-latest` amd64, `ubuntu-24.04-arm` arm64). `ga-candidate-image.yml:40` is amd64 on an amd64 runner.
- So no release path runs `bismark.bin` under emulation. The plan's QEMU assumption only matters for an ad-hoc local `buildx --platform linux/amd64,linux/arm64`. There it works when binfmt is registered and fails loudly via the empty-`v` guard when it is not. The assumption is correct, but its weight is lower than the plan implies.

### 1.6 Smaller inaccuracies in the plan text

- **Validation 3:** "`bismark --help` … shows the aligner help". It doesn't. The wrapper execs `bismark.bin --help`, and `cli.rs:149-151` prints the **composed suite help** (`print_top_level_help`, which starts with the `version_line` banner) and exits 0. The check still proves the exec works; only the expected text is wrong. (`bismark align --help` would show the aligner help.)
- **Stale comments the plan does not touch:**
  - `docker/bismark-canonical-wrapper.sh:10-11` and `Dockerfile:79` say the real aligner's `--version` is `Bismark Aligner (Rust port) Version: …`. Today it is the `version_line` shape.
  - The wrapper's example `3.0.0` (`:17-18`) is dated.
  - These are not in scope for #1152, but the plan already edits the wrapper header at line 19, so fixing them is nearly free. See O3; leaving them is also defensible under "touch only what the fix touches".

## 2. Assumptions

| Assumption | Status |
|---|---|
| Banner shape `bismark (Bismark Rust suite) v<ver> (…)` | Verified (`meta/mod.rs:42-47`). There is a unit test, `version_line_shape`, `meta/mod.rs:58-71`, but it only checks the **prefix**, not the ` (` after the version. A change to `v{ver}-{x}` would still pass it. The build guard covers the empty case only. |
| `rust/VERSION` reaches the builder | Verified: `3.1.0`. `.dockerignore` excludes only `**/*.md`, and the vendored `rust/bismark/VERSION` is also `3.1.0`. |
| `unknown` means build.rs failed to resolve | Verified (`build.rs:137`). Guarded. |
| Version has no space or `\|` | It also has no `&` and no `\`, which are special in a `sed` replacement. `[^ ]*` guarantees no space. `&`, `\` and `\|` are not guarded, only assumed. O1 covers all of them. |
| `bismark.bin` executes in the runtime stage | Verified for native builds. It is glibc-linked, built on `rust:1.89-bookworm` and run on bookworm-slim, so the glibc ABI matches. |
| RUN shell | Unstated: bash via the micromamba `SHELL` wrapper. Harmless (1.3). |

## 3. Efficiency

There is nothing to optimise. It is one process spawn at build time.

Validation 2 is the only real cost: a ~20–30 min cold cargo rebuild, because the builder `ENV` changes. It is worth paying once. It is the only check that proves the wrapper follows the **binary**, rather than coincidentally matching `rust/VERSION` (in Validation 1 both are `3.1.0`, so Validation 1 alone cannot tell the two sources apart).

## 4. Validation sufficiency

What the plan covers well:

- the plain path (V1);
- the arg path with a value that differs from `rust/VERSION` (V2). This is the key discriminating test;
- exec still working (V3);
- smoke-script equivalence (V5).

Gaps:

1. **Silent placeholder regression (Important).**
   - The guard validates `$v` but not the *written wrapper*. If `__SUITE_VERSION__` is ever renamed or dropped from `docker/bismark-canonical-wrapper.sh`, `sed` does nothing, exits 0, and ships a wrapper printing `Bismark Version: v__SUITE_VERSION__`. A wrong substitution does the same.
   - The one guard that would catch this is the release smoke test, and it runs only on a real (non-dry-run) cut, *after* the image is pushed (`release.yml:318-322`).
   - Plain builds, which are what #1152 is about, have no guard at all.
   - A one-line post-write self-check in the same RUN closes this at build time for every build path:
     ```sh
     [ "$(/usr/local/bin/bismark -v | head -n1)" = "Bismark Version: v$v" ] || { echo "wrapper does not report v$v" >&2; exit 1; }
     ```
     It proves the wrapper and the binary agree inside the image, which is the plan's stated goal ("can never disagree").
2. **V4 is a one-off manual stub test.** That is acceptable for a guard this small. The post-write check above gives a permanent, automatic regression check, which V4 does not.
3. **No CI exercise of the plain-build path.** The release dry-run always passes the build arg. With gap 1 fixed, the build itself is the regression test whenever anyone runs a plain `docker build`. Adding a CI job is not justified for this fix.
4. **amd64 is not validated locally.** It is fine to leave to the release workflow, since the logic is architecture-independent.

## 5. Alternatives

- **Runtime derivation in the wrapper** (`bismark.bin --version | sed …` on every `-v`): no build-time stamping at all. Rejected. It would move the banner coupling to runtime, where a mismatch fails silently inside a methylseq run instead of at build, and it adds a process plus a `sed` to every probe. The plan's build-time approach is better.
- **Keep the runtime ARG with `rust/VERSION` as a fallback:** two sources that can still disagree. Q2 rejected this, correctly.
- **`COPY` `rust/VERSION` into the runtime stage and read it:** simpler, but a `--build-arg` release would then make the wrapper (VERSION) disagree with the binary (arg), which is the very class of bug being fixed. Inferior.
- **Have the binary emit the wrapper banner** (e.g. a hidden `bismark.bin --methylseq-version`): it removes the `sed` and the banner coupling, but it is a Rust change and adds CLI surface for a container-only concern. Not worth it here.

## 6. Action items

### Critical

None.

### Important

- **I1. Add a post-write self-check in the same RUN** (gap 4.1).
  - It catches a missing or renamed `__SUITE_VERSION__` placeholder and any substitution error at build time on every path.
  - Without it, a plain build can still silently ship a wrong wrapper banner, the exact bug class of #1152.
  - Add it to the Implementation outline step 1 and to Validation 4 (also stub-test the placeholder-missing case).
  - Evidence: `docker/bismark-canonical-wrapper.sh:29` is the sole placeholder site, and `Dockerfile:93-96` currently has no output check.

### Optional

- **O1. Tighten the guard to a version shape** instead of only `""|unknown`, e.g. `printf '%s\n' "$v" | grep -Eqx '[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?'`. This rejects a residue-appended capture (1.2) and `&`, `\` or `|`, which would corrupt the `sed` replacement (§2). It makes the "versions are `X.Y.Z[-pre]`" assumption enforced rather than assumed.
- **O2. Call `/usr/local/bin/bismark.bin` by absolute path.** This removes the dependence on the activated-env `PATH` ordering (1.3).
- **O3. Correct the plan text:**
  - `--version` is handled by the dispatcher (`cli.rs:152-155`), not the aligner (`aligner/mod.rs:106`);
  - Validation 3 shows the **suite** help, not the aligner help (`cli.rs:149-151`);
  - the RUN shell is bash via the micromamba `SHELL` wrapper;
  - QEMU never applies to the release workflows, which use native runners (`release.yml:240-246`).
- **O4. Optionally fix the stale comments next to the line being edited:** `docker/bismark-canonical-wrapper.sh:10-11`, `:17-18` and `Dockerfile:79` still describe a `Bismark Aligner (Rust port) Version:` banner that no longer exists. Each would be a one-line current-state comment. Skipping this is fine if scope should stay minimal.
