// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Crash-safe output packages; Markdown revision companions support diffing.
use crate::{
    documents::{self, DocumentError, DocumentKind},
    gate::Gate,
    ledger::IssueLedger,
    model::*,
};
use docx_rs::{
    AbstractNumbering, Docx, IndentLevel, Level, LevelJc, LevelText, NumberFormat, Numbering,
    NumberingId, Paragraph, Run, Start, Style, StyleType,
};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use serde::Serialize;
use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;
/// Output package failure.
#[derive(Debug, Error)]
pub enum OutputError {
    /// Filesystem failure.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// JSON serialization failure.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Input extraction failure.
    #[error(transparent)]
    Document(#[from] DocumentError),
    /// DOCX packing failure.
    #[error("DOCX writing failed: {0}")]
    Docx(String),
    /// Invalid or ambiguous package input.
    #[error("invalid package input: {0}")]
    Invalid(String),
}
/// Convenient package result.
pub type Result<T> = std::result::Result<T, OutputError>;
/// Package writer rooted at a run's output directory; serialize calls through one owner.
pub struct OutputPackage {
    root: PathBuf,
    original_text: String,
    captured: Option<std::cell::RefCell<std::collections::BTreeMap<PathBuf, Vec<u8>>>>,
}
impl OutputPackage {
    /// Reopen a package after a restart without replacing any artifacts.
    pub fn reopen(run_dir: &Path, original_text: String) -> Self {
        Self {
            root: run_dir.join("output"),
            original_text,
            captured: None,
        }
    }
    /// Render derived files in memory using the same writer as publication.
    pub(crate) fn inspection(run_dir: &Path, original_text: String) -> Self {
        let mut package = Self::reopen(run_dir, original_text);
        package.captured = Some(Default::default());
        package
    }
    pub(crate) fn captured(self) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        self.captured.expect("inspection package").into_inner()
    }
    /// Copy inputs byte-for-byte and create the output directories.
    pub fn create(run_dir: &Path, primary: &Path, supporting: &[PathBuf]) -> Result<Self> {
        let original = documents::extract(primary)?;
        if original.kind == DocumentKind::Csv {
            return Err(DocumentError::SupportingOnly.into());
        }
        // Resolve every name and read every input before creating any confidential copy.
        let primary_name = file_name(primary)?;
        let primary_bytes = fs::read(primary)?;
        let mut names = std::collections::HashSet::new();
        let mut copies = Vec::new();
        for path in supporting {
            let name = file_name(path)?;
            if !names.insert(name.to_ascii_lowercase()) {
                return Err(OutputError::Invalid(
                    "duplicate supporting file name".into(),
                ));
            }
            copies.push((name.to_owned(), fs::read(path)?));
        }
        let owned = !run_dir.exists();
        fs::create_dir_all(run_dir)?;
        let result = (|| {
            let root = run_dir.join("output");
            fs::create_dir(&root)?;
            let package = Self {
                root,
                original_text: original.text,
                captured: None,
            };
            for directory in [
                "original",
                "revisions",
                "reviews",
                "final",
                "audit/provider_output",
            ] {
                fs::create_dir_all(package.root.join(directory))?;
            }
            package.write(&Path::new("original").join(primary_name), &primary_bytes)?;
            if !copies.is_empty() {
                fs::create_dir(package.root.join("original/supporting"))?;
                for (name, bytes) in copies {
                    package.write(&Path::new("original/supporting").join(name), &bytes)?;
                }
            }
            package.write_human_decision(None)?;
            Ok(package)
        })();
        if result.is_err() && owned {
            let _ = fs::remove_dir_all(run_dir);
        }
        result
    }
    /// Write a DOCX plus the identical Markdown companion required for diffing.
    pub fn write_revision(&self, n: u32, markdown: &str) -> Result<()> {
        if n == 0 {
            return Err(OutputError::Invalid("revision numbers start at 1".into()));
        }
        self.write_docx(&format!("revisions/revision_{n:02}.docx"), markdown)?;
        self.write(
            Path::new(&format!("revisions/revision_{n:02}.md")),
            markdown.as_bytes(),
        )
    }
    /// Save author JSON without revised_document; the revision files hold that text.
    pub fn write_author_response(&self, n: u32, response: &AuthorResponse) -> Result<()> {
        let mut value = serde_json::to_value(response)?;
        value
            .as_object_mut()
            .expect("AuthorResponse is an object")
            .remove("revised_document");
        self.json(&format!("reviews/author_response_{n:02}.json"), &value)
    }
    /// Save pretty reviewer JSON.
    pub fn write_review(&self, n: u32, review: &Review) -> Result<()> {
        self.json(&format!("reviews/review_{n:02}.json"), review)
    }
    /// Redact provider diagnostics before preserving stdout lines with one newline per line and optional stderr.
    pub fn write_provider_output(
        &self,
        n: u32,
        role: Role,
        provider: Provider,
        attempt: u32,
        stdout_lines: &[String],
        stderr: &str,
    ) -> Result<()> {
        let role = wire_name(&role)?;
        let provider = wire_name(&provider)?;
        let base = format!("audit/provider_output/round_{n:02}_{role}_{provider}_a{attempt:02}");
        // Diagnostic bodies are transient. Persist only controlled event counts.
        let metadata = serde_json::json!({"stdout_lines": stdout_lines.len(), "stderr_present": !stderr.is_empty()});
        self.write(
            Path::new(&format!("{base}.jsonl")),
            format!("{metadata}\n").as_bytes(),
        )?;
        let stderr_path = format!("{base}.stderr.txt");
        if !stderr.is_empty() {
            self.write(
                Path::new(&stderr_path),
                b"Provider diagnostic received; body omitted.\n",
            )?;
        } else if self.root.join(&stderr_path).exists() {
            fs::remove_file(self.root.join(stderr_path))?;
        }
        Ok(())
    }
    /// Persist the public run manifest.
    pub fn write_manifest(&self, manifest: &RunManifest) -> Result<()> {
        self.json("audit/run_manifest.json", manifest)
    }
    /// Persist the complete event array.
    pub fn write_timeline(&self, timeline: &[TimelineEvent]) -> Result<()> {
        let mut safe = timeline.to_vec();
        for event in &mut safe {
            event.detail = crate::redact(&event.detail);
        }
        self.json("audit/timeline.json", &safe)
    }
    /// Save final documents from the committed revision selected by the manifest, or the extracted original.
    pub fn write_final(
        &self,
        manifest: &RunManifest,
        ledger: &IssueLedger,
        last_review: Option<&Review>,
        rounds: &[(u32, Gate)],
    ) -> Result<()> {
        // Only a completed review commits a revision. Pending and orphan files stay in revisions/.
        let markdown = if manifest.rounds_completed == 0 {
            self.original_text.clone()
        } else {
            fs::read_to_string(self.root.join(format!(
                "revisions/revision_{:02}.md",
                manifest.rounds_completed
            )))?
        };
        self.write_final_document(manifest, ledger, last_review, rounds, &markdown)
    }
    /// Render final artifacts from authoritative committed text supplied by saved state.
    pub(crate) fn write_final_document(
        &self,
        manifest: &RunManifest,
        ledger: &IssueLedger,
        last_review: Option<&Review>,
        rounds: &[(u32, Gate)],
        markdown: &str,
    ) -> Result<()> {
        self.write_docx("final/final_document.docx", markdown)?;
        self.write(Path::new("final/final_document.md"), markdown.as_bytes())?;
        self.write_human_decision(manifest.human_decision.as_ref())?;
        self.json("final/unresolved_issues.json", &serde_json::json!({ "human_questions": ledger.human_questions().iter().filter(|q| !q.resolved).collect::<Vec<_>>(), "open_issues": ledger.open(), "human_issue_decisions": ledger.human_decisions() }))?;
        let report = report_html(manifest, ledger, last_review, rounds)?;
        self.write(Path::new("final/final_review.html"), report.as_bytes())
    }
    /// Include measured contribution violations in the final report, using an atomic replacement.
    pub fn write_contribution_warnings(&self, exceeded: &[(u32, f64)], limit: f64) -> Result<()> {
        if exceeded.is_empty() {
            return Ok(());
        }
        let mut warning = String::from("<h2>Contribution limit exceeded</h2><ul>");
        for (round, pct) in exceeded {
            warning.push_str(&format!(
                "<li>Round {round}: {pct:.1}% new text; limit {limit:.1}%</li>"
            ));
        }
        warning.push_str("</ul>");
        let report = if let Some(captured) = &self.captured {
            String::from_utf8(captured.borrow()[Path::new("final/final_review.html")].clone())
                .map_err(|e| OutputError::Invalid(e.to_string()))?
        } else {
            fs::read_to_string(self.root.join("final/final_review.html"))?
        }
            .replace("</body>", &format!("{warning}</body>"));
        self.write(Path::new("final/final_review.html"), report.as_bytes())
    }
    /// Write null until a human has made an explicit run decision.
    pub fn write_human_decision(&self, decision: Option<&HumanDecision>) -> Result<()> {
        self.json("final/human_decision.json", &decision)
    }
    /// List sorted file paths relative to output, excluding incomplete temporary files.
    pub fn tree(&self) -> Vec<String> {
        fn visit(root: &Path, path: &Path, files: &mut Vec<String>) {
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        visit(root, &p, files);
                    } else if p.extension().is_none_or(|e| e != "tmp") {
                        if let Ok(relative) = p.strip_prefix(root) {
                            files.push(relative.to_string_lossy().replace('\\', "/"));
                        }
                    }
                }
            }
        }
        let mut files = Vec::new();
        visit(&self.root, &self.root, &mut files);
        files.sort();
        files
    }
    fn json<T: Serialize + ?Sized>(&self, path: &str, value: &T) -> Result<()> {
        self.write(Path::new(path), &serde_json::to_vec_pretty(&serde_json::to_value(value)?)?)
    }
    fn write_docx(&self, path: &str, markdown: &str) -> Result<()> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        markdown_docx(markdown)
            .build()
            .pack(&mut bytes)
            .map_err(|e| OutputError::Docx(e.to_string()))?;
        self.write(Path::new(path), &bytes.into_inner())
    }
    fn write(&self, relative: &Path, bytes: &[u8]) -> Result<()> {
        if let Some(captured) = &self.captured {
            captured.borrow_mut().insert(relative.to_path_buf(), bytes.to_vec());
            return Ok(());
        }
        let path = self.root.join(relative);
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        // std::fs::rename replaces an existing destination on Windows as well as Unix.
        fs::rename(tmp, path)?;
        Ok(())
    }
}
fn file_name(path: &Path) -> Result<&str> {
    path.file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| OutputError::Invalid("file name must be UTF-8".into()))
}
fn wire_name<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_value(value)?
        .as_str()
        .expect("enum is a string")
        .to_owned())
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn report_html(
    manifest: &RunManifest,
    ledger: &IssueLedger,
    review: Option<&Review>,
    rounds: &[(u32, Gate)],
) -> Result<String> {
    let c = ledger.open_counts();
    let decision = if let Some(d) = &manifest.human_decision {
        format!("{} — {} — {}", wire_name(&d.kind)?, d.name, d.note)
    } else {
        "Not decided — this is not an approval.".into()
    };
    let mut html = format!("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>{}</title><style>body{{font:16px system-ui;max-width:1100px;margin:2rem auto;padding:1rem}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #aaa;padding:.5rem;text-align:left;vertical-align:top}}</style><body><h1>{}</h1><p>{}</p><p>Critical: {}; High: {}; Medium: {}; Low: {}</p><h2>Open issues</h2><table><tr><th>ID</th><th>Severity</th><th>Criterion</th><th>Finding</th><th>Evidence</th><th>Proposed action</th><th>Human decision required</th></tr>", escape(&manifest.name), escape(&manifest.name), escape(&decision), c.critical, c.high, c.medium, c.low);
    for i in ledger.open() {
        html.push_str(&format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>", escape(&i.issue_id), escape(&wire_name(&i.severity)?), escape(&i.criterion), escape(&i.finding), escape(&i.evidence), escape(&i.proposed_action), i.human_decision_required));
    }
    html.push_str("</table><h2>Findings reported already resolved</h2><ul>");
    for h in ledger.histories() {
        if let Some(round) = h.resolved_on_arrival() {
            let issue = &h.reviews[0].1;
            html.push_str(&format!("<li>{}: reported already resolved in round {} — {} — Evidence: {}</li>",
                escape(&issue.issue_id), round, escape(&issue.finding), escape(&issue.evidence)));
        }
    }
    html.push_str("</ul>");
    // Keep response-free legacy packages byte-stable during read reconciliation.
    if ledger.histories().any(|h| !h.responses.is_empty()) {
        html.push_str("<h2>Author responses</h2><table><tr><th>Finding</th><th>Round</th><th>Response</th><th>Explanation</th><th>Evidence</th></tr>");
        for h in ledger.histories() {
            for (round, response) in &h.responses {
                html.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    escape(&response.issue_id), round, escape(&wire_name(&response.response)?),
                    escape(&response.explanation), escape(&response.evidence)
                ));
            }
        }
        html.push_str("</table>");
    }
    html.push_str("<h2>Human instructions and risk waivers</h2><ul>");
    for d in ledger.human_decisions() {
        let status = if d.choice == HumanChoice::Keep {
            "waived"
        } else {
            "implement; awaiting reviewer resolution"
        };
        html.push_str(&format!(
            "<li>{}: {}  -  {}</li>",
            escape(&d.issue_id),
            status,
            escape(&d.note)
        ));
    }
    html.push_str("</ul><h2>Unresolved human questions</h2><ul>");
    for q in ledger.human_questions().iter().filter(|q| !q.resolved) {
        html.push_str(&format!(
            "<li>{} (round {}): {}</li>",
            escape(&q.issue_id),
            q.round,
            escape(&q.question)
        ));
    }
    html.push_str("</ul><h2>Answers from the human:</h2><ul>");
    for q in ledger.human_questions() {
        if let Some(answer) = &q.answer {
            html.push_str(&format!(
                "<li>Q ({}): {}<br>A: {}<br>answered by {} at {}</li>",
                escape(&q.issue_id),
                escape(&q.question),
                escape(answer),
                escape(q.answered_by.as_deref().unwrap_or("unknown")),
                escape(q.answered_at.as_deref().unwrap_or("unknown"))
            ));
        }
    }
    html.push_str(
        "</ul><h2>Criteria results</h2><table><tr><th>Criterion</th><th>Met</th><th>Note</th></tr>",
    );
    if let Some(r) = review {
        for c in &r.criteria_results {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape(&c.criterion),
                c.met,
                escape(&c.note)
            ));
        }
    }
    html.push_str("</table><h2>Rounds</h2><ol>");
    for (round, gate) in rounds {
        html.push_str(&format!(
            "<li>Round {}: {} ({})</li>",
            round,
            escape(&wire_name(&gate.kind)?),
            escape(&wire_name(&gate.reason)?)
        ));
    }
    html.push_str("</ol><p>Model agreement is evidence, not approval.</p></body></html>");
    Ok(html)
}
fn markdown_docx(markdown: &str) -> Docx {
    let mut doc = Docx::new();
    let mut next_numbering = 10; // docx-rs reserves its default definitions
    for (id, size, level) in [
        ("Heading1", 32, 0),
        ("Heading2", 28, 1),
        ("Heading3", 24, 2),
    ] {
        doc = doc.add_style(
            Style::new(id, StyleType::Paragraph)
                .name(id)
                .size(size)
                .bold()
                .outline_lvl(level),
        );
    }
    let mut lists = Vec::new();
    let mut paragraph = Paragraph::new();
    let (mut bold, mut italic) = (0_u32, 0_u32);
    let mut has_text = false;
    for event in Parser::new(markdown) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                paragraph = paragraph.style(match level {
                    HeadingLevel::H1 => "Heading1",
                    HeadingLevel::H2 => "Heading2",
                    HeadingLevel::H3 => "Heading3",
                    _ => "Normal",
                });
            }
            Event::Start(Tag::List(start)) => {
                let id = next_numbering;
                next_numbering += 1;
                let level = lists.len().min(8);
                let numbering = AbstractNumbering::new(id).add_level(Level::new(
                    level,
                    Start::new(start.unwrap_or(1) as usize),
                    NumberFormat::new(if start.is_some() { "decimal" } else { "bullet" }),
                    LevelText::new(&if start.is_some() {
                        format!("%{}.", level + 1)
                    } else {
                        "•".into()
                    }),
                    LevelJc::new("left"),
                ));
                doc = doc
                    .add_abstract_numbering(numbering)
                    .add_numbering(Numbering::new(id, id));
                lists.push(id);
            }
            Event::End(TagEnd::List(_)) => {
                lists.pop();
            }
            Event::Start(Tag::Item) => {
                if let Some(id) = lists.last() {
                    // A nested item starts a new paragraph even for tight Markdown lists.
                    if has_text {
                        doc = doc.add_paragraph(paragraph);
                        paragraph = Paragraph::new();
                        has_text = false;
                    }
                    paragraph = paragraph.numbering(
                        NumberingId::new(*id),
                        IndentLevel::new(lists.len().saturating_sub(1).min(8)),
                    );
                }
            }
            Event::Start(Tag::Strong) => bold += 1,
            Event::End(TagEnd::Strong) => bold = bold.saturating_sub(1),
            Event::Start(Tag::Emphasis) => italic += 1,
            Event::End(TagEnd::Emphasis) => italic = italic.saturating_sub(1),
            Event::Text(t) | Event::Code(t) | Event::Html(t) | Event::InlineHtml(t) => {
                let mut run = Run::new().add_text(t.as_ref());
                if bold > 0 {
                    run = run.bold();
                }
                if italic > 0 {
                    run = run.italic();
                }
                paragraph = paragraph.add_run(run);
                has_text = true;
            }
            Event::SoftBreak | Event::HardBreak => {
                paragraph = paragraph.add_run(Run::new().add_text(" "));
            }
            Event::End(
                TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item | TagEnd::CodeBlock,
            ) => {
                if has_text {
                    doc = doc.add_paragraph(paragraph);
                    paragraph = Paragraph::new();
                    has_text = false;
                }
            }
            Event::Rule => {
                paragraph = paragraph.add_run(Run::new().add_text("---"));
                has_text = true;
            }
            _ => (),
        }
    }
    if has_text {
        doc = doc.add_paragraph(paragraph);
    }
    doc
}
