//! Hand-written, fail-closed validators for the projection contracts.
//!
//! Every error is `ErrorKind::Value` and starts with the path of the failing field, for
//! example `projection.quests[2].title: longer than 80 characters`. Unknown fields are
//! always errors: the validator is the last line of defence against private data leaking
//! through a field nobody reviewed.

use super::{
    ALLOWLIST_SCHEMA, CONTRIBUTORS_SCHEMA, MAX_DISPLAY_NAME, MAX_QUESTS, MAX_SUMMARY, MAX_TAGS,
    MAX_TITLE, PROJECTION_SCHEMA, STATES, TAGS,
};
use crate::canonical::canonical_bytes;
use crate::Error;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

fn fail(path: &str, message: impl std::fmt::Display) -> Error {
    Error::value(format!("{path}: {message}"))
}

/// Checks that `value` is an object with every `required` key and no key outside
/// `required` and `optional`.
fn object<'a>(
    value: &'a Value,
    path: &str,
    required: &[&str],
    optional: &[&str],
) -> Result<&'a Map<String, Value>, Error> {
    let map = value
        .as_object()
        .ok_or_else(|| fail(path, "not an object"))?;
    if let Some(key) = map
        .keys()
        .find(|k| !required.contains(&k.as_str()) && !optional.contains(&k.as_str()))
    {
        return Err(fail(&format!("{path}.{key}"), "unknown field"));
    }
    if let Some(key) = required.iter().find(|k| !map.contains_key(**k)) {
        return Err(fail(&format!("{path}.{key}"), "missing"));
    }
    Ok(map)
}

fn string<'a>(value: &'a Value, path: &str) -> Result<&'a str, Error> {
    value.as_str().ok_or_else(|| fail(path, "not a string"))
}

fn boolean(value: &Value, path: &str) -> Result<bool, Error> {
    value.as_bool().ok_or_else(|| fail(path, "not a boolean"))
}

fn integer(value: &Value, path: &str, minimum: u64) -> Result<u64, Error> {
    match value.as_u64() {
        Some(n) if n >= minimum => Ok(n),
        _ => Err(fail(path, format!("not an integer >= {minimum}"))),
    }
}

fn array<'a>(value: &'a Value, path: &str, max: usize) -> Result<&'a Vec<Value>, Error> {
    let items = value.as_array().ok_or_else(|| fail(path, "not a list"))?;
    if items.len() > max {
        return Err(fail(path, format!("more than {max} items")));
    }
    Ok(items)
}

fn schema(map: &Map<String, Value>, path: &str, expected: &str) -> Result<(), Error> {
    if map["schema"] != expected {
        return Err(fail(
            &format!("{path}.schema"),
            format!("expected {expected}"),
        ));
    }
    Ok(())
}

/// Bidirectional overrides and isolates can make reviewed text display differently from
/// its bytes, so they are refused along with control characters.
fn is_forbidden_char(c: char) -> bool {
    c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

/// Public text: at most `max` characters (code points, as JSON Schema counts them), no
/// control or bidirectional-override characters, and non-blank when `required`.
fn text<'a>(value: &'a Value, path: &str, max: usize, required: bool) -> Result<&'a str, Error> {
    let s = string(value, path)?;
    if s.chars().count() > max {
        return Err(fail(path, format!("longer than {max} characters")));
    }
    if s.chars().any(is_forbidden_char) {
        return Err(fail(
            path,
            "contains a control or bidirectional-override character",
        ));
    }
    if required && s.trim().is_empty() {
        return Err(fail(path, "empty"));
    }
    Ok(s)
}

/// `prefix` followed by exactly 16 lowercase hexadecimal digits.
fn random_id<'a>(value: &'a Value, path: &str, prefix: &str) -> Result<&'a str, Error> {
    let s = string(value, path)?;
    match s.strip_prefix(prefix) {
        Some(hex) if hex.len() == 16 && hex.bytes().all(is_lower_hex) => Ok(s),
        _ => Err(fail(
            path,
            format!("not {prefix} followed by 16 lowercase hex digits"),
        )),
    }
}

fn is_lower_hex(b: u8) -> bool {
    b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
}

/// 1 to 39 ASCII letters, digits or hyphens. Handles are opt-in display names, so this is
/// the full rule for them.
fn handle<'a>(value: &'a Value, path: &str) -> Result<&'a str, Error> {
    let s = string(value, path)?;
    if s.is_empty() || s.len() > 39 || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err(fail(path, "not 1 to 39 letters, digits or hyphens"));
    }
    Ok(s)
}

