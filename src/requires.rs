//! `Requires: #N, #M` parsing, matching the reference regular expressions exactly:
//!
//! ```text
//! line:  ^\s*(?:[-*]\s*)?(?:\*\*)?requires(?:\*\*)?\s*:\s*(?:\*\*)?(.*)$   (IGNORECASE)
//! fence: ^\s*(```|~~~)
//! split: [,\s]+          ref: ^#([1-9][0-9]*)$
//! ```
//!
//! `\s` is Python's Unicode whitespace (`str.isspace`), and IGNORECASE lets `i` also match
//! U+0130/U+0131 and `s` also match U+017F, as Python's `re` module does.

use std::collections::BTreeSet;

/// Python `str.isspace()`; identical to `re`'s `\s` for str patterns.
pub fn is_py_space(c: char) -> bool {
    matches!(
        c,
        '\t'..='\r'
            | '\u{1c}'..='\u{20}'
            | '\u{85}'
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
    )
}

fn skip_space(s: &str) -> &str {
    s.trim_start_matches(is_py_space)
}

fn ignore_case_eq(c: char, pattern: char) -> bool {
    c.to_ascii_lowercase() == pattern
        || matches!(
            (pattern, c),
            ('i', '\u{130}') | ('i', '\u{131}') | ('s', '\u{17f}')
        )
}

fn strip_keyword(s: &str) -> Option<&str> {
    let mut chars = s.char_indices();
    for pattern in "requires".chars() {
        let (_, c) = chars.next()?;
        if !ignore_case_eq(c, pattern) {
            return None;
        }
    }
    Some(chars.next().map_or("", |(i, _)| &s[i..]))
}

/// Returns the captured remainder of a `Requires:` line, trying alternatives in the same
/// order as the backtracking regex engine (greedy optional groups first).
fn match_line(line: &str) -> Option<&str> {
    let start = skip_space(line);
    let mut bullets = Vec::with_capacity(2);
    if let Some(rest) = start.strip_prefix(['-', '*']) {
        bullets.push(skip_space(rest));
    }
    bullets.push(start);
    for after_bullet in bullets {
        let mut bolds = Vec::with_capacity(2);
        if let Some(rest) = after_bullet.strip_prefix("**") {
            bolds.push(rest);
        }
        bolds.push(after_bullet);
        for after_bold in bolds {
            let Some(after_word) = strip_keyword(after_bold) else {
                continue;
            };
            let mut closers = Vec::with_capacity(2);
            if let Some(rest) = after_word.strip_prefix("**") {
                closers.push(rest);
            }
            closers.push(after_word);
            for after_closer in closers {
                if let Some(rest) = skip_space(after_closer).strip_prefix(':') {
                    let rest = skip_space(rest);
                    return Some(rest.strip_prefix("**").unwrap_or(rest));
                }
            }
        }
    }
    None
}

fn is_fence(line: &str) -> bool {
    let rest = skip_space(line);
    rest.starts_with("```") || rest.starts_with("~~~")
}

fn issue_ref(token: &str) -> Option<u64> {
    let digits = token.strip_prefix('#')?;
    let bytes = digits.as_bytes();
    if bytes.is_empty()
        || !(b'1'..=b'9').contains(&bytes[0])
        || !bytes.iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    digits.parse().ok()
}

#[derive(Debug, PartialEq, Eq, Default)]
pub struct Requires {
    pub refs: Vec<u64>,
    pub unresolved: Vec<String>,
}

pub fn parse_requires(body: Option<&str>) -> Requires {
    let body = body.unwrap_or("").replace("\r\n", "\n");
    let mut refs = BTreeSet::new();
    let mut unresolved = BTreeSet::new();
    let mut in_fence = false;
    for line in body.split('\n') {
        if is_fence(line) {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let Some(rest) = match_line(line) else {
            continue;
        };
        let tokens: Vec<&str> = rest
            .trim_matches(is_py_space)
            .split(|c: char| c == ',' || is_py_space(c))
            .filter(|t| !t.is_empty())
            .collect();
        if tokens.is_empty()
            || (tokens.len() == 1 && (tokens[0].eq_ignore_ascii_case("none") || tokens[0] == "-"))
        {
            continue;
        }
        for token in tokens {
            match issue_ref(token) {
                Some(n) => {
                    refs.insert(n);
                }
                None => {
                    unresolved.insert(token.to_string());
                }
            }
        }
    }
    Requires {
        refs: refs.into_iter().collect(),
        unresolved: unresolved.into_iter().collect(),
    }
}
