"""Offline tests for real mode. No network: recorded fixtures are replayed byte for byte.

Rule tests build snapshots by hand. Those are constructed test inputs, never shipped
as map data; real-repository evidence lives only in tests/fixtures/recorded/.
"""
import copy
import json
import random
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from gitmaps import SNAPSHOT_SCHEMA, canonical_bytes, sha256_hex  # noqa: E402
from gitmaps.derive import check_publishable, ci_evidence, derive, review_evidence  # noqa: E402
from gitmaps.fetch import parse_requires, snapshot  # noqa: E402
from gitmaps.github import GitHubError, ReplayTransport  # noqa: E402

RECORDED = ROOT / "tests" / "fixtures" / "recorded"
REPO = "example/constructed"


def issue(number, requires=(), state="open", reason=None, assignees=(), linked=(), closer=None, unresolved=()):
    closed_event = None
    if state == "closed":
        closed_event = {"created_at": "2026-01-02T00:00:00Z", "actor": "closer-login",
                        "state_reason": reason, "closer": closer}
    return {"number": number, "title": "Constructed #%d" % number, "url": "https://example.invalid/%d" % number,
            "state": state, "state_reason": reason, "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-0%dT00:00:00Z" % min(number, 9), "closed_at": None, "body_sha256": "0" * 64,
            "requires": {"refs": sorted(requires), "unresolved": sorted(unresolved)},
            "assignees": sorted(assignees), "assignees_complete": True,
            "linked_pull_requests": [{"number": n, "repository": REPO} for n in linked],
            "linked_pull_requests_complete": True, "closed_event": closed_event}


def pr(number, state="open", merged=False, rollup="success", reviews=(), fetch_error=False, head="h0"):
    return {"number": number, "title": "PR %d" % number, "url": "https://example.invalid/pr/%d" % number,
            "repository": REPO, "state": state, "merged": merged, "merged_at": None,
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z",
            "author": "pr-author", "head_oid": head, "merge_commit_oid": "m%d" % number if merged else None,
            "reviews": list(reviews), "fetch_errors": [],
            "ci": {"commit": head, "rollup": rollup, "fetch_error": fetch_error, "contexts": [], "contexts_complete": True}}


def by_pr(number):
    return {"type": "pull_request", "number": number, "repository": REPO}


def snap(issues, pulls=(), private=False):
    return {"schema": SNAPSHOT_SCHEMA, "source": {"api": "github-graphql", "selection": {"mode": "label", "label": "x"}},
            "repository": {"name_with_owner": REPO, "is_private": private,
                           "visibility": "private" if private else "public", "label_exists": True},
            "issues": list(issues), "pull_requests": list(pulls)}


def states(s):
    return {c["number"]: c["state"] for c in derive(s)["crossings"]}


class RequiresParsing(unittest.TestCase):
    def test_forms(self):
        body = "Intro\r\nRequires: #3, #1\n- **Requires:** #2 #3\nrequires: none\n```\nRequires: #99\n```\n"
        self.assertEqual(parse_requires(body), {"refs": [1, 2, 3], "unresolved": []})

    def test_unresolved_tokens_kept(self):
        self.assertEqual(parse_requires("Requires: #1, other/repo#2, #x"),
                         {"refs": [1], "unresolved": ["#x", "other/repo#2"]})

    def test_no_declaration(self):
        self.assertEqual(parse_requires(None), {"refs": [], "unresolved": []})