/// A GitHub login: a handle that neither starts nor ends with a hyphen and has no `--`.
fn login<'a>(value: &'a Value, path: &str) -> Result<&'a str, Error> {
    let s = handle(value, path).map_err(|_| fail(path, "not a GitHub login"))?;
    if s.starts_with('-') || s.ends_with('-') || s.contains("--") {
        return Err(fail(path, "not a GitHub login"));
    }
    Ok(s)
}

fn digits(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let (Some(year), Some(month), Some(day)) = (digits(&s[..4]), digits(&s[5..7]), digits(&s[8..]))
    else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

/// `YYYY-MM-DD`. Fixed width, so string order is chronological order.
fn date<'a>(value: &'a Value, path: &str) -> Result<&'a str, Error> {
    let s = string(value, path)?;
    if !is_date(s) {
        return Err(fail(path, "not a YYYY-MM-DD date"));
    }
    Ok(s)
}

/// `YYYY-MM-DDTHH:MM:SSZ` in UTC. Fixed width, so string order is chronological order.
fn timestamp<'a>(value: &'a Value, path: &str) -> Result<&'a str, Error> {
    let s = string(value, path)?;
    let b = s.as_bytes();
    let valid = b.len() == 20
        && is_date(&s[..10])
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':'
        && b[19] == b'Z'
        && digits(&s[11..13]).is_some_and(|h| h < 24)
        && digits(&s[14..16]).is_some_and(|m| m < 60)
        && digits(&s[17..19]).is_some_and(|s| s < 60);
    if !valid {
        return Err(fail(path, "not a YYYY-MM-DDTHH:MM:SSZ timestamp"));
    }
    Ok(s)
}

fn region(value: &Value, path: &str) -> Result<(), Error> {
    let map = object(value, path, &["region_id", "display_name"], &[])?;
    random_id(&map["region_id"], &format!("{path}.region_id"), "r-")?;
    text(
        &map["display_name"],
        &format!("{path}.display_name"),
        MAX_DISPLAY_NAME,
        true,
    )?;
    Ok(())
}

/// The fields shared by allowlist and projection quests.
fn public_text(map: &Map<String, Value>, path: &str) -> Result<(), Error> {
    text(&map["title"], &format!("{path}.title"), MAX_TITLE, true)?;
    text(
        &map["summary"],
        &format!("{path}.summary"),
        MAX_SUMMARY,
        false,
    )?;
    if let Some(tags) = map.get("tags") {
        let path = format!("{path}.tags");
        let mut seen = BTreeSet::new();
        for (i, tag) in array(tags, &path, MAX_TAGS)?.iter().enumerate() {
            let tag_path = format!("{path}[{i}]");
            let tag = string(tag, &tag_path)?;
            if !TAGS.contains(&tag) {
                return Err(fail(&tag_path, "not a known tag"));
            }
            if !seen.insert(tag) {
                return Err(fail(&tag_path, "duplicate tag"));
            }
        }
    }
    boolean(&map["join_open"], &format!("{path}.join_open"))?;
    Ok(())
}

fn unique<'a>(seen: &mut BTreeSet<&'a str>, id: &'a str, path: &str) -> Result<(), Error> {
    if !seen.insert(id) {
        return Err(fail(path, format!("duplicate {id}")));
    }
    Ok(())
}

/// Validates projection bytes and returns the parsed value. The bytes must equal
/// `canonical_bytes` of their parsed value, so that the signature (signing quest) covers
/// exactly one serialisation.
pub fn validate_projection(bytes: &[u8]) -> Result<Value, Error> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|e| fail("projection", format!("not valid JSON: {e}")))?;
    let canonical = canonical_bytes(&value).map_err(|e| fail("projection", e))?;
    if canonical != bytes {
        return Err(fail(
            "projection",
            "bytes are not canonical (keys must be sorted, two-space indented, newline-terminated)",
        ));
    }
    check_projection(&value)?;
    Ok(value)
}

