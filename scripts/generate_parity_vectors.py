#!/usr/bin/env python3
"""Generate Rust parity vectors from the Python reference implementation (the oracle).

Every expected value is produced by running the reference code in gitmaps/, never
written by hand. Inputs are constructed test data, not GitHub activity.

    python3 scripts/generate_parity_vectors.py
    # rewrites tests/fixtures/parity/, src/pyrepr_table.rs and src/queries.rs
"""
import copy
import hashlib
import json
import random
import sys
import unicodedata
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "tests"))

from gitmaps import SNAPSHOT_SCHEMA, canonical_bytes, sha256_hex  # noqa: E402
from gitmaps.derive import derive  # noqa: E402
from gitmaps.fetch import parse_requires, snapshot  # noqa: E402
from gitmaps.github import GitHubError, ReplayTransport  # noqa: E402
import test_gitmaps_fetch as F  # noqa: E402

OUT = ROOT / "tests" / "fixtures" / "parity"
SEED = 20261021

TRICKY = ["", "plain", "quote\"s", "it's", "both'\"", "back\\slash", "tab\tnl\ncr\rbs\bff\f",
          "\x00\x01\x1b\x1f\x7f", "\x80\x85\x9f\xa0\xad", "é ü ñ ß", "中文", "😀🗺️", "  ",
          "﻿​‍", "\U000f0000", "͸\U000e0001", "\U0010ffff", "</script><!--&>",
          "#12", "＃3", "ﬁ", "İı", "ſ"]


def write(name, obj):
    OUT.mkdir(parents=True, exist_ok=True)
    data = json.dumps(obj, sort_keys=True, ensure_ascii=True, separators=(",", ":")) + "\n"
    (OUT / name).write_text(data, encoding="ascii")
    return len(data)


def canonical_vectors():
    values = [None, True, False, 0, -1, 7, 2 ** 53, 2 ** 63 - 1, -(2 ** 63), [], {}, [[]], [{}], {"a": []},
              {"b": 1, "a": 2, "B": 3, "_": 4, "é": 5, "Z": 6, "10": 7, "9": 8, "": 9, "😀": 10, "\x7f": 11},
              {"nested": {"z": [1, {"y": None, "x": [True, "s"]}], "a": {}}}]
    values += TRICKY + [{t: t} for t in TRICKY]
    values += ["".join(chr(c) for c in range(0, 0x100))]
    return [{"input": v, "expected": canonical_bytes(v).decode("ascii")} for v in values]


def repr_vectors():
    tokens = TRICKY + ["".join(chr(c) for c in range(0x20, 0x7f)), "a'b", 'a"b', "'\"'", "\\'"]
    rng = random.Random(SEED)
    for _ in range(300):
        tokens.append("".join(chr(rng.choice([rng.randrange(0, 0x80), rng.randrange(0x80, 0x3000),
                                              rng.randrange(0xe000, 0x10000), rng.randrange(0x10000, 0x110000)]))
                              for _ in range(rng.randrange(1, 6))))
    tokens = [t for t in tokens if not any(0xd800 <= ord(c) <= 0xdfff for c in t)]
    return [{"input": t, "expected": repr(t)} for t in tokens]


def requires_vectors():
    bodies = [
        None, "", "Requires: #1", "requires: #2, #3", "REQUIRES:#4,#5", "Requires : #6", "  -  Requires: #7",
        "* Requires: #8", "- **Requires:** #9", "**Requires**: #10", "Requires: **#11", "Requires: none",
        "Requires: NONE", "Requires: -", "Requires:", "Requires: none #12", "Requires: #0", "Requires: #01",
        "Requires: #12abc, abc, other/repo#1, #, ##3", "Requires:\t#13\t#14", "Requires: #15\r\nRequires: #16",
        "line\rRequires: #17", "```\nRequires: #18\n```\nRequires: #19", "~~~\nRequires: #20\n", "  ```py\nRequires: #21",
        "Requires: #22\x1c#23\x1f#24", "Requires: #25 #26 #27　#28", "Requires: #29\x85#30",
        "Requiſes: #31", "Requireſ: #47", "Requıres: #32", "REQUİRES: #48", "requıreſ: #49", "Requires: #33​#34", "ReQuIrEs: #35, #35, #35", "Requires: ＃36",
        "Requires: #٣", "Requires : #37", " Requires: #38", "x Requires: #39", "Requires: #40 # 41",
        "Requires: #42,,,#43", "Requires: 😀 #44", "Required: #45", "Requires:: #46",
    ]
    return [{"input": b, "expected": parse_requires(b)} for b in bodies]


