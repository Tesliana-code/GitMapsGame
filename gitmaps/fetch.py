"""Build an authoritative, normalised snapshot from the GitHub GraphQL API.

Webhook payloads are never read here: every run re-queries GitHub. The snapshot
contains no fetch timestamp, so an unchanged repository yields identical bytes.
"""
import hashlib
import re

from . import DEFAULT_LABEL, MAX_CROSSINGS, SNAPSHOT_SCHEMA
from .github import GitHubError

REPO_QUERY = """
query($owner:String!,$name:String!,$label:String!){
  repository(owner:$owner,name:$name){
    nameWithOwner isPrivate visibility
    label(name:$label){name}
  }
}"""

ISSUE_FIELDS = """
  number title url state stateReason createdAt updatedAt closedAt body
  assignees(first:20){totalCount nodes{login}}
  closedByPullRequestsReferences(first:25,includeClosedPrs:true){totalCount pageInfo{hasNextPage endCursor}
    nodes{number repository{nameWithOwner}}}
  timelineItems(last:1,itemTypes:[CLOSED_EVENT]){nodes{... on ClosedEvent{
    createdAt stateReason actor{login}
    closer{__typename ... on PullRequest{number repository{nameWithOwner}} ... on Commit{oid}}
  }}}
"""

ISSUES_BY_LABEL_QUERY = """
query($owner:String!,$name:String!,$label:String!,$after:String){
  repository(owner:$owner,name:$name){
    issues(labels:[$label],states:[OPEN,CLOSED],first:50,after:$after,orderBy:{field:CREATED_AT,direction:ASC}){
      totalCount pageInfo{hasNextPage endCursor}
      nodes{%s}
    }
  }
}""" % ISSUE_FIELDS

ISSUE_BY_NUMBER_QUERY = """
query($owner:String!,$name:String!,$number:Int!){
  repository(owner:$owner,name:$name){ issue(number:$number){%s} }
}""" % ISSUE_FIELDS

LINKED_PULL_REQUESTS_QUERY = """
query($owner:String!,$name:String!,$number:Int!,$after:String){
  repository(owner:$owner,name:$name){ issue(number:$number){
    closedByPullRequestsReferences(first:25,after:$after,includeClosedPrs:true){totalCount pageInfo{hasNextPage endCursor}
      nodes{number repository{nameWithOwner}}}
  }}
}"""

PULL_REQUEST_QUERY = """
query($owner:String!,$name:String!,$number:Int!,$after:String){
  repository(owner:$owner,name:$name){
    pullRequest(number:$number){
      number title url state merged mergedAt createdAt updatedAt
      author{login} headRefOid mergeCommit{oid} repository{nameWithOwner}
      reviews(first:100,after:$after){pageInfo{hasNextPage endCursor}
        nodes{state submittedAt author{login} commit{oid}}}
      commits(last:1){nodes{commit{oid statusCheckRollup{state
        contexts(first:100){totalCount nodes{__typename
          ... on CheckRun{name status conclusion}
          ... on StatusContext{context state}}}}}}}
    }
  }
}"""

_REQUIRES_LINE = re.compile(r"^\s*(?:[-*]\s*)?(?:\*\*)?requires(?:\*\*)?\s*:\s*(?:\*\*)?(.*)$", re.IGNORECASE)
_ISSUE_REF = re.compile(r"^#([1-9][0-9]*)$")
_FENCE = re.compile(r"^\s*(```|~~~)")


def parse_requires(body):
    """Extract `Requires: #N, #M` declarations outside code fences.

    Returns sorted unique issue numbers and every token that is not a plain
    same-repository `#N` reference (those make the crossing INVALID).
    """
    refs, unresolved, in_fence = set(), set(), False
    for line in (body or "").replace("\r\n", "\n").split("\n"):
        if _FENCE.match(line):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        match = _REQUIRES_LINE.match(line)
        if not match:
            continue
        tokens = [t for t in re.split(r"[,\s]+", match.group(1).strip()) if t]
        if [t.lower() for t in tokens] in ([], ["none"], ["-"]):
            continue
        for token in tokens:
            ref = _ISSUE_REF.match(token)
            if ref:
                refs.add(int(ref.group(1)))
            else:
                unresolved.add(token)
    return {"refs": sorted(refs), "unresolved": sorted(unresolved)}


def _login(actor):
    return actor["login"] if actor else None


def _lower(value):
    return value.lower() if value else None


