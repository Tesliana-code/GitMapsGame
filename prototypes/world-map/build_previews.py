#!/usr/bin/env python3
"""Build the four local world-map prototype previews into site/prototype/ (gitignored).

    python3 prototypes/world-map/build_previews.py [--fix-csp]

Previews:
  real-populated/      frozen recorded cli/cli snapshot (real GitHub data)
  real-empty/          frozen recorded Tesliana-code/GitMapsGame snapshot (real, no crossings)
  tutorial/            the prototype with no data: the fictional tutorial world
  constructed-stress/  constructed test world, not from a live repository: a designed graph that
                       exercises depth, roads, cycles and every state. Labelled in the page; local only.

Uses the reference oracle (gitmaps/) for derivation, escaping and the CSP check. Does not
touch production index.html, the Rust engine or the frozen fixtures. Output is deterministic.
"""
import argparse
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from gitmaps import SNAPSHOT_SCHEMA, canonical_bytes  # noqa: E402
from gitmaps.derive import check_publishable, derive  # noqa: E402
from gitmaps.render import check_template, inline_json, refresh_csp, render_html  # noqa: E402

TEMPLATE = ROOT / "index.html"  # the production template (approved design promoted from this folder)
OUT = ROOT / "site" / "prototype"
RECORDED = ROOT / "tests" / "fixtures" / "recorded"
PREVIEW_SLOT = '<script type="application/json" id="gitmaps-preview">null</script>'
CONSTRUCTED_REPO = "constructed/not-a-repository"


def constructed_snapshot():
    """A designed graph of invented crossings covering every state. Every value is synthetic."""
    url = "https://example.invalid/constructed/%s/%d"
    oid = lambda n: "%040x" % (n * 2654435761)  # deterministic fake commit ids
    # number: (title suffix, requires, kind, extra); kinds are mapped to GitHub facts below.
    plan = {
        1: ("source", [], "merged", {"ci": "success", "approver": "test-player-2"}),
        2: ("schema", [], "merged", {"ci": "failure", "approver": None}),
        3: ("tooling", [], "merged", {"ci": None, "approver": "test-player-3"}),
        4: ("open frontier", [], "open", {}),
        5: ("assigned root", [], "assigned", {"who": "test-player-4"}),
        6: ("abandoned", [], "not_planned", {}),
        7: ("closed by hand", [], "manual", {}),
        8: ("parser", [1], "merged", {"ci": "success", "approver": "test-player-1"}),
        9: ("model", [1, 2], "merged", {"ci": "success", "approver": None}),
        10: ("ready next", [2], "open", {}),
        11: ("in review", [3], "open_pr", {"ci": "pending", "changes": False}),
        12: ("assigned work", [3], "assigned", {"who": "test-player-5"}),
        13: ("behind ruins", [6], "open", {}),
        14: ("premature claim", [4], "assigned", {"who": "test-player-6"}),
        15: ("api", [8], "merged", {"ci": "success", "approver": "test-player-4"}),
        16: ("joins two", [8, 9], "open", {}),
        17: ("failing pr", [9], "open_pr", {"ci": "failure", "changes": True}),
        18: ("waits on 10", [10], "open", {}),
        19: ("waits on 11", [11], "open", {}),
        20: ("service", [15], "merged", {"ci": "success", "approver": "test-player-2"}),
        21: ("needs 15 and 16", [15, 16], "open", {}),
        22: ("needs 17", [17], "open", {}),
        23: ("frontier far", [20], "open", {}),
        24: ("needs 20 and 21", [20, 21], "open", {}),
        25: ("deep 1", [23], "open", {}),
        26: ("deep 2", [24], "open", {}),
        27: ("deepest", [25, 26], "open", {}),
        28: ("missing dependency", [99], "open", {}),
        29: ("cycle a", [30], "open", {}),
        30: ("cycle b", [29], "open", {}),
        31: ("unresolved reference", [], "open", {"unresolved": ["other/repo#1"]}),
        32: ("behind the cycle", [29], "assigned", {"who": "test-player-1"}),
        33: ("</script><b>hostile title</b> & <img src=x onerror=alert(1)>", [4], "open", {}),
    }
    issues, pulls = [], []
    for n, (suffix, requires, kind, extra) in sorted(plan.items()):
        linked, assignees, closed_event, state, reason = [], [], None, "open", None
        if kind in ("merged", "open_pr"):
            pr_n = 100 + n
            linked = [pr_n]
            merged = kind == "merged"
            approver = extra.get("approver")
            reviews = [{"state": "approved", "submitted_at": "2000-01-10T00:00:00Z", "author": approver, "commit": oid(pr_n)}] if approver else []
            if extra.get("changes"):
                reviews.append({"state": "changes_requested", "submitted_at": "2000-01-11T00:00:00Z", "author": "test-player-6", "commit": oid(pr_n)})
            ci = extra.get("ci")
            pulls.append({"number": pr_n, "title": "Constructed pull request %d" % pr_n, "url": url % ("pull", pr_n),
                          "repository": CONSTRUCTED_REPO, "state": "merged" if merged else "open", "merged": merged, "merged_at": None,
                          "created_at": "2000-01-01T00:00:00Z", "updated_at": "2000-01-12T00:00:00Z", "author": "test-player-%d" % (1 + n % 6),
                          "head_oid": oid(pr_n), "merge_commit_oid": oid(pr_n + 1) if merged else None, "reviews": reviews, "fetch_errors": [],
                          "ci": {"commit": oid(pr_n), "rollup": ci, "fetch_error": False, "contexts_complete": True,
                                 "contexts": [] if ci is None else [{"kind": "check_run", "name": "constructed-ci", "status": "completed", "conclusion": ci}]}})
            if merged:
                state, reason = "closed", "completed"
                closed_event = {"created_at": "2000-01-12T00:00:00Z", "actor": "test-player-1", "state_reason": "completed",
                                "closer": {"type": "pull_request", "number": pr_n, "repository": CONSTRUCTED_REPO}}
        elif kind == "assigned":
            assignees = [extra["who"]]
        elif kind in ("not_planned", "manual"):
            state, reason = "closed", "not_planned" if kind == "not_planned" else "completed"
            closed_event = {"created_at": "2000-01-12T00:00:00Z", "actor": "test-player-1", "state_reason": reason, "closer": None}
        issues.append({"number": n, "title": suffix if n == 33 else "Constructed crossing %d: %s" % (n, suffix), "url": url % ("issues", n),
                       "state": state, "state_reason": reason, "created_at": "2000-01-01T00:00:00Z",
                       "updated_at": "2000-01-%02dT00:00:00Z" % (1 + n % 28), "closed_at": None, "body_sha256": "0" * 64,
                       "requires": {"refs": sorted(requires), "unresolved": extra.get("unresolved", [])},
                       "assignees": assignees, "assignees_complete": True,
                       "linked_pull_requests": [{"number": p, "repository": CONSTRUCTED_REPO} for p in linked],
                       "linked_pull_requests_complete": True, "closed_event": closed_event})
    return {"schema": SNAPSHOT_SCHEMA, "source": {"api": "constructed-test-data", "selection": {"mode": "constructed"}},
            "repository": {"name_with_owner": CONSTRUCTED_REPO, "is_private": False, "visibility": "public", "label_exists": True},
            "issues": issues, "pull_requests": pulls}


