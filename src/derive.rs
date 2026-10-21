//! Pure snapshot -> map derivation. No I/O, clock, randomness or network.
//! Port of `gitmaps/derive.py`; reason strings are part of the byte-identical contract.
//!
//! State precedence, evaluated per crossing:
//!   INVALID          missing dependency, unresolved reference or dependency cycle
//!   LOCKED           any dependency is not PROVEN (overrides CLAIMED and a merged closure)
//!   PROVEN           closed as completed by a merged pull request in the same repository
//!   CLOSED_UNPROVEN  closed without that evidence (manual close, not planned, unmerged closer...)
//!   CLAIMED          open, with an assignee or an open linked pull request
//!   DISCOVERED       open, all dependencies PROVEN, nobody working on it

use crate::canonical::{canonical_bytes, sha256_hex};
use crate::pyrepr::py_repr;
use crate::{Error, MAP_SCHEMA, SNAPSHOT_SCHEMA};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// A non-empty string (Python truthiness for optional logins and reasons).
fn present(value: &Value) -> Option<&str> {
    value.as_str().filter(|s| !s.is_empty())
}

fn number(value: &Value, what: &str) -> Result<u64, Error> {
    value
        .as_u64()
        .ok_or_else(|| Error::value(format!("{what} is not an issue or pull request number")))
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

fn ci_status(rollup: &str) -> &'static str {
    match rollup {
        "success" => "success",
        "failure" | "error" => "failure",
        "pending" | "expected" => "pending",
        _ => "unknown",
    }
}

/// success | failure | pending | absent (no checks reported) | unknown (could not be read).
pub fn ci_evidence(pr: Option<&Value>) -> Value {
    let Some(pr) = pr else {
        return json!({"status": "unknown", "commit": null, "detail": "pull request evidence unavailable"});
    };
    let ci = &pr["ci"];
    if ci["fetch_error"] == true {
        return json!({"status": "unknown", "commit": ci["commit"], "detail": "check status could not be read"});
    }
    if ci["commit"].is_null() {
        return json!({"status": "absent", "commit": null, "detail": "pull request has no commits"});
    }
    let Some(rollup) = ci["rollup"].as_str() else {
        return json!({"status": "absent", "commit": ci["commit"], "detail": "no checks reported for head commit"});
    };
    let checks: Vec<Value> = ci["contexts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| json!({"name": c["name"], "conclusion": c["conclusion"]}))
        .collect();
    json!({
        "status": ci_status(rollup), "commit": ci["commit"], "detail": format!("rollup {rollup}"),
        "checks": checks, "checks_complete": ci["contexts_complete"],
    })
}

/// approved | changes_requested | absent (no decisive review) | unknown.
pub fn review_evidence(pr: Option<&Value>) -> Value {
    let Some(pr) = pr else {
        return json!({"status": "unknown", "approved_by": [], "changes_requested_by": [], "approved_on_head": []});
    };
    let mut latest: BTreeMap<&str, &Value> = BTreeMap::new();
    for review in pr["reviews"].as_array().into_iter().flatten() {
        let decisive = matches!(
            review["state"].as_str(),
            Some("approved" | "changes_requested" | "dismissed")
        );
        if let (Some(author), true) = (present(&review["author"]), decisive) {
            latest.insert(author, review);
        }
    }
    let approved: Vec<&str> = latest
        .iter()
        .filter(|(_, r)| r["state"] == "approved")
        .map(|(a, _)| *a)
        .collect();
    let changes: Vec<&str> = latest
        .iter()
        .filter(|(_, r)| r["state"] == "changes_requested")
        .map(|(a, _)| *a)
        .collect();
    let on_head: Vec<&str> = approved
        .iter()
        .copied()
        .filter(|a| latest[a]["commit"] == pr["head_oid"])
        .collect();
    let status = if !changes.is_empty() {
        "changes_requested"
    } else if !approved.is_empty() {
        "approved"
    } else {
        "absent"
    };
    json!({"status": status, "approved_by": approved, "changes_requested_by": changes, "approved_on_head": on_head})
}

/// Tarjan's algorithm over sorted nodes; components are returned sorted.
fn strongly_connected(graph: &BTreeMap<u64, Vec<u64>>) -> Vec<Vec<u64>> {
    struct Tarjan<'g> {
        graph: &'g BTreeMap<u64, Vec<u64>>,
        index: BTreeMap<u64, usize>,
        low: BTreeMap<u64, usize>,
        stack: Vec<u64>,
        on_stack: BTreeSet<u64>,
        counter: usize,
        out: Vec<Vec<u64>>,
    }
    impl Tarjan<'_> {
        fn visit(&mut self, v: u64) {
            self.index.insert(v, self.counter);
            self.low.insert(v, self.counter);
            self.counter += 1;
            self.stack.push(v);
            self.on_stack.insert(v);
            for &w in &self.graph[&v] {
                if !self.index.contains_key(&w) {
                    self.visit(w);
                    let low = self.low[&v].min(self.low[&w]);
                    self.low.insert(v, low);
                } else if self.on_stack.contains(&w) {
                    let low = self.low[&v].min(self.index[&w]);
                    self.low.insert(v, low);
                }
            }
            if self.low[&v] == self.index[&v] {
                let mut component = Vec::new();
                while let Some(w) = self.stack.pop() {
                    self.on_stack.remove(&w);
                    component.push(w);
                    if w == v {
                        break;
                    }
                }
                component.sort_unstable();
                self.out.push(component);
            }
        }
    }
    let mut t = Tarjan {
        graph,
        index: BTreeMap::new(),
        low: BTreeMap::new(),
        stack: Vec::new(),
        on_stack: BTreeSet::new(),
        counter: 0,
        out: Vec::new(),
    };
    for &v in graph.keys() {
        if !t.index.contains_key(&v) {
            t.visit(v);
        }
    }
    t.out
}

