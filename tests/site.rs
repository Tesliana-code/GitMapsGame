//! Rendering, template, publication and CLI parity with the Python reference, plus
//! transport security checks. Offline: no test here contacts the network.

use gitmaps::canonical::{canonical_bytes, sha256_hex};
use gitmaps::derive::{check_publishable, derive};
use gitmaps::fetch::snapshot;
use gitmaps::github::{
    redact_bodies, LiveTransport, RecordingTransport, ReplayTransport, Transport,
};
use gitmaps::render::{
    check_template, inline_json, refresh_csp, render_html, write_site, DATA_SLOT, TEMPLATE,
};
use gitmaps::ErrorKind;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(path)
}

fn json_file(path: &str) -> Value {
    serde_json::from_slice(&std::fs::read(fixture(path)).unwrap()).unwrap()
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        let path = std::env::temp_dir().join(format!("gitmaps-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn gitmaps(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gitmaps"));
    cmd.args(args)
        .env_remove("GH_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GITHUB_OUTPUT");
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().unwrap()
}

/// Reference outputs of `python3 -m gitmaps render` for the frozen recorded snapshots
/// (approved world-map template with GG favicon, 2026-10-21; map.json/snapshot.json unchanged).
const REFERENCE_SITES: [(&str, &str, &str); 2] = [
    (
        "gitmapsgame",
        "c22af4482df846ba96331ae7da86da9024a3b0f83339198ded7e65629ac4252d",
        "471d70acedd185e6e36fd93f024aaa8538188bb9abb1d73befdc032e147c9a3a",
    ),
    (
        "cli-cli",
        "555adaa5e82d64b84e12386113d969d04cd0a563d50a22baed0dd0a8fb836a12",
        "42bb2c5b3a4d266fe65537897ea0176df5dabc18a3ca846a89324df65eaa7f19",
    ),
];

#[test]
fn embedded_template_is_the_repository_index_html() {
    let on_disk =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("index.html")).unwrap();
    assert_eq!(TEMPLATE, on_disk);
    let vectors = json_file("parity/template.json");
    assert_eq!(
        sha256_hex(TEMPLATE.as_bytes()),
        vectors["template_sha256"],
        "template changed: regenerate vectors"
    );
    check_template(TEMPLATE).unwrap();
}

#[test]
fn recorded_sites_match_reference_bytes() {
    for (name, index_sha, sums_sha) in REFERENCE_SITES {
        let dir = TempDir::new(&format!("site-{name}"));
        let snap = json_file(&format!("recorded/{name}.snapshot.json"));
        let digests = write_site(&dir.0, &snap, &derive(&snap).unwrap()).unwrap();
        let read = |f: &str| std::fs::read(dir.0.join(f)).unwrap();
        assert_eq!(
            sha256_hex(&read("index.html")),
            index_sha,
            "{name} index.html"
        );
        assert_eq!(
            sha256_hex(&read("SHA256SUMS")),
            sums_sha,
            "{name} SHA256SUMS"
        );
        assert_eq!(
            read("map.json"),
            std::fs::read(fixture(&format!("recorded/{name}.map.json"))).unwrap()
        );
        assert_eq!(
            read("snapshot.json"),
            std::fs::read(fixture(&format!("recorded/{name}.snapshot.json"))).unwrap()
        );
        assert_eq!(digests.index_html, index_sha);
        let mut names: Vec<String> = std::fs::read_dir(&dir.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(
            names,
            ["SHA256SUMS", "index.html", "map.json", "snapshot.json"],
            "no temp files left behind"
        );
    }
}

#[test]
fn render_vectors_match_reference_hashes() {
    let accepted = json_file("parity/derive.json")["accepted"]
        .as_array()
        .unwrap()
        .clone();
    let cases = json_file("parity/render.json");
    let mut refused = 0;
    for case in cases.as_array().unwrap() {
        let snap = &accepted[case["case"].as_u64().unwrap() as usize]["snapshot"];
        if case["refused"] == true {
            assert_eq!(
                check_publishable(snap, false).unwrap_err().kind,
                ErrorKind::Permission
            );
            check_publishable(snap, true).unwrap();
            refused += 1;
            continue;
        }
        check_publishable(snap, false).unwrap();
        let html = render_html(&derive(snap).unwrap(), TEMPLATE).unwrap();
        assert_eq!(
            sha256_hex(html.as_bytes()),
            case["index_sha256"],
            "render case {}",
            case["case"]
        );
    }
    assert!(refused > 0 && refused < cases.as_array().unwrap().len());
}

#[test]
fn template_vectors_match_reference() {
    let vectors = json_file("parity/template.json");
    for v in vectors["variants"].as_array().unwrap() {
        let mut html = TEMPLATE.to_string();
        for edit in v["edits"].as_array().unwrap() {
            let (old, new) = (edit[0].as_str().unwrap(), edit[1].as_str().unwrap());
            assert!(html.contains(old), "{} edit not applicable", v["name"]);
            html = html.replacen(old, new, edit[2].as_u64().unwrap() as usize);
        }
        let name = v["name"].as_str().unwrap();
        assert_eq!(
            if check_template(&html).is_ok() {
                "ok"
            } else {
                "error"
            },
            v["check"],
            "{name} check"
        );
        match refresh_csp(&html) {
            Ok(fixed) => {
                assert_eq!(
                    sha256_hex(fixed.as_bytes()),
                    v["fixed_sha256"],
                    "{name} --fix output"
                );
                assert_eq!(
                    if check_template(&fixed).is_ok() {
                        "ok"
                    } else {
                        "error"
                    },
                    v["fixed_check"],
                    "{name}"
                );
            }
            Err(_) => assert!(v["fixed_sha256"].is_null(), "{name} --fix should succeed"),
        }
    }
}

#[test]
fn inline_json_cannot_break_out_and_round_trips() {
    let hostile = json!({"title": "</script><script>alert(1)</script> & <!-- \u{2028}"});
    let text = inline_json(&hostile).unwrap();
    assert!(!text.contains('<') && !text.contains('>') && !text.contains('&'));
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), hostile);
    let map = derive(&json_file("recorded/cli-cli.snapshot.json")).unwrap();
    let html = render_html(&map, TEMPLATE).unwrap();
    assert_eq!(
        html.matches("</script>").count(),
        TEMPLATE.matches("</script>").count()
    );
    assert!(!html.contains(DATA_SLOT));
    assert_eq!(
        render_html(&json!({"mode": "synthetic"}), TEMPLATE)
            .unwrap_err()
            .kind,
        ErrorKind::Value
    );
}

