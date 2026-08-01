//! `regex` — regular expressions (PRD §7 follow-up). Pure computation, no
//! capabilities.
//!
//! Patterns are strings compiled per call through a process-wide cache, so
//! hot loops don't recompile (there is no compiled-handle type — the
//! function-only surface matches json/toml). An **invalid pattern is a
//! trappable fault**, not a script `Result`: it is a programming error and
//! scripts have no way to recover a bad pattern at runtime anyway.

use std::collections::HashMap;
use std::sync::Mutex;

use wscript_core::Module;
use wscript_core::host::{Fault, HostError};

/// Compiled-pattern cache. Bounded: on overflow the whole cache is
/// dropped (simple, and 256 live patterns means something unusual).
static CACHE: Mutex<Option<HashMap<String, regex::Regex>>> = Mutex::new(None);
const CACHE_CAP: usize = 256;

fn with_regex<R>(pattern: &str, f: impl FnOnce(&regex::Regex) -> R) -> Result<R, HostError> {
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = guard.get_or_insert_with(HashMap::new);
    if let Some(re) = cache.get(pattern) {
        return Ok(f(re));
    }
    let re = regex::Regex::new(pattern)
        .map_err(|e| HostError::msg(format!("invalid regex pattern: {e}")))?;
    if cache.len() >= CACHE_CAP {
        cache.clear();
    }
    let r = f(&re);
    cache.insert(pattern.to_string(), re);
    Ok(r)
}

pub fn regex() -> Module {
    let mut m = Module::new("regex");
    m.doc("Regular expressions (pure; invalid patterns fault)");

    m.fn_named(
        "is_match",
        ["pattern", "s"],
        |pattern: &str, s: &str| -> Fault<bool> { Fault(with_regex(pattern, |re| re.is_match(s))) },
    );
    m.doc_next("First match, or None");
    m.fn_named(
        "find",
        ["pattern", "s"],
        |pattern: &str, s: &str| -> Fault<Option<String>> {
            Fault(with_regex(pattern, |re| {
                re.find(s).map(|m| m.as_str().to_string())
            }))
        },
    );
    m.doc_next("Every non-overlapping match, in order");
    m.fn_named(
        "find_all",
        ["pattern", "s"],
        |pattern: &str, s: &str| -> Fault<Vec<String>> {
            Fault(with_regex(pattern, |re| {
                re.find_iter(s).map(|m| m.as_str().to_string()).collect()
            }))
        },
    );
    m.doc_next("Replace every match; `$1`/`$name` expand capture groups");
    m.fn_named(
        "replace",
        ["pattern", "s", "rep"],
        |pattern: &str, s: &str, rep: &str| -> Fault<String> {
            Fault(with_regex(pattern, |re| {
                re.replace_all(s, rep).into_owned()
            }))
        },
    );
    m.doc_next(
        "Capture groups of the first match: group 0 (whole match) first; \
         non-participating groups are empty strings",
    );
    m.fn_named(
        "captures",
        ["pattern", "s"],
        |pattern: &str, s: &str| -> Fault<Option<Vec<String>>> {
            Fault(with_regex(pattern, |re| {
                re.captures(s).map(|caps| {
                    (0..caps.len())
                        .map(|i| {
                            caps.get(i)
                                .map_or(String::new(), |m| m.as_str().to_string())
                        })
                        .collect()
                })
            }))
        },
    );
    m.doc_next("Split around every match");
    m.fn_named(
        "split",
        ["pattern", "s"],
        |pattern: &str, s: &str| -> Fault<Vec<String>> {
            Fault(with_regex(pattern, |re| {
                re.split(s).map(str::to_string).collect()
            }))
        },
    );
    m
}
