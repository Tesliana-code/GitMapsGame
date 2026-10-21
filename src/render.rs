//! Render a derived map into a static, self-contained site directory. Port of `gitmaps/render.py`.
//!
//! The shipped `index.html` is compiled into the binary, so an installed `gitmaps` needs no
//! source checkout. The page's CSP must allow exactly its one inline script (by sha256).

use crate::canonical::{canonical_bytes, sha256_hex};
use crate::Error;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

/// The repository's `index.html`, byte for byte.
pub const TEMPLATE: &str = include_str!("../index.html");
pub const DATA_SLOT: &str = r#"<script type="application/json" id="gitmaps-data">null</script>"#;
const SCRIPT_OPEN: &str = "<script>";
const SCRIPT_CLOSE: &str = "</script>";
const CSP_PREFIX: &str = "script-src 'sha256-";

fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> shift) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Contents of every `<script>…</script>` element (the reference's non-greedy DOTALL findall).
fn inline_scripts(html: &str) -> Vec<&str> {
    let mut scripts = Vec::new();
    let mut rest = html;
    while let Some(open) = rest.find(SCRIPT_OPEN) {
        let body = &rest[open + SCRIPT_OPEN.len()..];
        let Some(close) = body.find(SCRIPT_CLOSE) else {
            break;
        };
        scripts.push(&body[..close]);
        rest = &body[close + SCRIPT_CLOSE.len()..];
    }
    scripts
}

/// Byte ranges of each `script-src 'sha256-([A-Za-z0-9+/=]+)'` match and of its hash group.
fn csp_hashes(html: &str) -> Vec<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    let is_b64 = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'=');
    let bytes = html.as_bytes();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(offset) = html[from..].find(CSP_PREFIX) {
        let start = from + offset;
        let hash_start = start + CSP_PREFIX.len();
        let mut end = hash_start;
        while end < bytes.len() && is_b64(bytes[end]) {
            end += 1;
        }
        if end > hash_start && bytes.get(end) == Some(&b'\'') {
            found.push((start..end + 1, hash_start..end));
            from = end + 1;
        } else {
            from = start + 1;
        }
    }
    found
}

pub fn script_hash(html: &str) -> Result<String, Error> {
    let scripts = inline_scripts(html);
    if scripts.len() != 1 {
        return Err(Error::value(
            "template must contain exactly one executable inline script",
        ));
    }
    Ok(base64(&Sha256::digest(scripts[0].as_bytes())))
}

/// The Content-Security-Policy must allow exactly the shipped script and nothing else.
pub fn check_template(html: &str) -> Result<(), Error> {
    let expected = script_hash(html)?;
    let declared: Vec<&str> = csp_hashes(html)
        .into_iter()
        .map(|(_, h)| &html[h])
        .collect();
    if declared != [expected.as_str()] {
        return Err(Error::value(format!(
            "template CSP script hash is stale; expected sha256-{expected}"
        )));
    }
    if html.matches(DATA_SLOT).count() != 1 {
        return Err(Error::value(
            "template must contain exactly one empty data slot",
        ));
    }
    Ok(())
}

/// Maintainer helper: rewrite the first CSP hash so it allows the template's current script.
pub fn refresh_csp(html: &str) -> Result<String, Error> {
    let hash = script_hash(html)?;
    Ok(match csp_hashes(html).first() {
        Some((whole, _)) => format!(
            "{}{CSP_PREFIX}{hash}'{}",
            &html[..whole.start],
            &html[whole.end..]
        ),
        None => html.to_string(),
    })
}

/// Map bytes made safe inside <script>: no '<', '>' or '&' can close or confuse the element.
pub fn inline_json(value: &Value) -> Result<String, Error> {
    let text = String::from_utf8(canonical_bytes(value)?).expect("canonical JSON is ASCII");
    Ok(text
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026"))
}

pub fn render_html(map: &Value, template: &str) -> Result<String, Error> {
    check_template(template)?;
    if map["mode"] != "real" {
        return Err(Error::value(
            "only real-mode maps are rendered; synthetic mode is the unmodified template",
        ));
    }
    let slot = format!(
        r#"<script type="application/json" id="gitmaps-data">{}</script>"#,
        inline_json(map)?
    );
    Ok(template.replace(DATA_SLOT, &slot))
}

/// sha256 digests of the files written by [`write_site`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteDigests {
    pub index_html: String,
    pub map_json: String,
    pub snapshot_json: String,
}

/// Write index.html, map.json, snapshot.json and SHA256SUMS, each replaced atomically.
pub fn write_site(out_dir: &Path, snapshot: &Value, map: &Value) -> Result<SiteDigests, Error> {
    let io = |what: &str, e: std::io::Error| Error::io(format!("cannot write {what}: {e}"));
    std::fs::create_dir_all(out_dir).map_err(|e| io(&out_dir.display().to_string(), e))?;
    let index = render_html(map, TEMPLATE)?.into_bytes();
    let map_bytes = canonical_bytes(map)?;
    let snapshot_bytes = canonical_bytes(snapshot)?;
    let digests = SiteDigests {
        index_html: sha256_hex(&index),
        map_json: sha256_hex(&map_bytes),
        snapshot_json: sha256_hex(&snapshot_bytes),
    };
    let sums = format!(
        "{}  index.html\n{}  map.json\n{}  snapshot.json\n",
        digests.index_html, digests.map_json, digests.snapshot_json
    );
    let files: [(&str, &[u8]); 4] = [
        ("index.html", &index),
        ("map.json", &map_bytes),
        ("snapshot.json", &snapshot_bytes),
        ("SHA256SUMS", sums.as_bytes()),
    ];
    for (name, data) in files {
        let target = out_dir.join(name);
        let tmp = out_dir.join(format!("{name}.tmp"));
        std::fs::write(&tmp, data).map_err(|e| io(name, e))?;
        std::fs::rename(&tmp, &target).map_err(|e| io(name, e))?;
    }
    Ok(digests)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648() {
        for (input, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), expected);
        }
    }

    #[test]
    fn embedded_template_is_valid() {
        check_template(TEMPLATE).unwrap();
    }
}