#[test]
fn render_refuses_non_public_and_writes_nothing() {
    for visibility in ["private", "internal"] {
        let dir = TempDir::new(&format!("private-{visibility}"));
        let mut snap = json_file("recorded/cli-cli.snapshot.json"); // constructed: public data relabelled
        snap["repository"]["is_private"] = (visibility == "private").into();
        snap["repository"]["visibility"] = visibility.into();
        let input = dir.0.join("snapshot.json");
        std::fs::write(&input, canonical_bytes(&snap).unwrap()).unwrap();
        let out = dir.0.join("site");
        let output = gitmaps(
            &[
                "render",
                input.to_str().unwrap(),
                "--out",
                out.to_str().unwrap(),
            ],
            &[],
        );
        assert_eq!(output.status.code(), Some(2), "{visibility}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("refusing to publish"));
        assert!(!out.exists(), "{visibility}: nothing may be written");
        let allowed = gitmaps(
            &[
                "render",
                input.to_str().unwrap(),
                "--out",
                out.to_str().unwrap(),
                "--allow-private",
            ],
            &[],
        );
        assert!(allowed.status.success());
        assert!(out.join("index.html").exists());
        let derive_pub = gitmaps(
            &[
                "derive",
                input.to_str().unwrap(),
                "--for-publication",
                "-o",
                "/dev/null",
            ],
            &[],
        );
        assert_eq!(derive_pub.status.code(), Some(2));
    }
}

#[test]
fn cli_render_writes_reference_site_and_github_output() {
    let dir = TempDir::new("cli-render");
    let out = dir.0.join("site");
    let github_output = dir.0.join("github_output");
    std::fs::write(&github_output, "existing=1\n").unwrap();
    let snapshot_path = fixture("recorded/gitmapsgame.snapshot.json");
    let output = gitmaps(
        &[
            "render",
            snapshot_path.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ],
        &[("GITHUB_OUTPUT", github_output.to_str().unwrap())],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let (_, index_sha, sums_sha) = REFERENCE_SITES[0];
    assert_eq!(
        sha256_hex(&std::fs::read(out.join("index.html")).unwrap()),
        index_sha
    );
    assert_eq!(
        sha256_hex(&std::fs::read(out.join("SHA256SUMS")).unwrap()),
        sums_sha
    );
    assert_eq!(
        std::fs::read_to_string(&github_output).unwrap(),
        "existing=1\nsnapshot-sha256=46bdec12a5750b567d5b6acb2e599e0fb8dfd1117a47ac49ff1c265429e9eaff\n\
         map-sha256=47e7489f4ab8c83e2bf6597887403bc15ad0b0d742cc96e89c0010f6fc174391\n"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr,
        format!(
            "{index_sha}  index.html\n47e7489f4ab8c83e2bf6597887403bc15ad0b0d742cc96e89c0010f6fc174391  map.json\n\
             46bdec12a5750b567d5b6acb2e599e0fb8dfd1117a47ac49ff1c265429e9eaff  snapshot.json\n\
             map sha256 47e7489f4ab8c83e2bf6597887403bc15ad0b0d742cc96e89c0010f6fc174391\n"
        )
    );
}

#[test]
fn cli_check_template_detects_and_fixes_stale_csp() {
    let dir = TempDir::new("template");
    let path = dir.0.join("index.html");
    std::fs::write(
        &path,
        TEMPLATE.replacen("TUTORIAL STARTED.", "TUTORIAL STARTED!", 1),
    )
    .unwrap();
    let path = path.to_str().unwrap();
    assert_eq!(
        gitmaps(&["check-template", path], &[]).status.code(),
        Some(2)
    );
    assert!(gitmaps(&["check-template", "--fix", path], &[])
        .status
        .success());
    assert!(gitmaps(&["check-template", path], &[]).status.success());
}

#[test]
fn cli_live_commands_fail_closed_without_token() {
    let dir = TempDir::new("no-token");
    let out = dir.0.join("site");
    for args in [
        vec!["build", "--repo", "o/r", "--out", out.to_str().unwrap()],
        vec![
            "snapshot",
            "--repo",
            "o/r",
            "--record",
            dir.0.join("rec.json").to_str().unwrap(),
        ],
    ] {
        let output = gitmaps(&args, &[]);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("no token"));
    }
    assert!(!out.exists() && !dir.0.join("rec.json").exists());
    for bad in [
        &["build"][..],
        &["render"],
        &["nope"],
        &["derive", "a", "--bogus"],
        &["snapshot", "--repo"],
    ] {
        assert_eq!(gitmaps(bad, &[]).status.code(), Some(2), "{bad:?}");
    }
}

#[test]
fn live_transport_refuses_plain_http_and_never_echoes_the_token() {
    let token = "ghp_DO_NOT_LEAK_0123456789abcdef";
    let mut transport = LiveTransport::new(token, "http://127.0.0.1:9/graphql");
    let err = transport
        .call("repository", "query{viewer{login}}", &json!({}))
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::GitHub);
    assert!(!err.message.contains(token), "{}", err.message);
    let mut unreachable = LiveTransport::new(token, "https://127.0.0.1:9/graphql");
    let err = unreachable.call("repository", "q", &json!({})).unwrap_err();
    assert!(
        err.message
            .starts_with("GitHub GraphQL unreachable for repository"),
        "{}",
        err.message
    );
    assert!(!err.message.contains(token));
}

