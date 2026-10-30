//! Projection, allowlist and contributor contracts (`src/projection/`). Offline; every
//! fixture under `tests/fixtures/projection/` is synthetic.

use gitmaps::canonical::canonical_bytes;
use gitmaps::projection::{
    validate_allowlist, validate_contributors, validate_projection, ALLOWLIST_SCHEMA,
    CONTRIBUTORS_SCHEMA, PROJECTION_SCHEMA, STATES, TAGS,
};
use gitmaps::{Error, ErrorKind};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_dir(path: &str) -> PathBuf {
    root().join("tests/fixtures/projection").join(path)
}

fn read(path: &str) -> Vec<u8> {
    std::fs::read(fixture_dir(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn parse(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}

fn valid_projection() -> Value {
    parse(&read("valid/projection.json"))
}

fn projection_from(value: &Value) -> Result<Value, Error> {
    validate_projection(&canonical_bytes(value).unwrap())
}

fn assert_rejected(result: Result<impl std::fmt::Debug, Error>, field: &str, what: &str) {
    let error = result.expect_err(what);
    assert_eq!(error.kind, ErrorKind::Value, "{what}: {error}");
    assert!(
        error.message.starts_with(&format!("{field}: ")),
        "{what}: expected an error naming {field}, got {:?}",
        error.message
    );
}

type Validator = fn(&[u8]) -> Result<(), Error>;

fn projection(bytes: &[u8]) -> Result<(), Error> {
    validate_projection(bytes).map(|_| ())
}

fn allowlist(bytes: &[u8]) -> Result<(), Error> {
    validate_allowlist(&parse(bytes))
}

fn contributors(bytes: &[u8]) -> Result<(), Error> {
    validate_contributors(&parse(bytes))
}

const Q: &str = "projection.quests";

/// Every negative fixture, the validator it targets, and the field its error must name.
const NEGATIVE: &[(&str, &str)] = &[
    // Bytes.
    ("projection/non-canonical-key-order", "projection"),
    ("projection/non-canonical-whitespace", "projection"),
    ("projection/non-canonical-no-newline", "projection"),
    ("projection/non-canonical-unescaped", "projection"),
    ("projection/non-canonical-crlf", "projection"),
    ("projection/duplicate-key", "projection"),
    ("projection/float-sequence", "projection"),
    ("projection/not-json", "projection"),
    // Top level.
    ("projection/unknown-field", "projection.repository"),
    ("projection/missing-key-id", "projection.key_id"),
    ("projection/wrong-schema", "projection.schema"),
    ("projection/bad-region-id", "projection.region.region_id"),
    (
        "projection/display-name-too-long",
        "projection.region.display_name",
    ),
    ("projection/bad-key-id", "projection.key_id"),
    ("projection/sequence-zero", "projection.sequence"),
    ("projection/sequence-negative", "projection.sequence"),
    ("projection/generated-not-a-date", "projection.generated_at"),
    (
        "projection/generated-with-offset",
        "projection.generated_at",
    ),
    ("projection/source-not-rounded", "projection.source_as_of"),
    (
        "projection/source-after-generated",
        "projection.source_as_of",
    ),
    (
        "projection/expires-not-after-generated",
        "projection.expires_at",
    ),
    ("projection/quests-not-a-list", "projection.quests"),
    ("projection/too-many-quests", "projection.quests"),
    // Quests.
    (
        "projection/unknown-quest-field",
        "projection.quests[0].source_issue",
    ),
    (
        "projection/unknown-contributors-field",
        "projection.quests[1].contributors.logins",
    ),
    (
        "projection/unknown-provenance-field",
        "projection.quests[1].provenance.merge_commit",
    ),
    (
        "projection/missing-requires",
        "projection.quests[0].requires",
    ),
    (
        "projection/join-open-missing",
        "projection.quests[0].join_open",
    ),
    ("projection/bad-quest-id", "projection.quests[0].quest_id"),
    (
        "projection/duplicate-quest-id",
        "projection.quests[4].quest_id",
    ),
    ("projection/title-too-long", "projection.quests[0].title"),
    ("projection/title-blank", "projection.quests[0].title"),
    (
        "projection/title-control-character",
        "projection.quests[0].title",
    ),
    (
        "projection/summary-too-long",
        "projection.quests[0].summary",
    ),
    (
        "projection/summary-bidi-override",
        "projection.quests[0].summary",
    ),
    ("projection/unknown-tag", "projection.quests[0].tags[0]"),
    ("projection/duplicate-tag", "projection.quests[0].tags[1]"),
    ("projection/too-many-tags", "projection.quests[0].tags"),
    ("projection/state-invalid", "projection.quests[0].state"),
    (
        "projection/state-since-after-generated",
        "projection.quests[0].state_since",
    ),
    (
        "projection/state-since-not-a-date",
        "projection.quests[0].state_since",
    ),
    (
        "projection/requires-self",
        "projection.quests[0].requires[0]",
    ),
    (
        "projection/requires-duplicate",
        "projection.quests[2].requires[1]",
    ),
    (
        "projection/requires-dangling",
        "projection.quests[2].requires[1]",
    ),
    ("projection/requires-cycle", "projection.quests[2].requires"),
    (
        "projection/hidden-prerequisites-not-boolean",
        "projection.quests[0].hidden_prerequisites",
    ),
    (
        "projection/negative-count",
        "projection.quests[0].contributors.count",
    ),
    (
        "projection/handles-exceed-count",
        "projection.quests[1].contributors.handles",
    ),
    (
        "projection/bad-handle",
        "projection.quests[1].contributors.handles[0]",
    ),
    (
        "projection/duplicate-handle",
        "projection.quests[1].contributors.handles[1]",
    ),
    (
        "projection/proven-without-provenance",
        "projection.quests[1].provenance",
    ),
    (
        "projection/provenance-not-proven",
        "projection.quests[0].provenance",
    ),
    (
        "projection/bad-commitment",
        "projection.quests[1].provenance.commitment",
    ),
    // Allowlist.
    ("allowlist/unknown-field", "allowlist.owner"),
    ("allowlist/manual-state", "allowlist.quests[0].state"),
    (
        "allowlist/missing-source-issue",
        "allowlist.quests[0].source_issue",
    ),
    (
        "allowlist/source-issue-zero",
        "allowlist.quests[0].source_issue",
    ),
    (
        "allowlist/source-issue-string",
        "allowlist.quests[0].source_issue",
    ),
    (
        "allowlist/duplicate-source-issue",
        "allowlist.quests[1].source_issue",
    ),
    (
        "allowlist/duplicate-quest-id",
        "allowlist.quests[2].quest_id",
    ),
    ("allowlist/title-too-long", "allowlist.quests[0].title"),
    ("allowlist/unknown-tag", "allowlist.quests[0].tags[0]"),
    ("allowlist/wrong-schema", "allowlist.schema"),
    (
        "allowlist/missing-join-open",
        "allowlist.quests[0].join_open",
    ),
    ("allowlist/bad-region-id", "allowlist.region.region_id"),
    // Contributors.
    (
        "contributors/unknown-field",
        "contributors.contributors[0].email",
    ),
    (
        "contributors/bad-login",
        "contributors.contributors[0].login",
    ),
    (
        "contributors/login-double-hyphen",
        "contributors.contributors[0].login",
    ),
    (
        "contributors/duplicate-login",
        "contributors.contributors[1].login",
    ),
    (
        "contributors/duplicate-handle",
        "contributors.contributors[1].handle",
    ),
    (
        "contributors/bad-handle",
        "contributors.contributors[0].handle",
    ),
    (
        "contributors/missing-handle",
        "contributors.contributors[0].handle",
    ),
    ("contributors/wrong-schema", "contributors.schema"),
];

fn validator(name: &str) -> Validator {
    match name.split('/').next().unwrap() {
        "projection" => projection,
        "allowlist" => allowlist,
        "contributors" => contributors,
        other => panic!("no validator for {other}"),
    }
}

#[test]
fn valid_fixtures_pass() {
    let bytes = read("valid/projection.json");
    let value = validate_projection(&bytes).unwrap();
    assert_eq!(canonical_bytes(&value).unwrap(), bytes);
    let states: BTreeSet<&str> = value["quests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q["state"].as_str().unwrap())
        .collect();
    assert_eq!(
        states,
        STATES.iter().copied().collect(),
        "fixture covers every state"
    );
    validate_allowlist(&parse(&read("valid/allowlist.json"))).unwrap();
    validate_contributors(&parse(&read("valid/contributors.json"))).unwrap();
}

#[test]
fn each_negative_fixture_fails_naming_its_field() {
    for (name, field) in NEGATIVE {
        let bytes = read(&format!("invalid/{name}.json"));
        assert_rejected(validator(name)(&bytes), field, name);
    }
}

#[test]
fn every_negative_fixture_is_listed() {
    let listed: BTreeSet<String> = NEGATIVE.iter().map(|(n, _)| n.to_string()).collect();
    let mut on_disk = BTreeSet::new();
    for kind in ["projection", "allowlist", "contributors"] {
        for entry in std::fs::read_dir(fixture_dir(&format!("invalid/{kind}"))).unwrap() {
            let name = entry.unwrap().file_name().into_string().unwrap();
            on_disk.insert(format!("{kind}/{}", name.strip_suffix(".json").unwrap()));
        }
    }
    assert_eq!(on_disk, listed);
}

#[test]
fn structural_fixtures_are_canonical_so_they_reach_the_rule_under_test() {
    for (name, _) in NEGATIVE {
        if !name.starts_with("projection/") {
            continue;
        }
        let bytes = read(&format!("invalid/{name}.json"));
        let canonical = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|v| canonical_bytes(&v).ok());
        let is_byte_case = matches!(
            *name,
            "projection/duplicate-key" | "projection/float-sequence" | "projection/not-json"
        ) || name.starts_with("projection/non-canonical-");
        assert_eq!(
            canonical.as_deref() == Some(&bytes[..]),
            !is_byte_case,
            "{name}"
        );
    }
}

#[test]
fn non_canonical_bytes_are_rejected_even_when_content_is_valid() {
    let value = valid_projection();
    for bytes in [
        serde_json::to_vec(&value).unwrap(),
        serde_json::to_vec_pretty(&value).unwrap(),
        [canonical_bytes(&value).unwrap(), b"\n".to_vec()].concat(),
        [b" ".to_vec(), canonical_bytes(&value).unwrap()].concat(),
    ] {
        assert_eq!(parse(&bytes), value);
        assert_rejected(validate_projection(&bytes), "projection", "non-canonical");
    }
}

#[test]
fn private_source_issue_cannot_be_carried_by_a_projection() {
    let mut value = valid_projection();
    for (path, field) in [
        ("/source_issue", "projection.source_issue"),
        (
            "/quests/0/source_issue",
            "projection.quests[0].source_issue",
        ),
        ("/quests/0/issue", "projection.quests[0].issue"),
        ("/quests/0/url", "projection.quests[0].url"),
        (
            "/quests/0/contributors/logins",
            "projection.quests[0].contributors.logins",
        ),
        ("/region/repository", "projection.region.repository"),
    ] {
        let mut leaked = value.clone();
        let (parent, key) = path.rsplit_once('/').unwrap();
        leaked
            .pointer_mut(if parent.is_empty() { "" } else { parent })
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(key.to_string(), json!(101));
        assert_rejected(projection_from(&leaked), field, path);
    }
    // An allowlist quest pasted as-is into a projection is refused.
    let allowlisted = parse(&read("valid/allowlist.json"))["quests"][0].clone();
    value["quests"][0] = allowlisted;
    assert!(projection_from(&value).is_err());
    let schema =
        std::fs::read_to_string(root().join("docs/schemas/projection-v1.schema.json")).unwrap();
    assert!(!schema.contains("source_issue"));
}

#[test]
fn only_opted_in_handles_are_named_and_never_more_than_counted() {
    let mut value = valid_projection();
    value["quests"][0]["contributors"] = json!({"count": 3, "handles": []});
    projection_from(&value).unwrap();
    value["quests"][0]["contributors"] = json!({"count": 1, "handles": ["a", "b"]});
    assert_rejected(
        projection_from(&value),
        &format!("{Q}[0].contributors.handles"),
        "two handles, one contributor",
    );
    value["quests"][0]["contributors"] = json!({"count": 1, "handles": ["player@example.invalid"]});
    assert_rejected(
        projection_from(&value),
        &format!("{Q}[0].contributors.handles[0]"),
        "email as handle",
    );
}

#[test]
fn provenance_is_required_exactly_for_proven() {
    let commitment = json!({"commitment": "ab".repeat(32)});
    for state in STATES {
        let mut value = valid_projection();
        let quest = value["quests"][0].as_object_mut().unwrap();
        quest.insert("state".into(), json!(state));
        quest.insert("provenance".into(), commitment.clone());
        let with = projection_from(&value);
        value["quests"][0]
            .as_object_mut()
            .unwrap()
            .remove("provenance");
        let without = projection_from(&value);
        if *state == "PROVEN" {
            with.unwrap();
            assert_rejected(without, &format!("{Q}[0].provenance"), state);
        } else {
            assert_rejected(with, &format!("{Q}[0].provenance"), state);
            without.unwrap();
        }
    }
    for state in ["INVALID", "CLOSED_UNPROVEN", "proven", ""] {
        let mut value = valid_projection();
        value["quests"][0]["state"] = json!(state);
        assert_rejected(projection_from(&value), &format!("{Q}[0].state"), state);
    }
}

#[test]
fn long_dependency_cycles_are_rejected_and_chains_accepted() {
    let quest = |n: usize, requires: Vec<String>| {
        json!({
            "quest_id": format!("q-{n:016x}"), "title": format!("Quest {n}"), "summary": "",
            "state": "LOCKED", "state_since": "2026-10-29", "requires": requires,
            "hidden_prerequisites": false, "contributors": {"count": 0, "handles": []},
            "join_open": false,
        })
    };
    let mut value = valid_projection();
    let chain: Vec<Value> = (1..=200)
        .map(|n| {
            quest(
                n,
                if n > 1 {
                    vec![format!("q-{:016x}", n - 1)]
                } else {
                    vec![]
                },
            )
        })
        .collect();
    value["quests"] = Value::Array(chain.clone());
    projection_from(&value).unwrap();

    let mut cyclic = chain;
    cyclic[0] = quest(1, vec![format!("q-{:016x}", 200)]);
    value["quests"] = Value::Array(cyclic);
    let error = projection_from(&value).unwrap_err();
    assert!(error.message.contains("dependency cycle"), "{error}");
}

#[test]
fn timestamps_are_real_utc_instants() {
    for (field, bad) in [
        ("generated_at", "2026-13-01T00:00:00Z"),
        ("generated_at", "2026-10-30T24:00:00Z"),
        ("generated_at", "2026-10-30T09:60:00Z"),
        ("generated_at", "2026-10-30 09:14:03Z"),
        ("generated_at", "2026-10-30T09:14:03.5Z"),
        ("generated_at", "2025-02-29T09:14:03Z"),
        ("expires_at", "2026-10-30T09:14:02Z"),
        ("expires_at", "２026-11-01T09:14:03Z"),
    ] {
        let mut value = valid_projection();
        value[field] = json!(bad);
        assert_rejected(projection_from(&value), &format!("projection.{field}"), bad);
    }
    let mut value = valid_projection();
    value["generated_at"] = json!("2028-02-29T09:14:03Z");
    value["expires_at"] = json!("2028-03-01T00:00:00Z");
    projection_from(&value).unwrap();
}

#[test]
fn lengths_count_characters_not_bytes() {
    let mut value = valid_projection();
    value["quests"][0]["title"] = json!("é".repeat(80));
    value["quests"][0]["summary"] = json!("ü".repeat(400));
    projection_from(&value).unwrap();
}

#[test]
fn documented_schemas_match_the_validators() {
    for (file, schema) in [
        ("projection-v1.schema.json", PROJECTION_SCHEMA),
        ("projection-allowlist-v1.schema.json", ALLOWLIST_SCHEMA),
        (
            "projection-contributors-v1.schema.json",
            CONTRIBUTORS_SCHEMA,
        ),
    ] {
        let doc: Value =
            serde_json::from_slice(&std::fs::read(root().join("docs/schemas").join(file)).unwrap())
                .unwrap();
        assert_eq!(doc["properties"]["schema"]["const"], schema, "{file}");
        assert_eq!(doc["title"], schema, "{file}");
        assert_eq!(doc["additionalProperties"], false, "{file}");
    }
    let doc: Value = serde_json::from_slice(
        &std::fs::read(root().join("docs/schemas/projection-v1.schema.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(doc["$defs"]["tags"]["items"]["enum"], json!(TAGS));
    assert_eq!(
        doc["$defs"]["quest"]["properties"]["state"]["enum"],
        json!(STATES)
    );
}