/// (closing PR number, reason). Fails closed on any missing evidence.
fn closure_proof(
    issue: &Value,
    pulls: &BTreeMap<u64, &Value>,
    repo: &str,
) -> (Option<u64>, String) {
    if issue["state"] != "closed" {
        return (None, "open".into());
    }
    if issue["state_reason"] != "completed" {
        return (
            None,
            format!(
                "closed as {}",
                present(&issue["state_reason"]).unwrap_or("unknown reason")
            ),
        );
    }
    let event = &issue["closed_event"];
    if event.is_null() {
        return (None, "close event not available".into());
    }
    let closer = &event["closer"];
    if closer.is_null() {
        return (None, "closed manually, not by a pull request".into());
    }
    let kind = closer["type"].as_str().unwrap_or("");
    if kind != "pull_request" {
        return (
            None,
            format!(
                "closed by {}, not by a pull request",
                kind.replace('_', " ")
            ),
        );
    }
    let n = closer["number"].as_u64().unwrap_or(0);
    if closer["repository"] != repo {
        let other = closer["repository"].as_str().unwrap_or("");
        return (
            None,
            format!("closed by pull request in another repository ({other}#{n})"),
        );
    }
    match pulls.get(&n) {
        None => (None, format!("closing pull request #{n} could not be read")),
        Some(pr) if pr["merged"] != true => {
            (None, format!("closing pull request #{n} is not merged"))
        }
        Some(_) => (Some(n), format!("closed by merged pull request #{n}")),
    }
}

struct Resolver<'s> {
    issues: &'s BTreeMap<u64, &'s Value>,
    pulls: &'s BTreeMap<u64, &'s Value>,
    graph: &'s BTreeMap<u64, Vec<u64>>,
    problems: &'s BTreeMap<u64, Vec<String>>,
    repo: &'s str,
    states: BTreeMap<u64, &'static str>,
    reasons: BTreeMap<u64, Vec<String>>,
}