def random_snapshot(rng, case):
    repo = "example/constructed-%d" % case
    other = "someone/else"
    n = rng.randrange(0, 11)
    numbers = sorted(rng.sample(range(1, 40), n))
    pr_numbers = sorted(rng.sample(range(100, 140), rng.randrange(0, 6)))
    logins = ["alice", "bob", "carol", "dave", "émile", "zoë", "李", "z-bot"]
    states = ["approved", "changes_requested", "dismissed", "commented", "pending"]
    pulls = []
    for p in pr_numbers:
        merged = rng.random() < 0.4
        state = "merged" if merged else rng.choice(["open", "closed"])
        head = "%040x" % rng.getrandbits(160)
        reviews = [{"state": rng.choice(states), "author": rng.choice(logins + [None]),
                    "commit": rng.choice([head, "%040x" % rng.getrandbits(160), None]),
                    "submitted_at": rng.choice(["2026-01-0%dT00:00:00Z" % rng.randrange(1, 10), None])}
                   for _ in range(rng.randrange(0, 5))]
        reviews.sort(key=lambda r: (r["submitted_at"] or "", r["author"] or "", r["state"], r["commit"] or ""))
        commit = rng.choice([head, head, None])
        rollup = rng.choice(["success", "failure", "error", "pending", "expected", None, "weird"])
        contexts = sorted(({"kind": rng.choice(["check_run", "status"]), "name": rng.choice(["build", "lint", "té"]),
                            "status": rng.choice(["completed", None]), "conclusion": rng.choice(["success", "failure", None])}
                           for _ in range(rng.randrange(0, 3))), key=lambda c: (c["kind"], c["name"], c["conclusion"] or ""))
        pulls.append({"number": p, "title": rng.choice(TRICKY), "url": "https://github.com/%s/pull/%d" % (repo, p),
                      "repository": repo if rng.random() < 0.9 else other, "state": state, "merged": merged,
                      "merged_at": None, "created_at": "2026-01-01T00:00:00Z",
                      "updated_at": "2026-0%d-01T00:00:00Z" % rng.randrange(1, 10),
                      "author": rng.choice(logins + [None]), "head_oid": head,
                      "merge_commit_oid": ("%040x" % rng.getrandbits(160)) if merged else None,
                      "reviews": reviews, "fetch_errors": [],
                      "ci": {"commit": commit, "rollup": rollup, "fetch_error": rng.random() < 0.1,
                             "contexts": contexts if rollup else [], "contexts_complete": rng.random() < 0.9}})
    tokens = ["other/repo#2", "#x", "it's", 'say"hi"', "＃3", "​", "back\\slash"]
    issues = []
    for i in numbers:
        closed = rng.random() < 0.45
        earlier = [m for m in numbers if m < i]
        if rng.random() < 0.85:  # mostly well-formed DAG edges, sometimes missing/self/forward (cycles)
            refs = sorted(set(rng.sample(earlier, min(len(earlier), rng.randrange(0, 3)))))
        else:
            refs = sorted(set(rng.sample(numbers + [99, i], min(len(numbers) + 2, rng.randrange(1, 3)))))
        merged_prs = [pr["number"] for pr in pulls if pr["merged"] and pr["repository"] == repo]
        closer = {"type": "pull_request", "number": rng.choice(merged_prs), "repository": repo} \
            if merged_prs and rng.random() < 0.6 else rng.choice([None, {"type": "pull_request", "number": rng.choice(pr_numbers + [150]), "repository": repo},
                             {"type": "pull_request", "number": 5, "repository": other},
                             {"type": "commit", "oid": "abc"}, {"type": "other", "typename": "ProjectV2"}]) \
            if pr_numbers else rng.choice([None, {"type": "commit", "oid": "abc"}])
        reason = rng.choice(["completed", "completed", "not_planned", "duplicate", None]) if closed else rng.choice([None, "reopened"])
        linked = sorted(({"number": rng.choice(pr_numbers * 6 + [151]) if pr_numbers else 151,
                          "repository": repo if rng.random() < 0.85 else other}
                         for _ in range(rng.randrange(0, 3))), key=lambda p: (p["repository"], p["number"]))
        issues.append({
            "number": i, "title": rng.choice(TRICKY), "url": "https://github.com/%s/issues/%d" % (repo, i),
            "state": "closed" if closed else "open", "state_reason": reason,
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-%02d-01T00:00:00Z" % rng.randrange(1, 13),
            "closed_at": None, "body_sha256": "0" * 64,
            "requires": {"refs": refs, "unresolved": sorted(set(rng.sample(tokens, rng.choice([0] * 8 + [1, 2]))))},
            "assignees": sorted(set(rng.sample(logins, rng.choice([0, 0, 1, 2])))),
            "assignees_complete": rng.random() < 0.9, "linked_pull_requests": linked,
            "linked_pull_requests_complete": rng.random() < 0.9,
            "closed_event": ({"created_at": "2026-02-01T00:00:00Z", "actor": rng.choice(logins + [None]),
                              "state_reason": reason, "closer": closer} if closed and rng.random() < 0.9 else None),
        })
    private = rng.random() < 0.2
    return {"schema": SNAPSHOT_SCHEMA,
            "source": {"api": "github-graphql", "selection": rng.choice([{"mode": "label", "label": "gitmaps:crossing"},
                                                                         {"mode": "numbers", "numbers": numbers}])},
            "repository": {"name_with_owner": repo, "is_private": private,
                           "visibility": "private" if private else "public", "label_exists": True},
            "issues": issues, "pull_requests": pulls}


