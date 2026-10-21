"""Pure snapshot -> map derivation. No I/O, clock, randomness or network.

State precedence, evaluated per crossing:
  INVALID          missing dependency, unresolved reference or dependency cycle
  LOCKED           any dependency is not PROVEN (overrides CLAIMED and a merged closure)
  PROVEN           closed as completed by a merged pull request in the same repository
  CLOSED_UNPROVEN  closed without that evidence (manual close, not planned, unmerged closer...)
  CLAIMED          open, with an assignee or an open linked pull request
  DISCOVERED       open, all dependencies PROVEN, nobody working on it
"""
from . import MAP_SCHEMA, SNAPSHOT_SCHEMA, canonical_bytes, sha256_hex

CI_STATUS = {"success": "success", "failure": "failure", "error": "failure",
             "pending": "pending", "expected": "pending"}


def ci_evidence(pr):
    """success | failure | pending | absent (no checks reported) | unknown (could not be read)."""
    if pr is None:
        return {"status": "unknown", "commit": None, "detail": "pull request evidence unavailable"}
    ci = pr["ci"]
    if ci["fetch_error"]:
        return {"status": "unknown", "commit": ci["commit"], "detail": "check status could not be read"}
    if ci["commit"] is None:
        return {"status": "absent", "commit": None, "detail": "pull request has no commits"}
    if ci["rollup"] is None:
        return {"status": "absent", "commit": ci["commit"], "detail": "no checks reported for head commit"}
    status = CI_STATUS.get(ci["rollup"], "unknown")
    return {"status": status, "commit": ci["commit"], "detail": "rollup " + ci["rollup"],
            "checks": [{"name": c["name"], "conclusion": c["conclusion"]} for c in ci["contexts"]],
            "checks_complete": ci["contexts_complete"]}


def review_evidence(pr):
    """approved | changes_requested | absent (no decisive review) | unknown."""
    if pr is None:
        return {"status": "unknown", "approved_by": [], "changes_requested_by": [], "approved_on_head": []}
    latest = {}
    for review in pr["reviews"]:  # chronological
        if review["author"] and review["state"] in ("approved", "changes_requested", "dismissed"):
            latest[review["author"]] = review
    approved = sorted(a for a, r in latest.items() if r["state"] == "approved")
    changes = sorted(a for a, r in latest.items() if r["state"] == "changes_requested")
    on_head = sorted(a for a in approved if latest[a]["commit"] == pr["head_oid"])
    status = "changes_requested" if changes else "approved" if approved else "absent"
    return {"status": status, "approved_by": approved, "changes_requested_by": changes,
            "approved_on_head": on_head}


def _strongly_connected(graph):
    """Tarjan's algorithm over sorted nodes, so results are deterministic."""
    index, low, stack, on_stack, out, counter = {}, {}, [], set(), [], [0]

    def visit(v):
        index[v] = low[v] = counter[0]
        counter[0] += 1
        stack.append(v)
        on_stack.add(v)
        for w in graph[v]:
            if w not in index:
                visit(w)
                low[v] = min(low[v], low[w])
            elif w in on_stack:
                low[v] = min(low[v], index[w])
        if low[v] == index[v]:
            component = []
            while True:
                w = stack.pop()
                on_stack.discard(w)
                component.append(w)
                if w == v:
                    break
            out.append(sorted(component))

    for v in sorted(graph):
        if v not in index:
            visit(v)
    return out


def _closure_proof(issue, pulls, repo):
    """Return (closing PR number or None, reason). Fail closed on any missing evidence."""
    if issue["state"] != "closed":
        return None, "open"
    if issue["state_reason"] != "completed":
        return None, "closed as %s" % (issue["state_reason"] or "unknown reason")
    event = issue["closed_event"]
    if event is None:
        return None, "close event not available"
    closer = event["closer"]
    if closer is None:
        return None, "closed manually, not by a pull request"
    if closer["type"] != "pull_request":
        return None, "closed by %s, not by a pull request" % closer["type"].replace("_", " ")
    if closer["repository"] != repo:
        return None, "closed by pull request in another repository (%s#%d)" % (closer["repository"], closer["number"])
    pr = pulls.get(closer["number"])
    if pr is None:
        return None, "closing pull request #%d could not be read" % closer["number"]
    if not pr["merged"]:
        return None, "closing pull request #%d is not merged" % closer["number"]
    return closer["number"], "closed by merged pull request #%d" % closer["number"]