def _normalise_issue(node):
    closed_event = None
    events = node["timelineItems"]["nodes"]
    if node["state"] == "CLOSED" and events:
        event, closer = events[-1], events[-1].get("closer")
        if closer is None:
            closer_out = None
        elif closer["__typename"] == "PullRequest":
            closer_out = {"type": "pull_request", "number": closer["number"],
                          "repository": closer["repository"]["nameWithOwner"]}
        elif closer["__typename"] == "Commit":
            closer_out = {"type": "commit", "oid": closer["oid"]}
        else:
            closer_out = {"type": "other", "typename": closer["__typename"]}
        closed_event = {"created_at": event["createdAt"], "actor": _login(event.get("actor")),
                        "state_reason": _lower(event.get("stateReason")), "closer": closer_out}
    linked = node["closedByPullRequestsReferences"]
    assignees = node["assignees"]
    body = (node.get("body") or "").replace("\r\n", "\n")
    return {
        "number": node["number"],
        "title": node["title"],
        "url": node["url"],
        "state": node["state"].lower(),
        "state_reason": _lower(node.get("stateReason")),
        "created_at": node["createdAt"],
        "updated_at": node["updatedAt"],
        "closed_at": node.get("closedAt"),
        "body_sha256": hashlib.sha256(body.encode("utf-8")).hexdigest(),
        "requires": parse_requires(body),
        "assignees": sorted(a["login"] for a in assignees["nodes"]),
        "assignees_complete": assignees["totalCount"] == len(assignees["nodes"]),
        "linked_pull_requests": sorted(
            ({"number": p["number"], "repository": p["repository"]["nameWithOwner"]} for p in linked["nodes"]),
            key=lambda p: (p["repository"], p["number"])),
        "linked_pull_requests_complete": linked["totalCount"] == len(linked["nodes"]),
        "closed_event": closed_event,
    }


def _ci_from_commit(commit_node, errors):
    if commit_node is None:
        return {"commit": None, "rollup": None, "fetch_error": False, "contexts": [], "contexts_complete": True}
    commit = commit_node["commit"]
    rollup = commit.get("statusCheckRollup")
    contexts, complete = [], True
    if rollup:
        conn = rollup["contexts"]
        complete = conn["totalCount"] == len(conn["nodes"])
        for c in conn["nodes"]:
            if c["__typename"] == "CheckRun":
                contexts.append({"kind": "check_run", "name": c["name"], "status": _lower(c["status"]),
                                 "conclusion": _lower(c.get("conclusion"))})
            elif c["__typename"] == "StatusContext":
                contexts.append({"kind": "status", "name": c["context"], "status": None,
                                 "conclusion": _lower(c["state"])})
        contexts.sort(key=lambda c: (c["kind"], c["name"], c["conclusion"] or ""))
    rollup_error = any("statusCheckRollup" in [str(p) for p in e.get("path") or []] for e in errors)
    return {"commit": commit["oid"], "rollup": _lower(rollup["state"]) if rollup else None,
            "fetch_error": rollup_error, "contexts": contexts, "contexts_complete": complete}


