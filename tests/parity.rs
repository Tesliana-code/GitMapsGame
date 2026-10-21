//! Byte-for-byte parity with the Python reference implementation (`gitmaps/`).
//!
//! `tests/fixtures/recorded/` holds frozen, read-only GitHub API recordings and the reference
//! outputs. `tests/fixtures/parity/` holds constructed inputs whose expected values were
//! produced by the reference (`scripts/generate_parity_vectors.py`); none is GitHub activity.

use gitmaps::canonical::{canonical_bytes, sha256_hex};
use gitmaps::derive::derive;
use gitmaps::fetch::snapshot;
use gitmaps::github::ReplayTransport;
use gitmaps::pyrepr::py_repr;
use gitmaps::requires::parse_requires;
use gitmaps::{ErrorKind, DEFAULT_LABEL};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(path)
}

fn read(path: &str) -> Vec<u8> {
    std::fs::read(fixture(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn json(path: &str) -> Value {
    serde_json::from_slice(&read(path)).unwrap()
}

/// Frozen on 2026-10-21 (preservation commit c31a97e); these files must never change.
const FROZEN: [(&str, &str); 6] = [
    (
        "recorded/gitmapsgame.recording.json",
        "5448fe91130288c346124f4d7d3e18713712b59cb150050f489f5193d3b43d11",
    ),
    (
        "recorded/gitmapsgame.snapshot.json",
        "46bdec12a5750b567d5b6acb2e599e0fb8dfd1117a47ac49ff1c265429e9eaff",
    ),
    (
        "recorded/gitmapsgame.map.json",
        "47e7489f4ab8c83e2bf6597887403bc15ad0b0d742cc96e89c0010f6fc174391",
    ),
    (
        "recorded/cli-cli.recording.json",
        "2feb18004750e365034ed44678bf1448f7843fc27d242b24205828926d03b123",
    ),
    (
        "recorded/cli-cli.snapshot.json",
        "660930e2cd8531af1858e4ab9517b8f29d72e1ab49c12e9cef027c32e1dc28d3",
    ),
    (
        "recorded/cli-cli.map.json",
        "1d31e8796baa51393fd8b39210a59a14f09006bd64c317c7060abc8bc14d17ec",
    ),
];

fn replay_recording(recording: &Value) -> Result<Value, gitmaps::Error> {
    let numbers: Option<Vec<u64>> = recording["numbers"]
        .as_array()
        .map(|n| n.iter().map(|v| v.as_u64().unwrap()).collect());
    let mut transport = ReplayTransport::new(recording["exchanges"].as_array().unwrap().clone());
    let label = recording["label"].as_str().unwrap_or(DEFAULT_LABEL);
    let result = snapshot(
        &mut transport,
        recording["repo"].as_str().unwrap(),
        label,
        numbers.as_deref(),
    )?;
    transport.assert_consumed()?;
    Ok(result)
}

#[test]
fn frozen_fixtures_unchanged() {
    for (path, expected) in FROZEN {
        assert_eq!(sha256_hex(&read(path)), expected, "{path} was modified");
    }
}

#[test]
fn recorded_replay_matches_reference_snapshot() {
    for name in ["gitmapsgame", "cli-cli"] {
        let result = replay_recording(&json(&format!("recorded/{name}.recording.json"))).unwrap();
        let bytes = canonical_bytes(&result).unwrap();
        assert_eq!(
            bytes,
            read(&format!("recorded/{name}.snapshot.json")),
            "{name} snapshot bytes differ"
        );
    }
}

#[test]
fn recorded_derive_matches_reference_map() {
    for name in ["gitmapsgame", "cli-cli"] {
        let snap = json(&format!("recorded/{name}.snapshot.json"));
        let map = derive(&snap).unwrap();
        assert_eq!(
            canonical_bytes(&map).unwrap(),
            read(&format!("recorded/{name}.map.json")),
            "{name} map bytes differ"
        );
        assert_eq!(
            map["source"]["snapshot_sha256"],
            sha256_hex(&read(&format!("recorded/{name}.snapshot.json")))
        );
    }
}

#[test]
fn recorded_cli_cli_states() {
    let map = json("recorded/cli-cli.map.json");
    let states: Vec<(u64, &str)> = map["crossings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["number"].as_u64().unwrap(), c["state"].as_str().unwrap()))
        .collect();
    assert_eq!(
        states,
        [
            (3718, "PROVEN"),
            (11247, "CLAIMED"),
            (13768, "CLAIMED"),
            (14692, "CLOSED_UNPROVEN"),
            (14703, "CLAIMED"),
            (14711, "CLOSED_UNPROVEN"),
            (14718, "PROVEN"),
            (14755, "DISCOVERED"),
            (14780, "CLOSED_UNPROVEN"),
        ]
    );
}

#[test]
fn replay_rejects_divergent_queries() {
    let mut recording = json("recorded/gitmapsgame.recording.json");
    recording["repo"] = "Tesliana-code/Other".into();
    assert_eq!(
        replay_recording(&recording).unwrap_err().kind,
        ErrorKind::GitHub
    );
    let mut truncated = json("recorded/cli-cli.recording.json");
    truncated["exchanges"].as_array_mut().unwrap().pop();
    assert_eq!(
        replay_recording(&truncated).unwrap_err().kind,
        ErrorKind::GitHub
    );
}

#[test]
fn canonical_serialisation_vectors() {
    let vectors = json("parity/canonical.json");
    for v in vectors.as_array().unwrap() {
        let actual = String::from_utf8(canonical_bytes(&v["input"]).unwrap()).unwrap();
        assert_eq!(
            actual,
            v["expected"].as_str().unwrap(),
            "input {}",
            v["input"]
        );
    }
    assert!(vectors.as_array().unwrap().len() >= 50);
}

#[test]
fn python_repr_vectors() {
    let vectors = json("parity/repr.json");
    for v in vectors.as_array().unwrap() {
        assert_eq!(
            py_repr(v["input"].as_str().unwrap()),
            v["expected"].as_str().unwrap(),
            "input {}",
            v["input"]
        );
    }
    assert!(vectors.as_array().unwrap().len() >= 300);
}

#[test]
fn requires_parsing_vectors() {
    for v in json("parity/requires.json").as_array().unwrap() {
        let parsed = parse_requires(v["input"].as_str());
        let refs: Vec<u64> = v["expected"]["refs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_u64().unwrap())
            .collect();
        let unresolved: Vec<&str> = v["expected"]["unresolved"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap())
            .collect();
        assert_eq!(parsed.refs, refs, "refs for {}", v["input"]);
        assert_eq!(
            parsed.unresolved, unresolved,
            "unresolved for {}",
            v["input"]
        );
    }
}

#[test]
fn derive_vectors_match_reference_hashes() {
    let vectors = json("parity/derive.json");
    let accepted = vectors["accepted"].as_array().unwrap();
    assert!(accepted.len() >= 100);
    let mut mismatches = Vec::new();
    for (i, v) in accepted.iter().enumerate() {
        let actual = sha256_hex(&canonical_bytes(&derive(&v["snapshot"]).unwrap()).unwrap());
        if actual != v["map_sha256"].as_str().unwrap() {
            mismatches.push(i);
        }
    }
    assert!(
        mismatches.is_empty(),
        "derive parity mismatches at cases {mismatches:?}"
    );
    for v in vectors["rejected"].as_array().unwrap() {
        assert_eq!(derive(&v["snapshot"]).unwrap_err().kind, ErrorKind::Value);
    }
}

#[test]
fn fetch_vectors_match_reference_hashes() {
    for v in json("parity/fetch.json").as_array().unwrap() {
        let name = v["name"].as_str().unwrap();
        match replay_recording(v) {
            Ok(snap) => {
                assert_eq!(
                    sha256_hex(&canonical_bytes(&snap).unwrap()),
                    v["snapshot_sha256"],
                    "{name} snapshot"
                );
                assert_eq!(
                    sha256_hex(&canonical_bytes(&derive(&snap).unwrap()).unwrap()),
                    v["map_sha256"],
                    "{name} map"
                );
            }
            Err(e) => assert_eq!(
                (v["error"].as_bool(), e.kind),
                (Some(true), ErrorKind::GitHub),
                "{name}: {e}"
            ),
        }
    }
}

#[test]
fn cli_replay_and_derive_write_reference_bytes() {
    let out = std::env::temp_dir().join(format!("gitmaps-parity-{}", std::process::id()));
    std::fs::create_dir_all(&out).unwrap();
    for name in ["gitmapsgame", "cli-cli"] {
        let snap_out = out.join(format!("{name}.snapshot.json"));
        let map_out = out.join(format!("{name}.map.json"));
        let replay = Command::new(env!("CARGO_BIN_EXE_gitmaps"))
            .args([
                "replay",
                fixture(&format!("recorded/{name}.recording.json"))
                    .to_str()
                    .unwrap(),
                "-o",
            ])
            .arg(&snap_out)
            .output()
            .unwrap();
        assert!(
            replay.status.success(),
            "{}",
            String::from_utf8_lossy(&replay.stderr)
        );
        let derive_run = Command::new(env!("CARGO_BIN_EXE_gitmaps"))
            .arg("derive")
            .arg(&snap_out)
            .arg("-o")
            .arg(&map_out)
            .output()
            .unwrap();
        assert!(derive_run.status.success());
        assert_eq!(
            std::fs::read(&snap_out).unwrap(),
            read(&format!("recorded/{name}.snapshot.json"))
        );
        assert_eq!(
            std::fs::read(&map_out).unwrap(),
            read(&format!("recorded/{name}.map.json"))
        );
        let stderr = String::from_utf8_lossy(&derive_run.stderr);
        assert!(
            stderr.contains(&sha256_hex(&read(&format!("recorded/{name}.map.json")))),
            "{stderr}"
        );
    }
    let bad = Command::new(env!("CARGO_BIN_EXE_gitmaps"))
        .args(["derive", "/nonexistent.json"])
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(2));
    std::fs::remove_dir_all(&out).unwrap();
}
