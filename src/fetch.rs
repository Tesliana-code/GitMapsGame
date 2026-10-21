//! Build the authoritative, normalised snapshot from GitHub GraphQL responses.
//!
//! Webhook payloads are never read: every run re-queries GitHub. The snapshot has no fetch
//! timestamp, so an unchanged repository yields identical bytes. Port of `gitmaps/fetch.py`.

use crate::github::Transport;
use crate::queries::{
    ISSUES_BY_LABEL_QUERY, ISSUE_BY_NUMBER_QUERY, LINKED_PULL_REQUESTS_QUERY, PULL_REQUEST_QUERY,
    REPO_QUERY,
};
use crate::requires::parse_requires;
use crate::{canonical::sha256_hex, Error, MAX_CROSSINGS, SNAPSHOT_SCHEMA};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

/// Required field: the reference indexes these with `[]` and fails if absent.
fn req<'a>(value: &'a Value, key: &str) -> Result<&'a Value, Error> {
    value
        .get(key)
        .ok_or_else(|| Error::value(format!("GitHub response is missing field {key:?}")))
}

fn req_str<'a>(value: &'a Value, key: &str) -> Result<&'a str, Error> {
    req(value, key)?
        .as_str()
        .ok_or_else(|| Error::value(format!("field {key:?} is not a string")))
}

fn req_array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, Error> {
    req(value, key)?
        .as_array()
        .ok_or_else(|| Error::value(format!("field {key:?} is not a list")))
}

/// Python truthiness for JSON values.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// `value.get(key)`, with a missing key reading as null.
fn opt<'a>(value: &'a Value, key: &str) -> &'a Value {
    value.get(key).unwrap_or(&Value::Null)
}

/// `value.lower() if value else None`
fn lower(value: &Value) -> Value {
    match value.as_str() {
        Some(s) if !s.is_empty() => Value::String(s.to_lowercase()),
        _ => Value::Null,
    }
}

/// `actor["login"] if actor else None`
fn login(actor: &Value) -> Result<Value, Error> {
    if truthy(actor) {
        Ok(req(actor, "login")?.clone())
    } else {
        Ok(Value::Null)
    }
}

fn str_or_empty(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}

