"""Fetch-layer tests: pagination, incomplete and unreadable evidence.

GraphQL responses here are constructed test inputs shaped like the recorded ones in
tests/fixtures/recorded/; they exercise paths the live fixtures cannot reach on demand.
"""
import copy
import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from gitmaps import MAX_CROSSINGS  # noqa: E402
from gitmaps.derive import derive  # noqa: E402
from gitmaps.fetch import snapshot  # noqa: E402
from gitmaps import canonical_bytes  # noqa: E402
from gitmaps.github import GitHubError, RecordingTransport, ReplayTransport  # noqa: E402

OWNER, NAME = "example", "constructed"
REPO = OWNER + "/" + NAME
BASE = {"owner": OWNER, "name": NAME}


def page(has_next, cursor=None):
    return {"hasNextPage": has_next, "endCursor": cursor}


def issue_node(number, body="", state="OPEN", reason=None, assignees=(), assignee_total=None,
               linked=(), linked_total=None, linked_next=None, closer=None):
    events = []
    if state == "CLOSED":
        events = [{"createdAt": "2026-01-02T00:00:00Z", "stateReason": reason, "actor": {"login": "closer"},
                   "closer": closer}]
    return {"number": number, "title": "Constructed %d" % number, "url": "https://github.com/%s/issues/%d" % (REPO, number),
            "state": state, "stateReason": reason, "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:00Z", "closedAt": None, "body": body,
            "assignees": {"totalCount": assignee_total if assignee_total is not None else len(assignees),
                          "nodes": [{"login": a} for a in assignees]},
            "closedByPullRequestsReferences": {
                "totalCount": linked_total if linked_total is not None else len(linked),
                "pageInfo": page(linked_next is not None, linked_next),
                "nodes": [{"number": n, "repository": {"nameWithOwner": REPO}} for n in linked]},
            "timelineItems": {"nodes": events}}


def pr_response(number, reviews=(), review_next=None, rollup="SUCCESS", contexts=(), context_total=None,
                errors=None, merged=False):
    pr = {"number": number, "title": "PR %d" % number, "url": "https://github.com/%s/pull/%d" % (REPO, number),
          "state": "MERGED" if merged else "OPEN", "merged": merged, "mergedAt": None,
          "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z",
          "author": {"login": "author"}, "headRefOid": "a" * 40,
          "mergeCommit": {"oid": "b" * 40} if merged else None, "repository": {"nameWithOwner": REPO},
          "reviews": {"pageInfo": page(review_next is not None, review_next),
                      "nodes": [{"state": s, "submittedAt": t, "author": {"login": a}, "commit": {"oid": "a" * 40}}
                                for s, a, t in reviews]},
          "commits": {"nodes": [{"commit": {"oid": "a" * 40, "statusCheckRollup": None if rollup is None else {
              "state": rollup,
              "contexts": {"totalCount": context_total if context_total is not None else len(contexts),
                           "nodes": [{"__typename": "CheckRun", "name": c, "status": "COMPLETED", "conclusion": "SUCCESS"}
                                     for c in contexts]}}}}]}}
    response = {"data": {"repository": {"pullRequest": pr}}}
    if errors:
        response["errors"] = errors
    return response


def repo_exchange(private=False):
    return {"name": "repository", "variables": dict(BASE, label="gitmaps:crossing"),
            "response": {"data": {"repository": {"nameWithOwner": REPO, "isPrivate": private,
                                                 "visibility": "PRIVATE" if private else "PUBLIC",
                                                 "label": {"name": "gitmaps:crossing"}}}}}


def issues_exchange(nodes, after=None, next_cursor=None, total=None):
    return {"name": "issues_by_label", "variables": dict(BASE, label="gitmaps:crossing", after=after),
            "response": {"data": {"repository": {"issues": {
                "totalCount": total if total is not None else len(nodes),
                "pageInfo": page(next_cursor is not None, next_cursor), "nodes": nodes}}}}}


def pr_exchange(number, response, after=None):
    return {"name": "pull_request", "variables": dict(BASE, number=number, after=after), "response": response}


def run(exchanges):
    transport = ReplayTransport(exchanges)
    result = snapshot(transport, REPO)
    transport.assert_consumed()
    return result


class Pagination(unittest.TestCase):
    def test_issue_linked_pr_and_review_pages_are_all_followed(self):
        first = issue_node(1, linked=[10], linked_total=2, linked_next="L1")
        exchanges = [
            repo_exchange(),
            issues_exchange([first], next_cursor="I1", total=2),
            issues_exchange([issue_node(2, body="Requires: #1")], after="I1", total=2),
            {"name": "linked_pull_requests", "variables": dict(BASE, number=1, after="L1"),
             "response": {"data": {"repository": {"issue": {"closedByPullRequestsReferences": {
                 "totalCount": 2, "pageInfo": page(False),
                 "nodes": [{"number": 11, "repository": {"nameWithOwner": REPO}}]}}}}}},
            pr_exchange(10, pr_response(10, reviews=[("APPROVED", "r1", "1")], review_next="R1")),
            pr_exchange(10, pr_response(10, reviews=[("CHANGES_REQUESTED", "r2", "2")]), after="R1"),
            pr_exchange(11, pr_response(11)),
        ]
        result = run(exchanges)
        self.assertEqual([i["number"] for i in result["issues"]], [1, 2])
        self.assertEqual([p["number"] for p in result["issues"][0]["linked_pull_requests"]], [10, 11])
        self.assertTrue(result["issues"][0]["linked_pull_requests_complete"])
        self.assertEqual([r["author"] for r in result["pull_requests"][0]["reviews"]], ["r1", "r2"])
        crossing = derive(result)["crossings"][0]
        self.assertEqual(crossing["pull_requests"][0]["review"]["status"], "changes_requested")
        self.assertTrue(crossing["evidence_complete"])

    def test_truncated_lists_are_reported_not_hidden(self):
        exchanges = [
            repo_exchange(),
            issues_exchange([issue_node(1, assignees=["a"], assignee_total=3, linked=[10])]),
            pr_exchange(10, pr_response(10, contexts=["build"], context_total=150)),
        ]
        crossing = derive(run(exchanges))["crossings"][0]
        self.assertFalse(crossing["evidence_complete"])
        self.assertIn("assignee list truncated by the API", crossing["evidence_gaps"])
        self.assertFalse(crossing["pull_requests"][0]["ci"]["checks_complete"])
        self.assertEqual(crossing["pull_requests"][0]["ci"]["status"], "success")  # rollup still authoritative

    def test_crossing_limit_fails_closed(self):
        exchanges = [repo_exchange(), issues_exchange([], total=MAX_CROSSINGS + 1)]
        with self.assertRaises(GitHubError):
            run(exchanges)

    def test_missing_label_means_no_issue_query(self):
        exchange = repo_exchange()
        exchange["response"]["data"]["repository"]["label"] = None
        result = run([exchange])
        self.assertEqual(result["issues"], [])
        self.assertFalse(result["repository"]["label_exists"])