impl Resolver<'_> {
    fn resolve(&mut self, n: u64) -> &'static str {
        if let Some(state) = self.states.get(&n) {
            return state;
        }
        let issue = self.issues[&n];
        if !self.problems[&n].is_empty() {
            let mut reasons = self.problems[&n].clone();
            reasons.sort();
            return self.set(n, "INVALID", reasons);
        }
        let mut blocking: Vec<u64> = Vec::new();
        for &d in &self.graph[&n] {
            if self.resolve(d) != "PROVEN" {
                blocking.push(d);
            }
        }
        blocking.sort_unstable();
        let blocked: Vec<String> = blocking
            .iter()
            .map(|d| format!("dependency #{d} is {}", self.states[d]))
            .collect();
        let (closing_pr, proof) = closure_proof(issue, self.pulls, self.repo);
        let working = issue["assignees"].as_array().is_some_and(|a| !a.is_empty())
            || issue["linked_pull_requests"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|p| {
                    p["repository"] == self.repo
                        && p["number"]
                            .as_u64()
                            .and_then(|k| self.pulls.get(&k))
                            .is_some_and(|pr| pr["state"] == "open")
                });
        if issue["state"] == "closed" {
            if closing_pr.is_none() {
                self.set(n, "CLOSED_UNPROVEN", vec![proof])
            } else if !blocking.is_empty() {
                let mut reasons = vec![format!("{proof}, but dependencies are not PROVEN")];
                reasons.extend(blocked);
                self.set(n, "LOCKED", reasons)
            } else {
                self.set(n, "PROVEN", vec![proof])
            }
        } else if !blocking.is_empty() {
            let mut reasons = blocked;
            if working {
                reasons.push("claim ignored until dependencies are PROVEN".into());
            }
            self.set(n, "LOCKED", reasons)
        } else if working {
            self.set(
                n,
                "CLAIMED",
                vec!["assigned or has an open linked pull request".into()],
            )
        } else {
            self.set(
                n,
                "DISCOVERED",
                vec!["no unproven dependencies; unclaimed".into()],
            )
        }
    }

    fn set(&mut self, n: u64, state: &'static str, reasons: Vec<String>) -> &'static str {
        self.states.insert(n, state);
        self.reasons.insert(n, reasons);
        state
    }
}

fn check_canonical_order(snapshot: &Value, key: &str) -> Result<(), Error> {
    let items = snapshot[key]
        .as_array()
        .ok_or_else(|| Error::value(format!("snapshot {key} is not a list")))?;
    let numbers = items
        .iter()
        .map(|i| number(&i["number"], key))
        .collect::<Result<Vec<_>, Error>>()?;
    if numbers.windows(2).any(|w| w[0] >= w[1]) {
        return Err(Error::value(format!(
            "snapshot {key} are not in canonical (sorted, unique) order"
        )));
    }
    Ok(())
}