fn check_projection(value: &Value) -> Result<(), Error> {
    let path = "projection";
    let map = object(
        value,
        path,
        &[
            "schema",
            "region",
            "key_id",
            "sequence",
            "generated_at",
            "source_as_of",
            "expires_at",
            "quests",
        ],
        &[],
    )?;
    schema(map, path, PROJECTION_SCHEMA)?;
    region(&map["region"], "projection.region")?;
    random_id(&map["key_id"], "projection.key_id", "k-")?;
    integer(&map["sequence"], "projection.sequence", 1)?;
    let generated_at = timestamp(&map["generated_at"], "projection.generated_at")?;
    let source_as_of = timestamp(&map["source_as_of"], "projection.source_as_of")?;
    if !source_as_of.ends_with(":00:00Z") {
        return Err(fail(
            "projection.source_as_of",
            "not rounded down to the hour",
        ));
    }
    if source_as_of > generated_at {
        return Err(fail("projection.source_as_of", "later than generated_at"));
    }
    let expires_at = timestamp(&map["expires_at"], "projection.expires_at")?;
    if expires_at <= generated_at {
        return Err(fail("projection.expires_at", "not later than generated_at"));
    }

    let quests = array(&map["quests"], "projection.quests", MAX_QUESTS)?;
    let mut ids = BTreeSet::new();
    let mut graph = BTreeMap::new();
    for (i, quest) in quests.iter().enumerate() {
        let path = format!("projection.quests[{i}]");
        let (id, requires) = projection_quest(quest, &path, &generated_at[..10])?;
        unique(&mut ids, id, &format!("{path}.quest_id"))?;
        graph.insert(id, (i, requires));
    }
    check_requires(&graph)
}

/// Returns the quest id and its `requires` ids.
fn projection_quest<'a>(
    value: &'a Value,
    path: &str,
    generated_on: &str,
) -> Result<(&'a str, Vec<&'a str>), Error> {
    let map = object(
        value,
        path,
        &[
            "quest_id",
            "title",
            "summary",
            "state",
            "state_since",
            "requires",
            "hidden_prerequisites",
            "contributors",
            "join_open",
        ],
        &["tags", "provenance"],
    )?;
    let id = random_id(&map["quest_id"], &format!("{path}.quest_id"), "q-")?;
    public_text(map, path)?;

    let state_path = format!("{path}.state");
    let state = string(&map["state"], &state_path)?;
    if !STATES.contains(&state) {
        return Err(fail(
            &state_path,
            format!("not one of {}", STATES.join(", ")),
        ));
    }
    let since_path = format!("{path}.state_since");
    if date(&map["state_since"], &since_path)? > generated_on {
        return Err(fail(&since_path, "later than generated_at"));
    }

    let requires_path = format!("{path}.requires");
    let mut requires = Vec::new();
    for (j, dep) in array(&map["requires"], &requires_path, MAX_QUESTS)?
        .iter()
        .enumerate()
    {
        let dep_path = format!("{requires_path}[{j}]");
        let dep = random_id(dep, &dep_path, "q-")?;
        if dep == id {
            return Err(fail(&dep_path, "quest requires itself"));
        }
        if requires.contains(&dep) {
            return Err(fail(&dep_path, format!("duplicate {dep}")));
        }
        requires.push(dep);
    }
    boolean(
        &map["hidden_prerequisites"],
        &format!("{path}.hidden_prerequisites"),
    )?;

    let contributors_path = format!("{path}.contributors");
    let contributors = object(
        &map["contributors"],
        &contributors_path,
        &["count", "handles"],
        &[],
    )?;
    let count = integer(
        &contributors["count"],
        &format!("{contributors_path}.count"),
        0,
    )?;
    let handles_path = format!("{contributors_path}.handles");
    let handles = array(&contributors["handles"], &handles_path, usize::MAX)?;
    if handles.len() as u64 > count {
        return Err(fail(&handles_path, "more handles than count"));
    }
    let mut seen = BTreeSet::new();
    for (j, h) in handles.iter().enumerate() {
        let h_path = format!("{handles_path}[{j}]");
        let h = handle(h, &h_path)?;
        if !seen.insert(h.to_ascii_lowercase()) {
            return Err(fail(&h_path, format!("duplicate {h}")));
        }
    }

    let provenance_path = format!("{path}.provenance");
    match (state == "PROVEN", map.get("provenance")) {
        (true, Some(provenance)) => {
            let provenance = object(provenance, &provenance_path, &["commitment"], &[])?;
            let commitment_path = format!("{provenance_path}.commitment");
            let commitment = string(&provenance["commitment"], &commitment_path)?;
            if commitment.len() != 64 || !commitment.bytes().all(is_lower_hex) {
                return Err(fail(&commitment_path, "not 64 lowercase hex digits"));
            }
        }
        (true, None) => return Err(fail(&provenance_path, "missing for a PROVEN quest")),
        (false, Some(_)) => {
            return Err(fail(
                &provenance_path,
                "present on a quest that is not PROVEN",
            ))
        }
        (false, None) => {}
    }
    Ok((id, requires))
}