class StateRules(unittest.TestCase):
    def test_chain_progression(self):
        s = snap([issue(1, state="closed", reason="completed", closer=by_pr(10)),
                  issue(2, requires=[1]), issue(3, requires=[2])],
                 [pr(10, state="merged", merged=True)])
        self.assertEqual(states(s), {1: "PROVEN", 2: "DISCOVERED", 3: "LOCKED"})

    def test_locked_overrides_claimed(self):
        s = snap([issue(1), issue(2, requires=[1], assignees=["someone"], linked=[11])], [pr(11)])
        result = {c["number"]: c for c in derive(s)["crossings"]}
        self.assertEqual(result[2]["state"], "LOCKED")
        self.assertIn("claim ignored until dependencies are PROVEN", result[2]["reasons"])

    def test_locked_overrides_merged_closure(self):
        s = snap([issue(1), issue(2, requires=[1], state="closed", reason="completed", closer=by_pr(10))],
                 [pr(10, state="merged", merged=True)])
        self.assertEqual(states(s)[2], "LOCKED")

    def test_claimed_by_assignee_or_open_pr_only(self):
        s = snap([issue(1, assignees=["a"]), issue(2, linked=[20]), issue(3, linked=[30])],
                 [pr(20), pr(30, state="closed")])
        self.assertEqual(states(s), {1: "CLAIMED", 2: "CLAIMED", 3: "DISCOVERED"})

    def test_invalid_missing_unresolved_and_cycles(self):
        s = snap([issue(1, requires=[99]), issue(2, unresolved=["other/repo#4"]),
                  issue(3, requires=[4]), issue(4, requires=[3]), issue(5, requires=[5]),
                  issue(6, requires=[3]), issue(7, requires=[1])])
        result = {c["number"]: c for c in derive(s)["crossings"]}
        self.assertEqual({n: c["state"] for n, c in result.items()},
                         {1: "INVALID", 2: "INVALID", 3: "INVALID", 4: "INVALID", 5: "INVALID",
                          6: "LOCKED", 7: "LOCKED"})
        self.assertIn("dependency cycle: #3 -> #4", result[3]["reasons"])
        self.assertIn("dependency #1 is INVALID", result[7]["reasons"])

    def test_closure_without_merged_pr_is_not_proven(self):
        s = snap([issue(1, state="closed", reason="completed", closer=None),
                  issue(2, state="closed", reason="not_planned", closer=by_pr(20)),
                  issue(3, state="closed", reason="completed", closer=by_pr(30)),
                  issue(4, state="closed", reason="completed", closer={"type": "commit", "oid": "abc"}),
                  issue(5, state="closed", reason="completed",
                        closer={"type": "pull_request", "number": 50, "repository": "someone/else"}),
                  issue(6, state="closed", reason="completed", closer=by_pr(60)),
                  issue(7, state="closed", reason="duplicate", closer=None)],
                 [pr(20, state="merged", merged=True), pr(30, state="closed")])
        self.assertEqual(set(states(s).values()), {"CLOSED_UNPROVEN"})

    def test_dependent_of_unproven_closure_stays_locked(self):
        s = snap([issue(1, state="closed", reason="completed"), issue(2, requires=[1])])
        self.assertEqual(states(s)[2], "LOCKED")


class Evidence(unittest.TestCase):
    def test_ci_states_distinct(self):
        self.assertEqual(ci_evidence(pr(1, rollup="success"))["status"], "success")
        self.assertEqual(ci_evidence(pr(1, rollup="failure"))["status"], "failure")
        self.assertEqual(ci_evidence(pr(1, rollup="error"))["status"], "failure")
        self.assertEqual(ci_evidence(pr(1, rollup="pending"))["status"], "pending")
        self.assertEqual(ci_evidence(pr(1, rollup=None))["status"], "absent")
        self.assertEqual(ci_evidence(pr(1, rollup=None, fetch_error=True))["status"], "unknown")
        self.assertEqual(ci_evidence(None)["status"], "unknown")

    def test_review_latest_decision_per_author(self):
        reviews = [{"state": "approved", "author": "a", "commit": "old", "submitted_at": "1"},
                   {"state": "commented", "author": "b", "commit": "h0", "submitted_at": "2"},
                   {"state": "changes_requested", "author": "c", "commit": "h0", "submitted_at": "3"},
                   {"state": "approved", "author": "c", "commit": "h0", "submitted_at": "4"},
                   {"state": "approved", "author": "d", "commit": "h0", "submitted_at": "5"},
                   {"state": "dismissed", "author": "d", "commit": "h0", "submitted_at": "6"}]
        result = review_evidence(pr(1, reviews=reviews))
        self.assertEqual(result["status"], "approved")
        self.assertEqual(result["approved_by"], ["a", "c"])
        self.assertEqual(result["approved_on_head"], ["c"])
        self.assertEqual(review_evidence(pr(1))["status"], "absent")
        self.assertEqual(review_evidence(None)["status"], "unknown")

    def test_contributors_only_from_snapshot(self):
        s = snap([issue(1, assignees=["assignee-x"], linked=[10])],
                 [pr(10, reviews=[{"state": "commented", "author": "rev-y", "commit": "h0", "submitted_at": "1"}])])
        logins = {c["login"] for c in derive(s)["contributors"]}
        self.assertEqual(logins, {"assignee-x", "pr-author", "rev-y"})


