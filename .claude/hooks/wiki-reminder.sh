#!/usr/bin/env bash
# SessionStart hook: points agents to wiki/ENTRY.md and reports code commits since the last wiki build.
# Prints where the wiki starts, and how many code commits came after the last build or update in wiki/log.md.
# Never fails the session: every error path exits 0.
cd "${CLAUDE_PROJECT_DIR:-.}" 2>/dev/null || exit 0

# Stay silent when the wiki is gone.
[ -f wiki/ENTRY.md ] || exit 0

echo "Read wiki/ENTRY.md first."

log="wiki/log.md"
[ -f "$log" ] || exit 0
# The last "@ <hash>" in the log is the HEAD that the last build or update read.
head_at_build="$(sed -n 's/.*@ \([0-9a-f]\{7,40\}\).*/\1/p' "$log" | tail -n 1)"
[ -n "$head_at_build" ] || exit 0

# Count commits that touch code, not the wiki commits.
count="$(git rev-list --count "${head_at_build}..HEAD" -- . \
  ':(exclude)wiki' ':(exclude)AGENTS.md' ':(exclude)CLAUDE.md' ':(exclude).claude' 2>/dev/null)" || exit 0
[ "${count:-0}" -gt 0 ] 2>/dev/null || exit 0

short="$(git rev-parse --short "$head_at_build" 2>/dev/null || echo "$head_at_build")"
echo "The wiki was built at ${short}. ${count} code commit(s) since then. The wiki may be out of date."
exit 0