fn normalise_issue(node: &Value) -> Result<Value, Error> {
    let state = req_str(node, "state")?;
    let events = req_array(req(node, "timelineItems")?, "nodes")?;
    let closed_event = match events.last() {
        Some(event) if state == "CLOSED" => {
            let closer = opt(event, "closer");
            let closer_out = if closer.is_null() {
                Value::Null
            } else {
                match req_str(closer, "__typename")? {
                    "PullRequest" => json!({
                        "type": "pull_request",
                        "number": req(closer, "number")?,
                        "repository": req(req(closer, "repository")?, "nameWithOwner")?,
                    }),
                    "Commit" => json!({"type": "commit", "oid": req(closer, "oid")?}),
                    other => json!({"type": "other", "typename": other}),
                }
            };
            json!({
                "created_at": req(event, "createdAt")?,
                "actor": login(opt(event, "actor"))?,
                "state_reason": lower(opt(event, "stateReason")),
                "closer": closer_out,
            })
        }
        _ => Value::Null,
    };

    let linked = req(node, "closedByPullRequestsReferences")?;
    let linked_nodes = req_array(linked, "nodes")?;
    let mut linked_out = linked_nodes
        .iter()
        .map(|p| {
            Ok(json!({
                "number": req(p, "number")?,
                "repository": req(req(p, "repository")?, "nameWithOwner")?,
            }))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    linked_out.sort_by(|a, b| {
        (str_or_empty(&a["repository"]), a["number"].as_u64())
            .cmp(&(str_or_empty(&b["repository"]), b["number"].as_u64()))
    });

    let assignees = req(node, "assignees")?;
    let assignee_nodes = req_array(assignees, "nodes")?;
    let mut logins = assignee_nodes
        .iter()
        .map(|a| req_str(a, "login").map(str::to_string))
        .collect::<Result<Vec<_>, Error>>()?;
    logins.sort();

    let body = opt(node, "body")
        .as_str()
        .unwrap_or("")
        .replace("\r\n", "\n");
    let requires = parse_requires(Some(&body));
    Ok(json!({
        "number": req(node, "number")?,
        "title": req(node, "title")?,
        "url": req(node, "url")?,
        "state": state.to_lowercase(),
        "state_reason": lower(opt(node, "stateReason")),
        "created_at": req(node, "createdAt")?,
        "updated_at": req(node, "updatedAt")?,
        "closed_at": opt(node, "closedAt"),
        "body_sha256": sha256_hex(body.as_bytes()),
        "requires": {"refs": requires.refs, "unresolved": requires.unresolved},
        "assignees": logins,
        "assignees_complete": req(assignees, "totalCount")?.as_u64() == Some(assignee_nodes.len() as u64),
        "linked_pull_requests": linked_out,
        "linked_pull_requests_complete": req(linked, "totalCount")?.as_u64() == Some(linked_nodes.len() as u64),
        "closed_event": closed_event,
    }))
}

fn ci_from_commit(commit_node: Option<&Value>, errors: &[Value]) -> Result<Value, Error> {
    let Some(commit_node) = commit_node else {
        return Ok(
            json!({"commit": null, "rollup": null, "fetch_error": false, "contexts": [], "contexts_complete": true}),
        );
    };
    let commit = req(commit_node, "commit")?;
    let rollup = opt(commit, "statusCheckRollup");
    let mut contexts = Vec::new();
    let mut complete = true;
    if truthy(rollup) {
        let conn = req(rollup, "contexts")?;
        let nodes = req_array(conn, "nodes")?;
        complete = req(conn, "totalCount")?.as_u64() == Some(nodes.len() as u64);
        for c in nodes {
            match req_str(c, "__typename")? {
                "CheckRun" => contexts.push(json!({
                    "kind": "check_run", "name": req(c, "name")?,
                    "status": lower(req(c, "status")?), "conclusion": lower(opt(c, "conclusion")),
                })),
                "StatusContext" => contexts.push(json!({
                    "kind": "status", "name": req(c, "context")?, "status": null, "conclusion": lower(req(c, "state")?),
                })),
                _ => {}
            }
        }
        contexts.sort_by(|a, b| {
            (
                str_or_empty(&a["kind"]),
                str_or_empty(&a["name"]),
                str_or_empty(&a["conclusion"]),
            )
                .cmp(&(
                    str_or_empty(&b["kind"]),
                    str_or_empty(&b["name"]),
                    str_or_empty(&b["conclusion"]),
                ))
        });
    }
    let rollup_error = errors.iter().any(|e| {
        opt(e, "path").as_array().is_some_and(|path| {
            path.iter().any(|p| match p {
                Value::String(s) => s == "statusCheckRollup",
                _ => false,
            })
        })
    });
    Ok(json!({
        "commit": req(commit, "oid")?,
        "rollup": if truthy(rollup) { lower(req(rollup, "state")?) } else { Value::Null },
        "fetch_error": rollup_error,
        "contexts": contexts,
        "contexts_complete": complete,
    }))
}

fn review_key(r: &Value) -> (&str, &str, &str, &str) {
    (
        str_or_empty(&r["submitted_at"]),
        str_or_empty(&r["author"]),
        str_or_empty(&r["state"]),
        str_or_empty(&r["commit"]),
    )
}

pub struct Fetcher<'t, T: Transport> {
    transport: &'t mut T,
    owner: String,
    name: String,
}

impl<'t, T: Transport> Fetcher<'t, T> {
    fn query(
        &mut self,
        query_name: &str,
        query: &str,
        extra: Value,
    ) -> Result<(Value, Vec<Value>), Error> {
        let mut variables = Map::new();
        variables.insert("owner".into(), Value::String(self.owner.clone()));
        variables.insert("name".into(), Value::String(self.name.clone()));
        if let Value::Object(extra) = extra {
            variables.extend(extra);
        }
        let response = self
            .transport
            .call(query_name, query, &Value::Object(variables))?;
        let data = opt(&response, "data");
        if data.is_null() {
            let messages: Vec<String> = opt(&response, "errors")
                .as_array()
                .map(|errors| {
                    errors
                        .iter()
                        .map(|e| opt(e, "message").as_str().unwrap_or("?").to_string())
                        .collect()
                })
                .unwrap_or_default();
            let joined = messages.join("; ");
            return Err(Error::github(format!(
                "{query_name} failed: {}",
                if joined.is_empty() {
                    "no data".to_string()
                } else {
                    joined
                }
            )));
        }
        let errors = opt(&response, "errors")
            .as_array()
            .cloned()
            .unwrap_or_default();
        Ok((data.clone(), errors))
    }

    fn repository(&mut self, label: &str) -> Result<Value, Error> {
        let (data, _) = self.query("repository", REPO_QUERY, json!({"label": label}))?;
        let repo = opt(&data, "repository");
        if repo.is_null() {
            return Err(Error::github(format!(
                "repository {}/{} not found or not accessible",
                self.owner, self.name
            )));
        }
        Ok(repo.clone())
    }

    fn complete_linked_pull_requests(&mut self, mut issue: Value) -> Result<Value, Error> {
        let mut conn = req(&issue, "closedByPullRequestsReferences")?.clone();
        while req(req(&conn, "pageInfo")?, "hasNextPage")?.as_bool() == Some(true) {
            let after = req(req(&conn, "pageInfo")?, "endCursor")?.clone();
            let (data, _) = self.query(
                "linked_pull_requests",
                LINKED_PULL_REQUESTS_QUERY,
                json!({"number": req(&issue, "number")?, "after": after}),
            )?;
            let page = req(
                req(req(&data, "repository")?, "issue")?,
                "closedByPullRequestsReferences",
            )?;
            let mut nodes = req_array(&conn, "nodes")?.clone();
            nodes.extend(req_array(page, "nodes")?.iter().cloned());
            conn = json!({"totalCount": req(page, "totalCount")?, "pageInfo": req(page, "pageInfo")?, "nodes": nodes});
        }
        issue["closedByPullRequestsReferences"] = conn;
        Ok(issue)
    }

    fn issues_by_label(&mut self, label: &str) -> Result<Vec<Value>, Error> {
        let mut nodes = Vec::new();
        let mut after = Value::Null;
        loop {
            let (data, _) = self.query(
                "issues_by_label",
                ISSUES_BY_LABEL_QUERY,
                json!({"label": label, "after": after}),
            )?;
            let conn = req(req(&data, "repository")?, "issues")?;
            let total = req(conn, "totalCount")?.as_u64().unwrap_or(0);
            if total > MAX_CROSSINGS {
                return Err(Error::github(format!(
                    "{total} crossings exceed the limit of {MAX_CROSSINGS}"
                )));
            }
            nodes.extend(req_array(conn, "nodes")?.iter().cloned());
            let page = req(conn, "pageInfo")?;
            if req(page, "hasNextPage")?.as_bool() != Some(true) {
                return Ok(nodes);
            }
            after = req(page, "endCursor")?.clone();
        }
    }

    fn issue_by_number(&mut self, number: u64) -> Result<Value, Error> {
        let (data, _) = self.query(
            "issue_by_number",
            ISSUE_BY_NUMBER_QUERY,
            json!({"number": number}),
        )?;
        let issue = opt(req(&data, "repository")?, "issue");
        if issue.is_null() {
            return Err(Error::github(format!(
                "issue #{number} not found in {}/{}",
                self.owner, self.name
            )));
        }
        Ok(issue.clone())
    }

    fn pull_request(&mut self, number: u64) -> Result<Option<Value>, Error> {
        let mut reviews = Vec::new();
        let mut after = Value::Null;
        let mut first: Option<(Value, Vec<Value>)> = None;
        loop {
            let (data, errors) = self.query(
                "pull_request",
                PULL_REQUEST_QUERY,
                json!({"number": number, "after": after}),
            )?;
            let pr = opt(req(&data, "repository")?, "pullRequest");
            if pr.is_null() {
                return Ok(None);
            }
            let page = req(pr, "reviews")?;
            reviews.extend(req_array(page, "nodes")?.iter().cloned());
            let info = req(page, "pageInfo")?;
            let next = req(info, "hasNextPage")?.as_bool() == Some(true);
            let cursor = req(info, "endCursor")?.clone();
            if first.is_none() {
                first = Some((pr.clone(), errors));
            }
            if !next {
                break;
            }
            after = cursor;
        }
        let (pr, errors) = first.expect("first page recorded");
        let mut reviews_out = reviews
            .iter()
            .map(|r| {
                Ok(json!({
                    "state": req_str(r, "state")?.to_lowercase(),
                    "submitted_at": opt(r, "submittedAt"),
                    "author": login(opt(r, "author"))?,
                    "commit": if truthy(opt(r, "commit")) { req(opt(r, "commit"), "oid")?.clone() } else { Value::Null },
                }))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        reviews_out.sort_by(|a, b| review_key(a).cmp(&review_key(b)));
        let commits = req_array(req(&pr, "commits")?, "nodes")?;
        let messages: BTreeSet<String> = errors
            .iter()
            .map(|e| opt(e, "message").as_str().unwrap_or("?").to_string())
            .collect();
        let merge_commit = opt(&pr, "mergeCommit");
        Ok(Some(json!({
            "number": req(&pr, "number")?,
            "title": req(&pr, "title")?,
            "url": req(&pr, "url")?,
            "repository": req(req(&pr, "repository")?, "nameWithOwner")?,
            "state": req_str(&pr, "state")?.to_lowercase(),
            "merged": truthy(req(&pr, "merged")?),
            "merged_at": opt(&pr, "mergedAt"),
            "created_at": req(&pr, "createdAt")?,
            "updated_at": req(&pr, "updatedAt")?,
            "author": login(opt(&pr, "author"))?,
            "head_oid": req(&pr, "headRefOid")?,
            "merge_commit_oid": if truthy(merge_commit) { req(merge_commit, "oid")?.clone() } else { Value::Null },
            "reviews": reviews_out,
            "ci": ci_from_commit(commits.last(), &errors)?,
            "fetch_errors": messages.into_iter().collect::<Vec<_>>(),
        })))
    }
}

/// Query GitHub through `transport` and return the normalised snapshot.
/// `numbers`: `Some(non-empty)` selects issues by number; otherwise issues carrying `label`.
pub fn snapshot<T: Transport>(
    transport: &mut T,
    repo: &str,
    label: &str,
    numbers: Option<&[u64]>,
) -> Result<Value, Error> {
    let (owner, name) = repo.split_once('/').unwrap_or((repo, ""));
    if owner.is_empty() || name.is_empty() {
        return Err(Error::github("repository must be owner/name"));
    }
    let mut fetcher = Fetcher {
        transport,
        owner: owner.to_string(),
        name: name.to_string(),
    };
    let meta = fetcher.repository(label)?;
    let numbers: Option<Vec<u64>> = numbers.filter(|n| !n.is_empty()).map(|n| {
        n.iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    });
    let (selection, raw_issues) = match &numbers {
        Some(numbers) => (
            json!({"mode": "numbers", "numbers": numbers}),
            numbers
                .iter()
                .map(|&n| fetcher.issue_by_number(n))
                .collect::<Result<Vec<_>, Error>>()?,
        ),
        None => (
            json!({"mode": "label", "label": label}),
            if opt(&meta, "label").is_null() {
                Vec::new()
            } else {
                fetcher.issues_by_label(label)?
            },
        ),
    };
    let full_name = req_str(&meta, "nameWithOwner")?.to_string();
    let mut issues = raw_issues
        .into_iter()
        .map(|n| {
            fetcher
                .complete_linked_pull_requests(n)
                .and_then(|n| normalise_issue(&n))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    issues.sort_by(|a, b| a["number"].as_u64().cmp(&b["number"].as_u64()));

    let mut wanted = BTreeSet::new();
    for issue in &issues {
        for p in issue["linked_pull_requests"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if p["repository"] == full_name.as_str() {
                wanted.extend(p["number"].as_u64());
            }
        }
        let closer = &issue["closed_event"]["closer"];
        if closer["type"] == "pull_request" && closer["repository"] == full_name.as_str() {
            wanted.extend(closer["number"].as_u64());
        }
    }
    let mut pulls = Vec::new();
    for number in wanted {
        if let Some(pr) = fetcher.pull_request(number)? {
            pulls.push(pr);
        }
    }

    Ok(json!({
        "schema": SNAPSHOT_SCHEMA,
        "source": {"api": "github-graphql", "selection": selection},
        "repository": {
            "name_with_owner": full_name,
            "is_private": truthy(req(&meta, "isPrivate")?),
            "visibility": req_str(&meta, "visibility")?.to_lowercase(),
            "label_exists": !opt(&meta, "label").is_null(),
        },
        "issues": issues,
        "pull_requests": pulls,
    }))
}