def derive_vectors():
    rng = random.Random(SEED)
    vectors = []
    for case in range(120):
        snap = random_snapshot(rng, case)
        vectors.append({"snapshot": snap, "map_sha256": sha256_hex(canonical_bytes(derive(snap)))})
    base = random_snapshot(random.Random(1), 0)
    rejected = [{"schema": "other"}, dict(base, issues=[{"number": 2}, {"number": 1}]),
                dict(base, issues=[{"number": 1}, {"number": 1}]), dict(base, pull_requests=[{"number": 5}, {"number": 4}])]
    return vectors, [{"snapshot": s} for s in rejected]


def fetch_vectors():
    """Constructed GraphQL exchanges (shaped like tests/fixtures/recorded) replayed through the reference fetch."""
    closer = {"__typename": "PullRequest", "number": 10, "repository": {"nameWithOwner": F.REPO}}
    error = {"message": "Resource not accessible by integration",
             "path": ["repository", "pullRequest", "commits", "nodes", 0, "commit", "statusCheckRollup"]}
    no_label = F.repo_exchange()
    no_label["response"]["data"]["repository"]["label"] = None
    scenarios = {
        "pagination": [
            F.repo_exchange(),
            F.issues_exchange([F.issue_node(1, linked=[10], linked_total=2, linked_next="L1",
                                            body="Requires: #2\r\n- **Requires:** #3, x/y#1")], next_cursor="I1", total=2),
            F.issues_exchange([F.issue_node(2, body="Requires: none", assignees=["b", "a"])], after="I1", total=2),
            {"name": "linked_pull_requests", "variables": dict(F.BASE, number=1, after="L1"),
             "response": {"data": {"repository": {"issue": {"closedByPullRequestsReferences": {
                 "totalCount": 2, "pageInfo": F.page(False),
                 "nodes": [{"number": 11, "repository": {"nameWithOwner": F.REPO}}]}}}}}},
            F.pr_exchange(10, F.pr_response(10, reviews=[("APPROVED", "r1", "1")], review_next="R1")),
            F.pr_exchange(10, F.pr_response(10, reviews=[("CHANGES_REQUESTED", "r2", "2")]), after="R1"),
            F.pr_exchange(11, F.pr_response(11, contexts=["build", "a-lint"], context_total=150)),
        ],
        "truncated": [F.repo_exchange(),
                      F.issues_exchange([F.issue_node(1, assignees=["a"], assignee_total=3, linked=[10])]),
                      F.pr_exchange(10, F.pr_response(10, contexts=["build"], context_total=150))],
        "unknown_ci": [F.repo_exchange(), F.issues_exchange([F.issue_node(1, linked=[10])]),
                       F.pr_exchange(10, F.pr_response(10, rollup=None, errors=[error]))],
        "absent_ci": [F.repo_exchange(), F.issues_exchange([F.issue_node(1, linked=[10])]),
                      F.pr_exchange(10, F.pr_response(10, rollup=None))],
        "unreadable_closer": [F.repo_exchange(),
                              F.issues_exchange([F.issue_node(1, state="CLOSED", reason="COMPLETED", linked=[10], closer=closer)]),
                              F.pr_exchange(10, {"data": {"repository": {"pullRequest": None}}})],
        "merged_closer": [F.repo_exchange(private=True),
                          F.issues_exchange([F.issue_node(1, state="CLOSED", reason="COMPLETED", linked=[10], closer=closer),
                                             F.issue_node(2, body="Requires: #1", assignees=["zoë"])]),
                          F.pr_exchange(10, F.pr_response(10, merged=True, reviews=[("APPROVED", "émile", "1")]))],
        "commit_closer": [F.repo_exchange(), F.issues_exchange([F.issue_node(
            1, state="CLOSED", reason="COMPLETED", closer={"__typename": "Commit", "oid": "c" * 40})])],
        "cross_repo_closer": [F.repo_exchange(), F.issues_exchange([F.issue_node(
            1, state="CLOSED", reason="COMPLETED",
            closer={"__typename": "PullRequest", "number": 5, "repository": {"nameWithOwner": "other/repo"}})])],
        "no_label": [no_label],
        "review_order": [F.repo_exchange(), F.issues_exchange([F.issue_node(1, linked=[10])]),
                         F.pr_exchange(10, F.pr_response(10, reviews=[
                             ("APPROVED", "zed", "2026-01-03T00:00:00Z"), ("COMMENTED", "amy", "2026-01-01T00:00:00Z"),
                             ("CHANGES_REQUESTED", "amy", "2026-01-02T00:00:00Z"), ("APPROVED", "bob", "2026-01-02T00:00:00Z"),
                             ("APPROVED", "amy", "2026-01-04T00:00:00Z"), ("DISMISSED", "zed", "2026-01-05T00:00:00Z")],
                             review_next="R1")),
                         F.pr_exchange(10, F.pr_response(10, reviews=[("CHANGES_REQUESTED", "bob", "2026-01-01T00:00:00Z")]),
                                       after="R1")],
        "cycle": [F.repo_exchange(), F.issues_exchange([
            F.issue_node(1, body="Requires: #3"), F.issue_node(2, body="Requires: #1"), F.issue_node(3, body="Requires: #2"),
            F.issue_node(4, body="Requires: #2"), F.issue_node(5, body="Requires: #4, other/repo#1")])],
        "limit_exceeded": [F.repo_exchange(), F.issues_exchange([], total=201)],
        "no_data": [{"name": "repository", "variables": dict(F.BASE, label="gitmaps:crossing"),
                     "response": {"data": None, "errors": [{"message": "Bad credentials"}]}}],
    }
    vectors = []
    for name, exchanges in sorted(scenarios.items()):
        # The reference fetch mutates response objects in place (complete_linked_pull_requests),
        # so replay a copy and store the pristine exchanges.
        transport = ReplayTransport(copy.deepcopy(exchanges))
        try:
            snap = snapshot(transport, F.REPO)
            transport.assert_consumed()
            entry = {"snapshot_sha256": sha256_hex(canonical_bytes(snap)),
                     "map_sha256": sha256_hex(canonical_bytes(derive(snap)))}
        except GitHubError:
            entry = {"error": True}
        vectors.append(dict(entry, name=name, repo=F.REPO, label="gitmaps:crossing", numbers=None, exchanges=exchanges))
    return vectors


