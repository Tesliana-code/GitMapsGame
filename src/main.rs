//! `gitmaps` command line, matching the Python reference (`python3 -m gitmaps`).

use gitmaps::canonical::{canonical_bytes, sha256_hex};
use gitmaps::derive::{check_publishable, derive};
use gitmaps::fetch::snapshot;
use gitmaps::github::{redact_bodies, LiveTransport, RecordingTransport, ReplayTransport};
use gitmaps::render::{check_template, refresh_csp, write_site};
use gitmaps::{Error, DEFAULT_LABEL};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "usage: gitmaps <command> [options]

  snapshot --repo OWNER/NAME [--label L | --issue N ...] [--record FILE [--redact-bodies]] [-o FILE]
      query GitHub (read-only) and write a snapshot
  replay RECORDING [-o FILE]
      rebuild a snapshot from a recording, without network
  derive SNAPSHOT [-o FILE] [--for-publication [--allow-private]]
      derive map.json from a snapshot
  render SNAPSHOT --out DIR [--allow-private]
      derive and render a site from a snapshot file
  build --repo OWNER/NAME [--label L] --out DIR [--allow-private]
      snapshot GitHub, derive and render a site
  check-template [--fix] [FILE]
      verify the index.html CSP hash and data slot (default FILE: index.html)

Token: GH_TOKEN or GITHUB_TOKEN. Files default to stdout (-). sha256 digests go to stderr.";

/// Parsed arguments: positionals, `--option value` (repeatable) and boolean `--flag`s.
struct Args {
    positional: Vec<String>,
    options: BTreeMap<&'static str, Vec<String>>,
    flags: Vec<&'static str>,
}