class Determinism(unittest.TestCase):
    def test_same_snapshot_same_bytes(self):
        issues = [issue(n, requires=[n - 1] if n > 1 else []) for n in range(1, 8)]
        base = canonical_bytes(derive(snap(issues)))
        self.assertEqual(canonical_bytes(derive(snap(copy.deepcopy(issues)))), base)

    def test_non_canonical_snapshot_rejected(self):
        issues = [issue(n) for n in range(1, 8)]
        random.Random(1).shuffle(issues)
        with self.assertRaises(ValueError):
            derive(snap(issues))
        with self.assertRaises(ValueError):
            derive(snap([issue(1), issue(1)]))

    def test_unsupported_schema_rejected(self):
        with self.assertRaises(ValueError):
            derive({"schema": "other"})


class Publication(unittest.TestCase):
    def test_private_denied_by_default(self):
        with self.assertRaises(PermissionError):
            check_publishable(snap([], private=True))
        check_publishable(snap([], private=True), allow_private=True)
        check_publishable(snap([]))


class RecordedRealEvidence(unittest.TestCase):
    """Replays raw GitHub GraphQL exchanges recorded on 2026-10-21 (read-only)."""

    def replay(self, name):
        recording = json.loads((RECORDED / ("%s.recording.json" % name)).read_text())
        transport = ReplayTransport(recording["exchanges"])
        result = snapshot(transport, recording["repo"], label=recording["label"], numbers=recording["numbers"])
        transport.assert_consumed()
        return result

    def assert_reproducible(self, name):
        result = self.replay(name)
        self.assertEqual(canonical_bytes(result), (RECORDED / ("%s.snapshot.json" % name)).read_bytes())
        derived = derive(result)
        self.assertEqual(canonical_bytes(derived), (RECORDED / ("%s.map.json" % name)).read_bytes())
        self.assertEqual(derived["source"]["snapshot_sha256"], sha256_hex(canonical_bytes(result)))
        return derived

    def test_gitmapsgame_current_state(self):
        derived = self.assert_reproducible("gitmapsgame")
        self.assertEqual(derived["repository"]["visibility"], "public")
        self.assertFalse(derived["repository"]["label_exists"])
        self.assertEqual(derived["crossings"], [])
        self.assertEqual(derived["contributors"], [])

    def test_cli_cli_closure_and_claim_evidence(self):
        derived = self.assert_reproducible("cli-cli")
        by_number = {c["number"]: c for c in derived["crossings"]}
        self.assertEqual({n: c["state"] for n, c in by_number.items()}, {
            3718: "PROVEN", 14718: "PROVEN",                         # closed by merged PR
            14711: "CLOSED_UNPROVEN", 14692: "CLOSED_UNPROVEN",       # closed manually
            14780: "CLOSED_UNPROVEN",                                 # not planned
            13768: "CLAIMED", 14703: "CLAIMED", 11247: "CLAIMED",     # open linked PR
            14755: "DISCOVERED"})
        proven = by_number[3718]["pull_requests"][0]
        self.assertEqual((proven["number"], proven["merge_commit_oid"], proven["review"]["approved_by"], proven["ci"]["status"]),
                         (14148, "1531837d8578fecd64eb8050d44a3665d85f9219", ["babakks"], "success"))
        self.assertEqual(by_number[13768]["pull_requests"][0]["ci"]["status"], "absent")
        self.assertEqual(by_number[11247]["pull_requests"][0]["review"]["status"], "changes_requested")

    def test_replay_rejects_divergent_queries(self):
        recording = json.loads((RECORDED / "gitmapsgame.recording.json").read_text())
        with self.assertRaises(GitHubError):
            snapshot(ReplayTransport(recording["exchanges"]), "Tesliana-code/Other")


if __name__ == "__main__":
    unittest.main()