def derive(snapshot):
    if snapshot.get("schema") != SNAPSHOT_SCHEMA:
        raise ValueError("unsupported snapshot schema: %r" % snapshot.get("schema"))
    for key in ("issues", "pull_requests"):
        numbers = [item["number"] for item in snapshot[key]]
        if numbers != sorted(set(numbers)):
            raise ValueError("snapshot %s are not in canonical (sorted, unique) order" % key)
    repo = snapshot["repository"]["name_with_owner"]
    issues = {i["number"]: i for i in snapshot["issues"]}
    pulls = {p["number"]: p for p in snapshot["pull_requests"] if p["repository"] == repo}

    problems = {n: [] for n in issues}
    graph = {n: [] for n in issues}
    for n, issue in issues.items():
        for token in issue["requires"]["unresolved"]:
            problems[n].append("unresolved reference %r" % token)
        for ref in issue["requires"]["refs"]:
            if ref not in issues:
                problems[n].append("missing dependency #%d (not a crossing in this snapshot)" % ref)
            else:
                graph[n].append(ref)
    for component in _strongly_connected(graph):
        if len(component) > 1 or component[0] in graph[component[0]]:
            cycle = " -> ".join("#%d" % c for c in component)
            for n in component:
                problems[n].append("dependency cycle: " + cycle)

    states, reasons = {}, {}

    def resolve(n):
        if n in states:
            return states[n]
        issue = issues[n]
        if problems[n]:
            states[n], reasons[n] = "INVALID", sorted(problems[n])
            return states[n]
        blocking = sorted(d for d in graph[n] if resolve(d) != "PROVEN")
        blocked = ["dependency #%d is %s" % (d, states[d]) for d in blocking]
        closing_pr, proof = _closure_proof(issue, pulls, repo)
        working = issue["assignees"] or [
            p for p in issue["linked_pull_requests"]
            if p["repository"] == repo and pulls.get(p["number"], {}).get("state") == "open"]
        if issue["state"] == "closed":
            if closing_pr is None:
                states[n], reasons[n] = "CLOSED_UNPROVEN", [proof]
            elif blocking:
                states[n], reasons[n] = "LOCKED", [proof + ", but dependencies are not PROVEN"] + blocked
            else:
                states[n], reasons[n] = "PROVEN", [proof]
        elif blocking:
            note = ["claim ignored until dependencies are PROVEN"] if working else []
            states[n], reasons[n] = "LOCKED", blocked + note
        elif working:
            states[n], reasons[n] = "CLAIMED", ["assigned or has an open linked pull request"]
        else:
            states[n], reasons[n] = "DISCOVERED", ["no unproven dependencies; unclaimed"]
        return states[n]

    for n in sorted(issues):
        resolve(n)

    dependents = {n: sorted(m for m in issues if n in graph[m]) for n in issues}
    contributors = {}

    def credit(login, role):
        if login:
            contributors.setdefault(login, set()).add(role)

    crossings = []
    for n in sorted(issues):
        issue = issues[n]
        for login in issue["assignees"]:
            credit(login, "assignee")
        evidence = []
        for ref in issue["linked_pull_requests"]:
            pr = pulls.get(ref["number"]) if ref["repository"] == repo else None
            if pr:
                credit(pr["author"], "author")
                for review in pr["reviews"]:
                    credit(review["author"], "reviewer")
            evidence.append({
                "repository": ref["repository"], "number": ref["number"],
                "url": pr["url"] if pr else None, "title": pr["title"] if pr else None,
                "state": pr["state"] if pr else "unknown", "merged": pr["merged"] if pr else None,
                "author": pr["author"] if pr else None,
                "head_oid": pr["head_oid"] if pr else None,
                "merge_commit_oid": pr["merge_commit_oid"] if pr else None,
                "ci": ci_evidence(pr), "review": review_evidence(pr),
            })
        closer = (issue["closed_event"] or {}).get("closer")
        gaps = []
        if not issue["assignees_complete"]:
            gaps.append("assignee list truncated by the API")
        if not issue["linked_pull_requests_complete"]:
            gaps.append("linked pull request list incomplete")
        gaps.extend("pull request #%d could not be read" % e["number"] for e in evidence
                    if e["state"] == "unknown" and e["repository"] == repo)
        crossings.append({
            "number": n, "title": issue["title"], "url": issue["url"],
            "state": states[n], "reasons": reasons[n],
            "github_state": issue["state"], "github_state_reason": issue["state_reason"],
            "requires": issue["requires"]["refs"], "unresolved_requires": issue["requires"]["unresolved"],
            "dependents": dependents[n], "assignees": issue["assignees"],
            "closed_by": closer if states[n] in ("PROVEN", "LOCKED") and closer and closer.get("type") == "pull_request" else None,
            "pull_requests": evidence,
            "evidence_complete": not gaps, "evidence_gaps": gaps,
        })

    counts = {}
    for c in crossings:
        counts[c["state"]] = counts.get(c["state"], 0) + 1
    stamps = [i["updated_at"] for i in issues.values()] + [p["updated_at"] for p in pulls.values()]
    return {
        "schema": MAP_SCHEMA,
        "mode": "real",
        "repository": snapshot["repository"],
        "source": dict(snapshot["source"], snapshot_sha256=sha256_hex(canonical_bytes(snapshot))),
        "as_of": max(stamps) if stamps else None,
        "counts": counts,
        "crossings": crossings,
        "contributors": [{"login": k, "roles": sorted(v)} for k, v in sorted(contributors.items())],
    }


def check_publishable(snapshot, allow_private=False):
    """Publishing exposes repository data; anything not public is refused unless explicitly allowed."""
    repo = snapshot["repository"]
    if (repo["is_private"] or repo["visibility"] != "public") and not allow_private:
        raise PermissionError("refusing to publish %s repository %s; pass allow_private explicitly"
                              % (repo["visibility"], repo["name_with_owner"]))