def write(directory, files):
    directory.mkdir(parents=True, exist_ok=True)
    digests = {}
    for name, data in files.items():
        (directory / name).write_bytes(data)
        digests[name] = hashlib.sha256(data).hexdigest()
    sums = "".join("%s  %s\n" % (digests[n], n) for n in sorted(digests) if n.endswith((".html", ".json")))
    (directory / "SHA256SUMS").write_text(sums, encoding="ascii")
    return digests["index.html"]


def site_files(template, snapshot, preview=None):
    check_publishable(snapshot)
    map_obj = derive(snapshot)
    html = render_html(map_obj, template=template)
    if preview is not None:
        html = html.replace(PREVIEW_SLOT, '<script type="application/json" id="gitmaps-preview">%s</script>' % inline_json(preview))
    return {"index.html": html.encode("utf-8"), "map.json": canonical_bytes(map_obj), "snapshot.json": canonical_bytes(snapshot)}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--fix-csp", action="store_true", help="rewrite index.html's CSP hash after editing its script")
    args = parser.parse_args()
    template = TEMPLATE.read_text(encoding="utf-8")
    if args.fix_csp:
        template = refresh_csp(template)
        TEMPLATE.write_text(template, encoding="utf-8")
    check_template(template)
    if template.count(PREVIEW_SLOT) != 1:
        raise SystemExit("index.html must contain exactly one empty preview slot")

    previews = {
        "real-populated": site_files(template, json.loads((RECORDED / "cli-cli.snapshot.json").read_text())),
        "real-empty": site_files(template, json.loads((RECORDED / "gitmapsgame.snapshot.json").read_text())),
        "tutorial": {"index.html": template.encode("utf-8")},
        "constructed-stress": dict(site_files(template, constructed_snapshot(), preview={"kind": "constructed"}),
                                   **{"NOTICE.txt": b"CONSTRUCTED TEST DATA - NOT A REAL REPOSITORY.\n"
                                                    b"Generated locally by build_previews.py to stress-test the map layout.\n"
                                                    b"No crossing, player, pull request or check here exists. Never publish.\n"}),
    }
    for name, files in previews.items():
        digest = write(OUT / name, files)
        print("%-20s %s  index.html sha256 %s" % (name, OUT / name / "index.html", digest[:16]))


if __name__ == "__main__":
    main()
