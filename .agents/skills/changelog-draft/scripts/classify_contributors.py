#!/usr/bin/env python3
"""Classify GitHub usernames as internal, external, or bot.

Uses `gh api` to check org membership — stdlib only, no pip deps.

Usage:
    python3 classify_contributors.py --org warpdotdev --authors user1,user2,user3

Outputs JSON to stdout.
"""

import argparse
import json
import subprocess
import sys

KNOWN_BOTS = frozenset(
    {
        "dependabot",
        "dependabot[bot]",
        "renovate",
        "renovate[bot]",
        "github-actions",
        "github-actions[bot]",
        "codecov",
        "codecov[bot]",
        "warp-bot",
        "warp-bot[bot]",
    }
)


def run(cmd: list[str], *, check: bool = True) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, check=check)


def is_org_member(org: str, username: str) -> bool:
    """Check if a user is a member of the given GitHub org via gh api."""
    result = run(
        ["gh", "api", f"orgs/{org}/members/{username}", "--silent"],
        check=False,
    )
    # 204 No Content = is a member, 302/404 = not a member
    return result.returncode == 0


def main() -> None:
    parser = argparse.ArgumentParser(description="Classify contributor types")
    parser.add_argument("--org", required=True, help="GitHub org to check membership")
    parser.add_argument(
        "--authors",
        required=True,
        help="Comma-separated list of GitHub usernames",
    )
    args = parser.parse_args()

    authors = [a.strip() for a in args.authors.split(",") if a.strip()]

    internal: list[str] = []
    external: list[str] = []
    bot: list[str] = []

    for author in authors:
        if author.lower() in KNOWN_BOTS or author.endswith("[bot]"):
            bot.append(author)
        elif is_org_member(args.org, author):
            internal.append(author)
        else:
            external.append(author)

    output = {"internal": internal, "external": external, "bot": bot}
    json.dump(output, sys.stdout, indent=2)
    print()


if __name__ == "__main__":
    main()
