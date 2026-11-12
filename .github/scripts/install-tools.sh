#!/usr/bin/env bash
# Install bioinformatics tools for CI: from apt, or from bioconda when the Ubuntu
# mirrors are unreachable (a runner DNS outage on *.ubuntu.com once failed every
# run for hours while GitHub and anaconda.org still resolved).
# Usage: install-tools.sh minimap2 samtools bowtie2
set -uo pipefail

apt_install() {
  local opts=(-o Acquire::Retries=2 -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20)
  sudo apt-get "${opts[@]}" update && sudo apt-get "${opts[@]}" install -y --no-install-recommends "$@"
}

if ! (export -f apt_install; timeout 300 bash -c 'apt_install "$@"' _ "$@"); then
  echo "::warning::apt could not install $* (Ubuntu mirrors unreachable?); installing from bioconda instead"
  set -e
  curl -fsSL --retry 3 -o "$RUNNER_TEMP/micromamba" \
    https://github.com/mamba-org/micromamba-releases/releases/latest/download/micromamba-linux-64
  chmod +x "$RUNNER_TEMP/micromamba"
  # The versions apt installs on ubuntu-latest (26.04), so results do not depend on the
  # source. One environment per tool: bioconda's minimap2 2.27 and samtools 1.22.1 need
  # incompatible zlib versions.
  for tool in "$@"; do
    case "$tool" in
      minimap2) spec=minimap2=2.27 ;;
      samtools) spec=samtools=1.22.1 ;;
      bowtie2) spec=bowtie2=2.5.4 ;;
      *) spec="$tool" ;;
    esac
    "$RUNNER_TEMP/micromamba" create -y -r "$RUNNER_TEMP/mamba" -p "$RUNNER_TEMP/$tool" \
      -c conda-forge -c bioconda "$spec"
    # Only the tool's own executables (bowtie2-build, …): the env's perl/python must
    # not shadow the system ones.
    mkdir -p "$RUNNER_TEMP/bin"
    ln -sf "$RUNNER_TEMP/$tool/bin/$tool"* "$RUNNER_TEMP/bin/"
  done
  echo "$RUNNER_TEMP/bin" >> "$GITHUB_PATH"
  export PATH="$RUNNER_TEMP/bin:$PATH"
fi

set -e
for tool in "$@"; do
  case "$tool" in
    minimap2) minimap2 --version ;;
    # sed reads all of it: `head -1` would end the pipe early and fail it under pipefail.
    *) "$tool" --version | sed -n 1p ;;
  esac
done
command -v "$@"