def render_vectors(accepted):
    from gitmaps.derive import check_publishable
    from gitmaps.render import render_html
    cases = []
    for i, v in enumerate(accepted):
        try:
            check_publishable(v["snapshot"])
        except PermissionError:
            cases.append({"case": i, "refused": True})
            continue
        html = render_html(derive(v["snapshot"])).encode("utf-8")
        cases.append({"case": i, "index_sha256": sha256_hex(html)})
    return cases


def template_vectors():
    from gitmaps.render import TEMPLATE, check_template, refresh_csp
    base = TEMPLATE.read_text(encoding="utf-8")
    csp = base[base.index('<meta http-equiv="Content-Security-Policy"'):]
    csp = csp[:csp.index(">") + 1]
    variants = {
        "original": [],
        "script_edited": [["TUTORIAL STARTED.", "TUTORIAL STARTED!", 1]],
        "two_scripts": [["</body>", "<script>void 0</script></body>", 1]],
        "no_executable_script": [["<script>\n", "<script defer>\n", 1]],
        "slot_missing": [['id="gitmaps-data">null</script>', 'id="gitmaps-data">{}</script>', 1]],
        "slot_duplicated": [["</main>", '</main><script type="application/json" id="gitmaps-data">null</script>', 1]],
        "csp_hash_removed": [["script-src 'sha256-", "script-src 'none' x-", 1]],
        "csp_duplicated": [[csp, csp + csp, 1]],
        "csp_hash_malformed": [["script-src 'sha256-", "script-src 'sha256-!", 1]],
        "unclosed_script_tail": [["</html>", "</html><script>", 1]],
        "edited_and_duplicated_csp": [["TUTORIAL STARTED.", "TUTORIAL STARTED!", 1], [csp, csp + csp, 1]],
    }
    out = []
    for name, edits in sorted(variants.items()):
        html = base
        for old, new, count in edits:
            assert old in html, (name, old)
            html = html.replace(old, new, count)
        entry = {"name": name, "edits": edits}
        try:
            check_template(html)
            entry["check"] = "ok"
        except ValueError:
            entry["check"] = "error"
        try:
            fixed = refresh_csp(html)
            entry["fixed_sha256"] = sha256_hex(fixed.encode("utf-8"))
            try:
                check_template(fixed)
                entry["fixed_check"] = "ok"
            except ValueError:
                entry["fixed_check"] = "error"
        except ValueError:
            entry["fixed_sha256"] = None
        out.append(entry)
    return {"template_sha256": sha256_hex(base.encode("utf-8")), "variants": out}