class Recording(unittest.TestCase):
    def test_recording_with_linked_pr_pagination_replays(self):
        """Regression: fetch merges linked-PR pages into the response; the recording must not change."""
        exchanges = [
            repo_exchange(),
            issues_exchange([issue_node(1, linked=[10], linked_total=2, linked_next="L1")]),
            {"name": "linked_pull_requests", "variables": dict(BASE, number=1, after="L1"),
             "response": {"data": {"repository": {"issue": {"closedByPullRequestsReferences": {
                 "totalCount": 2, "pageInfo": page(False),
                 "nodes": [{"number": 11, "repository": {"nameWithOwner": REPO}}]}}}}}},
            pr_exchange(10, pr_response(10)),
            pr_exchange(11, pr_response(11)),
        ]
        recorder = RecordingTransport(ReplayTransport(copy.deepcopy(exchanges)))  # stands in for GitHub
        live = snapshot(recorder, REPO)
        written = json.loads(json.dumps(recorder.exchanges))  # as stored in a recording file
        self.assertEqual(written, json.loads(json.dumps(exchanges)))
        replayed = snapshot(ReplayTransport(written), REPO)
        self.assertEqual(canonical_bytes(replayed), canonical_bytes(live))


class UnknownEvidence(unittest.TestCase):
    def test_unreadable_check_status_is_unknown_not_absent(self):
        error = {"message": "Resource not accessible by integration",
                 "path": ["repository", "pullRequest", "commits", "nodes", 0, "commit", "statusCheckRollup"]}
        exchanges = [repo_exchange(), issues_exchange([issue_node(1, linked=[10])]),
                     pr_exchange(10, pr_response(10, rollup=None, errors=[error]))]
        ci = derive(run(exchanges))["crossings"][0]["pull_requests"][0]["ci"]
        self.assertEqual(ci["status"], "unknown")

    def test_no_checks_is_absent(self):
        exchanges = [repo_exchange(), issues_exchange([issue_node(1, linked=[10])]),
                     pr_exchange(10, pr_response(10, rollup=None))]
        self.assertEqual(derive(run(exchanges))["crossings"][0]["pull_requests"][0]["ci"]["status"], "absent")

    def test_unreadable_closing_pr_blocks_proven(self):
        closer = {"__typename": "PullRequest", "number": 10, "repository": {"nameWithOwner": REPO}}
        exchanges = [repo_exchange(),
                     issues_exchange([issue_node(1, state="CLOSED", reason="COMPLETED", linked=[10], closer=closer)]),
                     pr_exchange(10, {"data": {"repository": {"pullRequest": None}}})]
        crossing = derive(run(exchanges))["crossings"][0]
        self.assertEqual(crossing["state"], "CLOSED_UNPROVEN")
        self.assertIn("closing pull request #10 could not be read", crossing["reasons"])
        self.assertIn("pull request #10 could not be read", crossing["evidence_gaps"])

    def test_cross_repository_closer_not_fetched_and_not_proven(self):
        closer = {"__typename": "PullRequest", "number": 5, "repository": {"nameWithOwner": "other/repo"}}
        exchanges = [repo_exchange(),
                     issues_exchange([issue_node(1, state="CLOSED", reason="COMPLETED", closer=closer)])]
        crossing = derive(run(exchanges))["crossings"][0]
        self.assertEqual(crossing["state"], "CLOSED_UNPROVEN")

    def test_query_without_data_fails_closed(self):
        exchanges = [{"name": "repository", "variables": dict(BASE, label="gitmaps:crossing"),
                      "response": {"data": None, "errors": [{"message": "Bad credentials"}]}}]
        with self.assertRaises(GitHubError):
            run(exchanges)


class DependencyCycles(unittest.TestCase):
    def test_three_cycle_and_downstream(self):
        nodes = [issue_node(1, body="Requires: #3"), issue_node(2, body="Requires: #1"),
                 issue_node(3, body="Requires: #2"), issue_node(4, body="Requires: #2"),
                 issue_node(5, body="Requires: #4, other/repo#1")]
        states = {c["number"]: c["state"] for c in derive(run([repo_exchange(), issues_exchange(nodes)]))["crossings"]}
        self.assertEqual(states, {1: "INVALID", 2: "INVALID", 3: "INVALID", 4: "LOCKED", 5: "INVALID"})


if __name__ == "__main__":
    unittest.main()
