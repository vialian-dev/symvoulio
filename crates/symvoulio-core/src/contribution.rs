// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Observable new text proxies, never a measure of value.
use similar::{ChangeTag, TextDiff};
use std::collections::HashSet;
/// Inserted whitespace-delimited words divided by previous words, rounded to 0.1 percent.
/// An empty previous document yields 0 for empty output and 100 for nonempty output.
pub fn new_text_pct(previous: &str, revised: &str) -> f64 {
    let old: Vec<&str> = previous.split_whitespace().collect();
    let new: Vec<&str> = revised.split_whitespace().collect();
    if old.is_empty() {
        return if new.is_empty() { 0.0 } else { 100.0 };
    }
    let inserted = TextDiff::from_slices(&old, &new)
        .iter_all_changes()
        .filter(|c| c.tag() == ChangeTag::Insert)
        .count();
    (inserted as f64 / old.len() as f64 * 1000.0).round() / 10.0
}
/// Count distinct Markdown heading texts in revised that are absent from previous.
pub fn new_sections(previous: &str, revised: &str) -> usize {
    fn headings(text: &str) -> HashSet<String> {
        use pulldown_cmark::{Event, Parser, Tag, TagEnd};
        let mut result = HashSet::new();
        let mut heading = None;
        for event in Parser::new(text) {
            match event {
                Event::Start(Tag::Heading { .. }) => heading = Some(String::new()),
                Event::Text(t) | Event::Code(t) => {
                    if let Some(h) = &mut heading {
                        h.push_str(&t);
                    }
                }
                Event::End(TagEnd::Heading(_)) => {
                    if let Some(h) = heading.take() {
                        result.insert(h);
                    }
                }
                _ => (),
            }
        }
        result
    }
    headings(revised).difference(&headings(previous)).count()
}