#[test]
fn recording_keeps_pristine_responses_and_replays() {
    let vector = json_file("parity/fetch.json")
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "pagination")
        .unwrap()
        .clone();
    let exchanges = vector["exchanges"].as_array().unwrap().clone();
    let mut recorder = RecordingTransport::new(ReplayTransport::new(exchanges.clone()), None);
    let live = snapshot(
        &mut recorder,
        vector["repo"].as_str().unwrap(),
        "gitmaps:crossing",
        None,
    )
    .unwrap();
    assert_eq!(
        recorder.exchanges, exchanges,
        "recording must equal what GitHub sent"
    );
    let mut replay = ReplayTransport::new(recorder.exchanges.clone());
    let again = snapshot(
        &mut replay,
        vector["repo"].as_str().unwrap(),
        "gitmaps:crossing",
        None,
    )
    .unwrap();
    replay.assert_consumed().unwrap();
    assert_eq!(
        canonical_bytes(&again).unwrap(),
        canonical_bytes(&live).unwrap()
    );
}

#[test]
fn redaction_blanks_every_body() {
    let redacted = redact_bodies(
        "issue_by_number",
        json!({"data": {"body": "Requires: #1", "nested": [{"body": {"x": 1}, "title": "kept"}]}}),
    );
    assert_eq!(
        redacted,
        json!({"data": {"body": "", "nested": [{"body": "", "title": "kept"}]}})
    );
}
