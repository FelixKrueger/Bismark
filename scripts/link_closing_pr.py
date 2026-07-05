#!/usr/bin/env python3
"""
Populate the "Closing PR" custom Text field on FelixKrueger Project #1 for one
PR + the issue(s) it closes.

Usage:
    python3 scripts/link_closing_pr.py <PR_NUMBER>

Or from a GitHub Actions workflow, with the merged PR's number as env var.

Authentication: requires `GH_TOKEN` env var with classic-token `project` scope
or fine-grained-token "Projects (read/write)" permission. The workflow's
default GITHUB_TOKEN does NOT have project scope; configure a repository
secret `PROJECT_TOKEN` and pass it as GH_TOKEN.

Why this script exists: GitHub's automatic "Linked pull requests" field on
Project V2 only populates when a PR merges into the repository's DEFAULT
branch. The Bismark Rust rewrite merges into `rust/iron-chancellor` (an
integration branch), so that auto-link never fires. This script populates a
custom Text field "Closing PR" instead, which we read on the project board
to surface the issue↔PR relationship.

See plans/ history for the 2026-05-28 rationale + backfill notes.
"""

import json
import os
import re
import subprocess
import sys
from collections import defaultdict

REPO = "FelixKrueger/Bismark"
OWNER = "FelixKrueger"
PROJECT_NUMBER = 1
PROJECT_ID = "PVT_kwHOAFmTrc4BYkHj"
FIELD_ID = "PVTF_lAHOAFmTrc4BYkHjzhUCrQs"  # "Closing PR" TEXT field on project #1
REPO_URL = f"https://github.com/{REPO}"

# Closes-keyword regex — per GitHub docs:
# https://docs.github.com/en/issues/tracking-your-work-with-issues/using-issues/linking-a-pull-request-to-an-issue
CLOSE_RE = re.compile(
    r"\b(?:close[ds]?|fix(?:e[ds])?|resolve[ds]?)\s+(?:issue\s+)?#(\d+)\b",
    re.IGNORECASE,
)


def sh(args):
    r = subprocess.run(args, capture_output=True, text=True)
    if r.returncode != 0:
        sys.stderr.write(f"FAILED: {' '.join(args)}\n{r.stderr}\n")
        sys.exit(1)
    return r.stdout


def gh_graphql(query, **variables):
    args = ["gh", "api", "graphql", "-f", f"query={query}"]
    for k, v in variables.items():
        args.extend(["-f", f"{k}={v}"])
    return json.loads(sh(args))


def get_pr(pr_number):
    """Return {number, body, url} for the given PR."""
    out = sh(["gh", "pr", "view", str(pr_number), "--json", "number,body,url"])
    return json.loads(out)


def get_project_items():
    """Map content-number → project-item-id for items currently on the board."""
    out = sh([
        "gh", "project", "item-list", str(PROJECT_NUMBER),
        "--owner", OWNER,
        "--format", "json",
        "--limit", "300",
    ])
    items = json.loads(out)["items"]
    return {it["content"]["number"]: it["id"] for it in items if it.get("content", {}).get("number")}


def parse_closes(body):
    if not body:
        return []
    return sorted({int(m.group(1)) for m in CLOSE_RE.finditer(body)})


def set_field(item_id, value):
    gh_graphql(
        """
        mutation($projectId: ID!, $itemId: ID!, $fieldId: ID!, $value: String!) {
          updateProjectV2ItemFieldValue(input: {
            projectId: $projectId, itemId: $itemId, fieldId: $fieldId,
            value: { text: $value }
          }) { projectV2Item { id } }
        }
        """,
        projectId=PROJECT_ID, itemId=item_id, fieldId=FIELD_ID, value=value,
    )


def add_to_project_if_missing(content_number, items_map, content_type):
    """If the issue/PR isn't on the board, add it. Returns the item ID."""
    if content_number in items_map:
        return items_map[content_number]
    url = f"{REPO_URL}/{'pull' if content_type == 'pr' else 'issues'}/{content_number}"
    out = sh([
        "gh", "project", "item-add", str(PROJECT_NUMBER),
        "--owner", OWNER, "--url", url, "--format", "json",
    ])
    item = json.loads(out)
    item_id = item["id"]
    items_map[content_number] = item_id
    print(f"  added #{content_number} ({content_type}) to project: {item_id}", file=sys.stderr)
    return item_id


def main():
    if len(sys.argv) != 2:
        sys.stderr.write("Usage: link_closing_pr.py <PR_NUMBER>\n")
        sys.exit(2)
    pr_number = int(sys.argv[1])

    print(f"Fetching PR #{pr_number}…", file=sys.stderr)
    pr = get_pr(pr_number)
    closes = parse_closes(pr["body"])
    if not closes:
        print(f"  PR #{pr_number} body has no closes/fixes/resolves references — nothing to link.", file=sys.stderr)
        return
    print(f"  closes: {closes}", file=sys.stderr)

    print("Fetching project items…", file=sys.stderr)
    items = get_project_items()

    # Issue side: each closed issue gets its "Closing PR" set to this PR's URL.
    # (If an issue is closed by multiple PRs over time, this overwrites — acceptable
    # since the most-recent closer is the most useful pointer.)
    pr_url = pr["url"]
    for issue_num in closes:
        item_id = add_to_project_if_missing(issue_num, items, "issue")
        set_field(item_id, pr_url)
        print(f"  issue #{issue_num} → Closing PR = {pr_url}", file=sys.stderr)

    # PR side: this PR's "Closing PR" field lists the issues it closes.
    pr_item_id = add_to_project_if_missing(pr_number, items, "pr")
    pr_value = ", ".join(f"{REPO_URL}/issues/{n}" for n in closes)
    set_field(pr_item_id, pr_value)
    print(f"  PR #{pr_number} → Closing PR = {pr_value}", file=sys.stderr)

    print("Done.", file=sys.stderr)


if __name__ == "__main__":
    main()