def pyrepr_table():
    ranges, start = [], None
    for c in range(0x80, 0x110001):
        nonprintable = c <= 0x10ffff and not chr(c).isprintable()
        if nonprintable and start is None:
            start = c
        elif not nonprintable and start is not None:
            ranges.append((start, c - 1))
            start = None
    lines = ["// @generated by scripts/generate_parity_vectors.py from Python %s (Unicode %s)."
             % (sys.version.split()[0], unicodedata.unidata_version),
             "// Non-ASCII code points for which Python's str.isprintable() is false; repr() escapes them.",
             "pub(crate) static NON_PRINTABLE: &[(u32, u32)] = &["]
    lines += ["    (0x%x, 0x%x)," % r for r in ranges]
    lines.append("];")
    (ROOT / "src" / "pyrepr_table.rs").write_text("\n".join(lines) + "\n", encoding="ascii")
    return len(ranges)


def queries_module():
    from gitmaps import fetch
    names = ["REPO_QUERY", "ISSUES_BY_LABEL_QUERY", "ISSUE_BY_NUMBER_QUERY", "LINKED_PULL_REQUESTS_QUERY",
             "PULL_REQUEST_QUERY"]
    lines = ["// @generated by scripts/generate_parity_vectors.py from gitmaps/fetch.py; do not edit.", ""]
    for name in names:
        text = getattr(fetch, name)
        assert '"#' not in text
        lines.append('pub const %s: &str = r#"%s"#;' % (name, text))
        lines.append("")
    (ROOT / "src" / "queries.rs").write_text("\n".join(lines), encoding="ascii")


def main():
    accepted, rejected = derive_vectors()
    sizes = {
        "canonical.json": write("canonical.json", canonical_vectors()),
        "repr.json": write("repr.json", repr_vectors()),
        "requires.json": write("requires.json", requires_vectors()),
        "derive.json": write("derive.json", {"accepted": accepted, "rejected": rejected}),
        "fetch.json": write("fetch.json", fetch_vectors()),
        "render.json": write("render.json", render_vectors(accepted)),
        "template.json": write("template.json", template_vectors()),
    }
    ranges = pyrepr_table()
    queries_module()
    for name, size in sizes.items():
        digest = hashlib.sha256((OUT / name).read_bytes()).hexdigest()
        print("%-15s %7d bytes  sha256 %s" % (name, size, digest))
    print("src/pyrepr_table.rs  %d ranges" % ranges)


if __name__ == "__main__":
    main()