pub fn derive(snapshot: &Value) -> Result<Value, Error> {
    if snapshot["schema"] != SNAPSHOT_SCHEMA {
        return Err(Error::value(format!(
            "unsupported snapshot schema: {}",
            snapshot["schema"]
        )));
    }
    check_canonical_order(snapshot, "issues")?;
    check_canonical_order(snapshot, "pull_requests")?;
    let repo = snapshot["repository"]["name_with_owner"]
        .as_str()
        .ok_or_else(|| Error::value("snapshot repository has no name_with_owner"))?;
    let issues: BTreeMap<u64, &Value> = snapshot["issues"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|i| (i["number"].as_u64().unwrap(), i))
        .collect();
    let pulls: BTreeMap<u64, &Value> = snapshot["pull_requests"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["repository"] == repo)
        .map(|p| (p["number"].as_u64().unwrap(), p))
        .collect();

    let mut problems: BTreeMap<u64, Vec<String>> =
        issues.keys().map(|&n| (n, Vec::new())).collect();
    let mut graph: BTreeMap<u64, Vec<u64>> = issues.keys().map(|&n| (n, Vec::new())).collect();
    for (&n, issue) in &issues {
        for token in strings(&issue["requires"]["unresolved"]) {
            problems
                .get_mut(&n)
                .unwrap()
                .push(format!("unresolved reference {}", py_repr(&token)));
        }
        for r in issue["requires"]["refs"].as_array().into_iter().flatten() {
            let r = number(r, "requires reference")?;
            if issues.contains_key(&r) {
                graph.get_mut(&n).unwrap().push(r);
            } else {
                problems.get_mut(&n).unwrap().push(format!(
                    "missing dependency #{r} (not a crossing in this snapshot)"
                ));
            }
        }
    }
    for component in strongly_connected(&graph) {
        if component.len() > 1 || graph[&component[0]].contains(&component[0]) {
            let cycle = component
                .iter()
                .map(|c| format!("#{c}"))
                .collect::<Vec<_>>()
                .join(" -> ");
            for n in &component {
                problems
                    .get_mut(n)
                    .unwrap()
                    .push(format!("dependency cycle: {cycle}"));
            }
        }
    }

    let mut resolver = Resolver {
        issues: &issues,
        pulls: &pulls,
        graph: &graph,
        problems: &problems,
        repo,
        states: BTreeMap::new(),
        reasons: BTreeMap::new(),
    };
    for &n in issues.keys() {
        resolver.resolve(n);
    }
    let Resolver {
        states, reasons, ..
    } = resolver;

    let mut contributors: BTreeMap<String, BTreeSet<&'static str>> = BTreeMap::new();
    let mut credit = |login: &Value, role: &'static str| {
        if let Some(login) = present(login) {
            contributors
                .entry(login.to_string())
                .or_default()
                .insert(role);
        }
    };

    let mut crossings = Vec::new();
    for (&n, issue) in &issues {
        for login in issue["assignees"].as_array().into_iter().flatten() {
            credit(login, "assignee");
        }
        let mut evidence = Vec::new();
        for r in issue["linked_pull_requests"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let pr = if r["repository"] == repo {
                r["number"].as_u64().and_then(|k| pulls.get(&k)).copied()
            } else {
                None
            };
            if let Some(pr) = pr {
                credit(&pr["author"], "author");
                for review in pr["reviews"].as_array().into_iter().flatten() {
                    credit(&review["author"], "reviewer");
                }
            }
            let field = |key: &str| pr.map_or(Value::Null, |p| p[key].clone());
            evidence.push(json!({
                "repository": r["repository"], "number": r["number"],
                "url": field("url"), "title": field("title"),
                "state": pr.map_or(Value::from("unknown"), |p| p["state"].clone()),
                "merged": field("merged"), "author": field("author"),
                "head_oid": field("head_oid"), "merge_commit_oid": field("merge_commit_oid"),
                "ci": ci_evidence(pr), "review": review_evidence(pr),
            }));
        }
        let closer = &issue["closed_event"]["closer"];
        let mut gaps = Vec::new();
        if issue["assignees_complete"] != true {
            gaps.push("assignee list truncated by the API".to_string());
        }
        if issue["linked_pull_requests_complete"] != true {
            gaps.push("linked pull request list incomplete".to_string());
        }
        for e in &evidence {
            if e["state"] == "unknown" && e["repository"] == repo {
                gaps.push(format!("pull request #{} could not be read", e["number"]));
            }
        }
        let state = states[&n];
        let closed_by = if matches!(state, "PROVEN" | "LOCKED")
            && closer.as_object().is_some_and(|c| !c.is_empty())
            && closer["type"] == "pull_request"
        {
            closer.clone()
        } else {
            Value::Null
        };
        let dependents: Vec<u64> = issues
            .keys()
            .copied()
            .filter(|m| graph[m].contains(&n))
            .collect();
        crossings.push(json!({
            "number": n, "title": issue["title"], "url": issue["url"],
            "state": state, "reasons": reasons[&n],
            "github_state": issue["state"], "github_state_reason": issue["state_reason"],
            "requires": issue["requires"]["refs"], "unresolved_requires": issue["requires"]["unresolved"],
            "dependents": dependents, "assignees": issue["assignees"],
            "closed_by": closed_by,
            "pull_requests": evidence,
            "evidence_complete": gaps.is_empty(), "evidence_gaps": gaps,
        }));
    }

    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    for state in states.values() {
        *counts.entry(state).or_default() += 1;
    }
    let as_of = issues
        .values()
        .chain(pulls.values())
        .filter_map(|v| v["updated_at"].as_str())
        .max()
        .map_or(Value::Null, Value::from);
    let mut source = snapshot["source"]
        .as_object()
        .cloned()
        .unwrap_or_else(Map::new);
    source.insert(
        "snapshot_sha256".into(),
        Value::String(sha256_hex(&canonical_bytes(snapshot)?)),
    );
    Ok(json!({
        "schema": MAP_SCHEMA,
        "mode": "real",
        "repository": snapshot["repository"],
        "source": source,
        "as_of": as_of,
        "counts": counts,
        "crossings": crossings,
        "contributors": contributors
            .into_iter()
            .map(|(login, roles)| json!({"login": login, "roles": roles.into_iter().collect::<Vec<_>>()}))
            .collect::<Vec<_>>(),
    }))
}

/// Publishing exposes repository data; anything not public is refused unless explicitly allowed.
pub fn check_publishable(snapshot: &Value, allow_private: bool) -> Result<(), Error> {
    let repo = &snapshot["repository"];
    let private = repo["is_private"] == true || repo["visibility"] != "public";
    if private && !allow_private {
        return Err(Error::permission(format!(
            "refusing to publish {} repository {}; pass allow_private explicitly",
            repo["visibility"].as_str().unwrap_or("unknown"),
            repo["name_with_owner"].as_str().unwrap_or("unknown")
        )));
    }
    Ok(())
}
