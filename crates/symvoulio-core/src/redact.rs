// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Shared diagnostic sanitization. Raw provider data stays transient for parsing only.
use regex::Regex;
use std::sync::OnceLock;

/// Mask credential-shaped text before diagnostics are persisted or sent over IPC.
pub fn redact(text: &str) -> String {
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        [
            (r"(?i)(\b(?:secret|client_secret|password|token|api[-_]?key|access_?token|refresh_?token)\s*=\s*)[^\s;,&]+", "${1}[REDACTED]"),
            (r"(?im)(\bCookie:\s*)[^\r\n]+", "${1}[REDACTED]"),
            (r"(?i)(\bAuthorization:\s*(?:Basic|Bearer)\s+)[A-Za-z0-9._~+/=-]+", "${1}[REDACTED]"),
            // JSON embedded in a provider JSON string, then ordinary JSON fields.
            (r#"(?i)(\\"(?:secret|client_secret|token|api[-_]?key|access_?token|refresh_?token|password)\\"\s*:\s*\\")(.*?)(\\")"#, "${1}[REDACTED]${3}"),
            (r#"(?i)("(?:secret|client_secret|token|api[-_]?key|access_?token|refresh_?token|password)"\s*:\s*")([^"\\]*(?:\\.[^"\\]*)*)(")"#, "${1}[REDACTED]${3}"),
            (r"(?i)(\b(?:secret|client_secret|password|token|api[-_]?key)\s*:\s*)[^\s,;]+", "${1}[REDACTED]"),
            (r"(?i)([a-z][a-z0-9+.-]*://[^\s/:]+:)[^\s@]+(@)", "${1}[REDACTED]${2}"),
            (r"\bsk-(?:ant-)?[A-Za-z0-9_-]+", "[REDACTED]"),
            (r"(?i:\bBearer)[ \t]+[A-Za-z0-9._~+/=-]+", "[REDACTED]"),
            (r"\b[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b", "[JWT]"),
            (r"[A-Za-z0-9+/=_-]{32,}", "[REDACTED]"),
        ].into_iter().map(|(pattern, replacement)| (Regex::new(pattern).expect("redaction pattern"), replacement)).collect()
    });
    patterns
        .iter()
        .fold(text.to_owned(), |value, (pattern, replacement)| {
            if *replacement == "[JWT]" {
                pattern
                    .replace_all(&value, |capture: &regex::Captures<'_>| {
                        let token = &capture[0];
                        if token
                            .split('.')
                            .all(|part| part.bytes().all(|byte| byte.is_ascii_digit()))
                        {
                            token.to_owned()
                        } else {
                            "[REDACTED]".into()
                        }
                    })
                    .into_owned()
            } else {
                pattern.replace_all(&value, *replacement).into_owned()
            }
        })
}

/// Accept a CLI version only when the entire probe matches known version metadata.
pub(crate) fn version(text: &str) -> Option<String> {
    static VERSION: OnceLock<Regex> = OnceLock::new();
    let text = text.trim();
    VERSION
        .get_or_init(|| {
            Regex::new(r"^(?:codex-cli )?[0-9]{1,5}\.[0-9]{1,5}\.[0-9]{1,5}(?: \(Claude Code\))?$")
                .expect("version pattern")
        })
        .is_match(text)
        .then(|| text.to_owned())
}