class Fetcher:
    def __init__(self, transport, owner, name):
        self._transport = transport
        self.owner, self.name = owner, name
        self.full_name = "%s/%s" % (owner, name)

    def _query(self, query_name, query, **variables):
        variables = dict({"owner": self.owner, "name": self.name}, **variables)
        response = self._transport(query_name, query, variables)
        if "data" not in response or response["data"] is None:
            messages = "; ".join(e.get("message", "?") for e in response.get("errors", []))
            raise GitHubError("%s failed: %s" % (query_name, messages or "no data"))
        return response["data"], response.get("errors", [])

    def repository(self, label):
        data, _ = self._query("repository", REPO_QUERY, label=label)
        repo = data["repository"]
        if repo is None:
            raise GitHubError("repository %s not found or not accessible" % self.full_name)
        return repo

    def complete_linked_pull_requests(self, issue):
        """Follow closedByPullRequestsReferences pagination so no linked PR is silently dropped."""
        conn = issue["closedByPullRequestsReferences"]
        while conn["pageInfo"]["hasNextPage"]:
            data, _ = self._query("linked_pull_requests", LINKED_PULL_REQUESTS_QUERY,
                                  number=issue["number"], after=conn["pageInfo"]["endCursor"])
            page = data["repository"]["issue"]["closedByPullRequestsReferences"]
            conn = {"totalCount": page["totalCount"], "pageInfo": page["pageInfo"],
                    "nodes": conn["nodes"] + page["nodes"]}
        issue["closedByPullRequestsReferences"] = conn
        return issue

    def issues_by_label(self, label):
        nodes, after = [], None
        while True:
            data, _ = self._query("issues_by_label", ISSUES_BY_LABEL_QUERY, label=label, after=after)
            conn = data["repository"]["issues"]
            if conn["totalCount"] > MAX_CROSSINGS:
                raise GitHubError("%d crossings exceed the limit of %d" % (conn["totalCount"], MAX_CROSSINGS))
            nodes.extend(conn["nodes"])
            if not conn["pageInfo"]["hasNextPage"]:
                return nodes
            after = conn["pageInfo"]["endCursor"]

    def issue_by_number(self, number):
        data, _ = self._query("issue_by_number", ISSUE_BY_NUMBER_QUERY, number=number)
        issue = data["repository"]["issue"]
        if issue is None:
            raise GitHubError("issue #%d not found in %s" % (number, self.full_name))
        return issue

    def pull_request(self, number):
        reviews, after, first = [], None, None
        while True:
            data, errors = self._query("pull_request", PULL_REQUEST_QUERY, number=number, after=after)
            pr = data["repository"]["pullRequest"]
            if pr is None:
                return None
            first = first or (pr, errors)
            reviews.extend(pr["reviews"]["nodes"])
            if not pr["reviews"]["pageInfo"]["hasNextPage"]:
                break
            after = pr["reviews"]["pageInfo"]["endCursor"]
        pr, errors = first
        commits = pr["commits"]["nodes"]
        return {
            "number": pr["number"],
            "title": pr["title"],
            "url": pr["url"],
            "repository": pr["repository"]["nameWithOwner"],
            "state": pr["state"].lower(),
            "merged": bool(pr["merged"]),
            "merged_at": pr.get("mergedAt"),
            "created_at": pr["createdAt"],
            "updated_at": pr["updatedAt"],
            "author": _login(pr.get("author")),
            "head_oid": pr["headRefOid"],
            "merge_commit_oid": pr["mergeCommit"]["oid"] if pr.get("mergeCommit") else None,
            "reviews": sorted(({"state": r["state"].lower(), "submitted_at": r.get("submittedAt"),
                                "author": _login(r.get("author")),
                                "commit": r["commit"]["oid"] if r.get("commit") else None} for r in reviews),
                              key=lambda r: (r["submitted_at"] or "", r["author"] or "", r["state"], r["commit"] or "")),
            "ci": _ci_from_commit(commits[-1] if commits else None, errors),
            "fetch_errors": sorted({e.get("message", "?") for e in errors}),
        }


def snapshot(transport, repo, label=DEFAULT_LABEL, numbers=None):
    """Query GitHub and return the normalised snapshot dictionary."""
    owner, _, name = repo.partition("/")
    if not owner or not name:
        raise GitHubError("repository must be owner/name")
    fetcher = Fetcher(transport, owner, name)
    meta = fetcher.repository(label)
    if numbers:
        selection = {"mode": "numbers", "numbers": sorted(set(numbers))}
        raw_issues = [fetcher.issue_by_number(n) for n in selection["numbers"]]
    else:
        selection = {"mode": "label", "label": label}
        raw_issues = fetcher.issues_by_label(label) if meta["label"] else []
    full_name = meta["nameWithOwner"]
    raw_issues = [fetcher.complete_linked_pull_requests(n) for n in raw_issues]
    issues = sorted((_normalise_issue(n) for n in raw_issues), key=lambda i: i["number"])

    wanted = set()
    for issue in issues:
        wanted.update(p["number"] for p in issue["linked_pull_requests"] if p["repository"] == full_name)
        closer = (issue["closed_event"] or {}).get("closer") or {}
        if closer.get("type") == "pull_request" and closer["repository"] == full_name:
            wanted.add(closer["number"])
    pulls = [pr for pr in (fetcher.pull_request(n) for n in sorted(wanted)) if pr is not None]

    return {
        "schema": SNAPSHOT_SCHEMA,
        "source": {"api": "github-graphql", "selection": selection},
        "repository": {"name_with_owner": full_name, "is_private": bool(meta["isPrivate"]),
                       "visibility": meta["visibility"].lower(), "label_exists": meta["label"] is not None},
        "issues": issues,
        "pull_requests": pulls,
    }