impl Args {
    fn parse(
        raw: &[String],
        options: &[&'static str],
        flags: &[&'static str],
    ) -> Result<Args, Error> {
        let mut args = Args {
            positional: Vec::new(),
            options: BTreeMap::new(),
            flags: Vec::new(),
        };
        let mut iter = raw.iter();
        while let Some(arg) = iter.next() {
            let name = if arg == "-o" {
                "--output"
            } else {
                arg.as_str()
            };
            if let Some(&opt) = options.iter().find(|&&o| o == name) {
                let value = iter
                    .next()
                    .ok_or_else(|| Error::value(format!("{arg} needs a value\n\n{USAGE}")))?;
                args.options.entry(opt).or_default().push(value.clone());
            } else if let Some(&flag) = flags.iter().find(|&&f| f == name) {
                args.flags.push(flag);
            } else if arg.starts_with('-') && arg != "-" {
                return Err(Error::value(format!("unknown option {arg}\n\n{USAGE}")));
            } else {
                args.positional.push(arg.clone());
            }
        }
        Ok(args)
    }

    fn one(&self, name: &str) -> Option<&str> {
        self.options
            .get(name)
            .and_then(|v| v.last())
            .map(String::as_str)
    }

    fn required(&self, name: &str) -> Result<&str, Error> {
        self.one(name)
            .ok_or_else(|| Error::value(format!("{name} is required\n\n{USAGE}")))
    }

    fn flag(&self, name: &str) -> bool {
        self.flags.contains(&name)
    }

    fn input(&self) -> Result<&str, Error> {
        match self.positional.as_slice() {
            [one] => Ok(one),
            _ => Err(Error::value(format!(
                "expected exactly one input file\n\n{USAGE}"
            ))),
        }
    }
}

fn read_json(path: &str) -> Result<Value, Error> {
    let data = std::fs::read(path).map_err(|e| Error::io(format!("cannot read {path}: {e}")))?;
    serde_json::from_slice(&data)
        .map_err(|e| Error::value(format!("{path} is not valid JSON: {e}")))
}

fn write_json(path: &str, value: &Value) -> Result<String, Error> {
    let data = canonical_bytes(value)?;
    if path == "-" {
        std::io::stdout()
            .write_all(&data)
            .map_err(|e| Error::io(format!("cannot write stdout: {e}")))?;
    } else {
        std::fs::write(path, &data).map_err(|e| Error::io(format!("cannot write {path}: {e}")))?;
    }
    Ok(sha256_hex(&data))
}

/// Refuse non-public repositories before anything is written, then write the site.
fn site(out_dir: &str, snap: &Value, allow_private: bool) -> Result<String, Error> {
    check_publishable(snap, allow_private)?;
    let digests = write_site(Path::new(out_dir), snap, &derive(snap)?)?;
    if let Ok(github_output) = std::env::var("GITHUB_OUTPUT") {
        let mut handle = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&github_output)
            .map_err(|e| Error::io(format!("cannot open GITHUB_OUTPUT: {e}")))?;
        writeln!(
            handle,
            "snapshot-sha256={}\nmap-sha256={}",
            digests.snapshot_json, digests.map_json
        )
        .map_err(|e| Error::io(format!("cannot write GITHUB_OUTPUT: {e}")))?;
    }
    eprintln!("{}  index.html", digests.index_html);
    eprintln!("{}  map.json", digests.map_json);
    eprintln!("{}  snapshot.json", digests.snapshot_json);
    Ok(format!("map sha256 {}", digests.map_json))
}

fn issue_numbers(args: &Args) -> Result<Option<Vec<u64>>, Error> {
    args.options
        .get("--issue")
        .map(|values| {
            values
                .iter()
                .map(|v| {
                    v.parse::<u64>()
                        .map_err(|_| Error::value(format!("--issue {v} is not an issue number")))
                })
                .collect()
        })
        .transpose()
}

fn run(raw: &[String]) -> Result<String, Error> {
    let Some((command, rest)) = raw.split_first() else {
        return Err(Error::value(USAGE));
    };
    match command.as_str() {
        "snapshot" => {
            let args = Args::parse(
                rest,
                &["--repo", "--label", "--issue", "--record", "--output"],
                &["--redact-bodies"],
            )?;
            let repo = args.required("--repo")?;
            let label = args.one("--label").unwrap_or(DEFAULT_LABEL);
            let numbers = issue_numbers(&args)?;
            let live = LiveTransport::from_env()?;
            let (result, recording) = if let Some(record) = args.one("--record") {
                let redact = args
                    .flag("--redact-bodies")
                    .then_some(redact_bodies as fn(&str, Value) -> Value);
                let mut transport = RecordingTransport::new(live, redact);
                let result = snapshot(&mut transport, repo, label, numbers.as_deref())?;
                (result, Some((record, transport.exchanges)))
            } else {
                let mut transport = live;
                (
                    snapshot(&mut transport, repo, label, numbers.as_deref())?,
                    None,
                )
            };
            if let Some((path, exchanges)) = recording {
                write_json(
                    path,
                    &json!({
                        "repo": repo, "label": label, "numbers": numbers,
                        "body_redacted": args.flag("--redact-bodies"), "exchanges": exchanges,
                    }),
                )?;
            }
            Ok(format!(
                "sha256 {}",
                write_json(args.one("--output").unwrap_or("-"), &result)?
            ))
        }
        "replay" => {
            let args = Args::parse(rest, &["--output"], &[])?;
            let recording = read_json(args.input()?)?;
            let repo = recording["repo"]
                .as_str()
                .ok_or_else(|| Error::value("recording has no repo"))?;
            let label = recording["label"].as_str().unwrap_or(DEFAULT_LABEL);
            let numbers: Option<Vec<u64>> = recording["numbers"]
                .as_array()
                .map(|n| n.iter().filter_map(Value::as_u64).collect());
            let exchanges = recording["exchanges"]
                .as_array()
                .cloned()
                .ok_or_else(|| Error::value("recording has no exchanges"))?;
            let mut transport = ReplayTransport::new(exchanges);
            let result = snapshot(&mut transport, repo, label, numbers.as_deref())?;
            transport.assert_consumed()?;
            Ok(format!(
                "sha256 {}",
                write_json(args.one("--output").unwrap_or("-"), &result)?
            ))
        }
        "derive" => {
            let args = Args::parse(
                rest,
                &["--output"],
                &["--for-publication", "--allow-private"],
            )?;
            let snap = read_json(args.input()?)?;
            if args.flag("--for-publication") {
                check_publishable(&snap, args.flag("--allow-private"))?;
            }
            Ok(format!(
                "sha256 {}",
                write_json(args.one("--output").unwrap_or("-"), &derive(&snap)?)?
            ))
        }
        "render" => {
            let args = Args::parse(rest, &["--out"], &["--allow-private"])?;
            let snap = read_json(args.input()?)?;
            site(args.required("--out")?, &snap, args.flag("--allow-private"))
        }
        "build" => {
            let args = Args::parse(rest, &["--repo", "--label", "--out"], &["--allow-private"])?;
            let out = args.required("--out")?;
            let mut transport = LiveTransport::from_env()?;
            let snap = snapshot(
                &mut transport,
                args.required("--repo")?,
                args.one("--label").unwrap_or(DEFAULT_LABEL),
                None,
            )?;
            site(out, &snap, args.flag("--allow-private"))
        }
        "check-template" => {
            let args = Args::parse(rest, &[], &["--fix"])?;
            let path = match args.positional.as_slice() {
                [] => "index.html",
                [one] => one.as_str(),
                _ => return Err(Error::value(USAGE)),
            };
            let mut html = std::fs::read_to_string(path)
                .map_err(|e| Error::io(format!("cannot read {path}: {e}")))?;
            if args.flag("--fix") {
                html = refresh_csp(&html)?;
                std::fs::write(path, &html)
                    .map_err(|e| Error::io(format!("cannot write {path}: {e}")))?;
            }
            check_template(&html)?;
            Ok("template ok".into())
        }
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(String::new())
        }
        other => Err(Error::value(format!("unknown command {other}\n\n{USAGE}"))),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(summary) => {
            if !summary.is_empty() {
                eprintln!("{summary}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("gitmaps: error: {e}");
            ExitCode::from(2)
        }
    }
}