/// Every `requires` id names a quest in the same file, and the graph is acyclic.
fn check_requires(graph: &BTreeMap<&str, (usize, Vec<&str>)>) -> Result<(), Error> {
    for (index, requires) in graph.values() {
        for (j, dep) in requires.iter().enumerate() {
            if !graph.contains_key(dep) {
                return Err(fail(
                    &format!("projection.quests[{index}].requires[{j}]"),
                    format!("unknown quest {dep}"),
                ));
            }
        }
    }
    // Iterative depth-first search; `on_path` holds the grey vertices.
    let mut done = BTreeSet::new();
    for &start in graph.keys() {
        if done.contains(start) {
            continue;
        }
        let mut on_path = BTreeSet::from([start]);
        let mut stack = vec![(start, 0usize)];
        while let Some((node, next)) = stack.last_mut() {
            let (index, requires) = &graph[*node];
            let Some(&dep) = requires.get(*next) else {
                on_path.remove(*node);
                done.insert(*node);
                stack.pop();
                continue;
            };
            *next += 1;
            if on_path.contains(dep) {
                return Err(fail(
                    &format!("projection.quests[{index}].requires"),
                    format!("dependency cycle through {dep}"),
                ));
            }
            if !done.contains(dep) {
                on_path.insert(dep);
                stack.push((dep, 0));
            }
        }
    }
    Ok(())
}

/// Validates a parsed allowlist. `source_issue` is private: it links an allowlisted quest
/// to its issue in the private repository and has no counterpart in the projection.
pub fn validate_allowlist(value: &Value) -> Result<(), Error> {
    let path = "allowlist";
    let map = object(value, path, &["schema", "region", "quests"], &[])?;
    schema(map, path, ALLOWLIST_SCHEMA)?;
    region(&map["region"], "allowlist.region")?;
    let mut ids = BTreeSet::new();
    let mut issues = BTreeSet::new();
    for (i, quest) in array(&map["quests"], "allowlist.quests", MAX_QUESTS)?
        .iter()
        .enumerate()
    {
        let path = format!("allowlist.quests[{i}]");
        let quest = object(
            quest,
            &path,
            &["quest_id", "source_issue", "title", "summary", "join_open"],
            &["tags"],
        )?;
        let id_path = format!("{path}.quest_id");
        unique(
            &mut ids,
            random_id(&quest["quest_id"], &id_path, "q-")?,
            &id_path,
        )?;
        let issue_path = format!("{path}.source_issue");
        if !issues.insert(integer(&quest["source_issue"], &issue_path, 1)?) {
            return Err(fail(&issue_path, "issue already allowlisted"));
        }
        public_text(quest, &path)?;
    }
    Ok(())
}

/// Validates a parsed contributors file. Logins and handles are each unique, ignoring
/// case as GitHub does.
pub fn validate_contributors(value: &Value) -> Result<(), Error> {
    let path = "contributors";
    let map = object(value, path, &["schema", "contributors"], &[])?;
    schema(map, path, CONTRIBUTORS_SCHEMA)?;
    let mut logins = BTreeSet::new();
    let mut handles = BTreeSet::new();
    for (i, entry) in array(
        &map["contributors"],
        "contributors.contributors",
        usize::MAX,
    )?
    .iter()
    .enumerate()
    {
        let path = format!("contributors.contributors[{i}]");
        let entry = object(entry, &path, &["login", "handle"], &[])?;
        let login_path = format!("{path}.login");
        let login = login(&entry["login"], &login_path)?;
        if !logins.insert(login.to_ascii_lowercase()) {
            return Err(fail(&login_path, "duplicate login"));
        }
        let handle_path = format!("{path}.handle");
        let handle = handle(&entry["handle"], &handle_path)?;
        if !handles.insert(handle.to_ascii_lowercase()) {
            return Err(fail(&handle_path, "duplicate handle"));
        }
    }
    Ok(())
}
