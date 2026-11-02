// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Text extraction with original-byte hashes and a provider input size guard.
use docx_rs::*;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
use thiserror::Error;
/// Maximum UTF-8 extracted text bytes (below Claude's 10 MB stdin cap).
pub const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;
/// Input document format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    /// UTF-8 plain text.
    Txt,
    /// UTF-8 Markdown.
    Md,
    /// Office Open XML document.
    Docx,
    /// PDF with a text layer.
    Pdf,
    /// Supporting CSV text.
    Csv,
}
/// Extraction failure with a user-readable explanation.
#[derive(Debug, Error)]
pub enum DocumentError {
    /// File access failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Invalid text encoding.
    #[error("input must be valid UTF-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    /// Unsupported extension.
    #[error("unsupported document type: {0}")]
    Unsupported(String),
    /// CSV cannot be the primary document.
    #[error("CSV is supported for supporting files only")]
    SupportingOnly,
    /// Primary document has no text to review.
    #[error("Primary document is empty or contains only whitespace. Choose a document with text before starting.")]
    EmptyPrimary,
    /// Missing PDF text layer.
    #[error("no text layer: scanned PDFs are not supported in v0.1")]
    ScannedPdf,
    /// Extraction failed for a malformed document.
    #[error("document extraction failed: {0}")]
    Invalid(String),
    /// Extracted text exceeds the provider guard.
    #[error("extracted text exceeds 8 MB; Claude stdin cap is 10 MB")]
    TooLarge,
}
/// Extracted text and identity of the original bytes.
#[derive(Debug, Clone)]
pub struct Extracted {
    /// Plain text or Markdown in source order.
    pub text: String,
    /// Input format.
    pub kind: DocumentKind,
    /// Original file length.
    pub bytes: u64,
    /// Lowercase SHA-256 of the original bytes.
    pub sha256: String,
}
/// Extract an input; callers selecting a primary must reject DocumentKind::Csv.
pub fn extract(path: &Path) -> Result<Extracted, DocumentError> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let kind = match extension.as_str() {
        "txt" => DocumentKind::Txt,
        "md" => DocumentKind::Md,
        "docx" => DocumentKind::Docx,
        "pdf" => DocumentKind::Pdf,
        "csv" => DocumentKind::Csv,
        _ => return Err(DocumentError::Unsupported(extension)),
    };
    let bytes = fs::read(path)?;
    let text = match kind {
        DocumentKind::Csv => String::from_utf8(bytes.clone())?,
        DocumentKind::Txt | DocumentKind::Md => {
            let text = String::from_utf8(bytes.clone())?;
            text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned()
        }
        DocumentKind::Docx => {
            let damaged =
                || DocumentError::Invalid("the .docx file is damaged or not supported".into());
            let mut archive =
                zip::ZipArchive::new(std::io::Cursor::new(&bytes)).map_err(|_| damaged())?;
            let mut total = 0_u64;
            for index in 0..archive.len() {
                let entry = archive.by_index(index).map_err(|_| damaged())?;
                total = total.checked_add(entry.size()).ok_or_else(damaged)?;
                if entry.size() > 32 * 1024 * 1024 || total > 64 * 1024 * 1024 {
                    return Err(DocumentError::Invalid(
                        ".docx expansion exceeds 64 MB total or 32 MB per entry".into(),
                    ));
                }
            }
            let doc = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_docx(&bytes)))
                .map_err(|_| damaged())?
                .map_err(|_| damaged())?;
            // docx-rs 0.4.22 does not retain w:default on Style.
            let default_style = match archive.by_name("word/styles.xml") {
                Ok(mut entry) => {
                    let mut xml = String::new();
                    std::io::Read::read_to_string(&mut entry, &mut xml)?;
                    default_paragraph_style(&xml)?
                }
                Err(zip::result::ZipError::FileNotFound) => None,
                Err(_) => return Err(damaged()),
            };
            let mut numbering = NumberingResolver::new(&doc.numberings, &doc.styles)?;
            numbering.default_style = default_style;
            let mut blocks = Vec::new();
            for child in &doc.document.children {
                match child {
                    DocumentChild::Paragraph(p) => blocks.push(numbering.paragraph(p)?),
                    DocumentChild::Table(t) => blocks.push(numbering.table(t)?),
                    _ => (),
                }
            }
            blocks.join("\n\n")
        }
        DocumentKind::Pdf => {
            let text = pdf_extract::extract_text_from_mem(&bytes)
                .map_err(|e| DocumentError::Invalid(e.to_string()))?;
            if text.chars().filter(|c| !c.is_whitespace()).count() < 20 {
                return Err(DocumentError::ScannedPdf);
            }
            text
        }
    };
    if text.len() > MAX_TEXT_BYTES {
        return Err(DocumentError::TooLarge);
    }
    Ok(Extracted {
        text,
        kind,
        bytes: bytes.len() as u64,
        sha256: format!("{:x}", Sha256::digest(&bytes)),
    })
}
fn paragraph(p: &Paragraph) -> String {
    let prefix = match p.property.style.as_ref().map(|s| s.val.as_str()) {
        Some("Heading1" | "Title") => "# ",
        Some("Heading2") => "## ",
        Some("Heading3") => "### ",
        _ => "",
    };
    let mut text = prefix.to_owned();
    paragraph_children(&p.children, &mut text);
    text
}
fn paragraph_children(children: &[ParagraphChild], text: &mut String) {
    for child in children {
        match child {
            ParagraphChild::Run(run) => {
                for c in &run.children {
                    match c {
                        RunChild::Text(t) => text.push_str(&t.text),
                        RunChild::Tab(_) => text.push('\t'),
                        RunChild::Break(_) | RunChild::CarriageReturn(_) => text.push('\n'),
                        _ => (),
                    }
                }
            }
            ParagraphChild::Hyperlink(h) => paragraph_children(&h.children, text),
            _ => (),
        }
    }
}
fn default_paragraph_style(xml: &str) -> Result<Option<String>, DocumentError> {
    use quick_xml::{events::Event, reader::NsReader};
    const W: &[u8] = b"http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let mut reader = NsReader::from_str(xml);
    let mut default_style = None;
    loop {
        let event = reader.read_resolved_event()
            .map_err(|e| DocumentError::Invalid(format!("invalid styles XML: {e}")))?;
        match event {
            (quick_xml::name::ResolveResult::Bound(ns), Event::Start(e) | Event::Empty(e))
                if ns.as_ref() == W && e.local_name().as_ref() == b"style" => {
                let mut id = None;
                let mut paragraph = false;
                let mut default = false;
                for a in e.attributes() {
                    let a = a.map_err(|e| DocumentError::Invalid(format!("invalid style attribute: {e}")))?;
                    let (ns, name) = reader.resolver().resolve_attribute(a.key);
                    if !matches!(ns, quick_xml::name::ResolveResult::Bound(ns) if ns.as_ref() == W) {
                        continue;
                    }
                    let value = a.decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, reader.decoder())
                        .map_err(|e| DocumentError::Invalid(format!("invalid style value: {e}")))?;
                    match name.as_ref() {
                        b"type" => paragraph = value == "paragraph",
                        b"default" => default = matches!(value.as_ref(), "1" | "true" | "on"),
                        b"styleId" => id = Some(value.into_owned()),
                        _ => (),
                    }
                }
                if paragraph && default { default_style = id; }
            }
            (_, Event::Eof) => return Ok(default_style),
            _ => (),
        }
    }
}
#[derive(Clone)]
struct NumberLevel {
    start: u32,
    format: String,
    text: String,
    restart: Option<usize>,
}
type Levels = std::collections::HashMap<usize, NumberLevel>;
// An override isolates only its level; all other levels share the abstract count.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum CounterOwner { Abstract(usize), Instance(usize) }
struct NumberingResolver {
    counter_sources: std::collections::HashMap<(usize, usize), CounterOwner>,
    default_style: Option<String>,
    nums: std::collections::HashMap<usize, Levels>,
    styles: Styles,
    counts: std::collections::HashMap<(CounterOwner, usize), u32>,
}
impl NumberingResolver {
    fn new(numberings: &Numberings, styles: &Styles) -> Result<Self, DocumentError> {
        fn level(l: &Level) -> Result<NumberLevel, DocumentError> {
            Ok(NumberLevel {
                // Start's Serialize implementation truncates usize to u32 in docx-rs.
                // Its generated XML preserves the original value, so validate that instead.
                start: regex::Regex::new(r#"val="([0-9]+)""#).expect("start XML")
                    .captures(&String::from_utf8_lossy(&l.start.build()))
                    .and_then(|c| c[1].parse::<u32>().ok())
                    .ok_or_else(|| DocumentError::Invalid("invalid numbering start".into()))?,
                format: l.format.val.clone(),
                text: serde_json::to_value(&l.text)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default(),
                restart: l
                    .level_restart
                    .as_ref()
                    .and_then(|v| serde_json::to_value(v).ok())
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize),
            })
        }
        // Preserve first-match semantics for duplicate IDs while indexing each link once.
        let mut abstracts = std::collections::HashMap::new();
        let mut style_links = std::collections::HashMap::new();
        let mut numbering_styles = std::collections::HashMap::new();
        let mut instances = std::collections::HashMap::new();
        for a in &numberings.abstract_nums {
            abstracts.entry(a.id).or_insert(a);
            if let Some(link) = &a.style_link {
                style_links.entry(link.as_str()).or_insert(a.id);
            }
        }
        for style in &styles.styles {
            if style.style_type == StyleType::Numbering {
                numbering_styles.entry(style.style_id.as_str()).or_insert(style);
            }
        }
        for n in &numberings.numberings {
            instances.entry(n.id).or_insert(n.abstract_num_id);
        }
        let mut resolved = std::collections::HashMap::new();
        // Resolve only referenced abstracts, in w:num document order for stable errors.
        for n in &numberings.numberings {
            let root = n.abstract_num_id;
            let mut id = root;
            let mut path = Vec::new();
            let mut visiting = std::collections::HashSet::new();
            let target = loop {
                if let Some(&target) = resolved.get(&id) { break target; }
                if !visiting.insert(id) {
                    return Err(DocumentError::Invalid("cyclic numStyleLink numbering link".into()));
                }
                path.push(id);
                let a = abstracts.get(&id)
                    .ok_or_else(|| DocumentError::Invalid("dangling numbering abstract link".into()))?;
                let Some(link) = &a.num_style_link else { break id };
                let style = numbering_styles.get(link.as_str())
                    .ok_or_else(|| DocumentError::Invalid(format!("dangling numStyleLink: {link}")))?;
                id = if let Some(&target) = style_links.get(link.as_str()) {
                    target
                } else {
                    style.paragraph_property.numbering_property.as_ref()
                        .and_then(|p| p.id.as_ref())
                        .and_then(|n| instances.get(&n.id).copied())
                        .ok_or_else(|| DocumentError::Invalid(format!("dangling numStyleLink/styleLink: {link}")))?
                };
            };
            // Memoise the entire path, so later roots and instances never walk it again.
            for id in path { resolved.insert(id, target); }
        }
        let mut abstract_levels = std::collections::HashMap::<usize, Levels>::new();
        let mut nums = std::collections::HashMap::new();
        let mut counter_sources = std::collections::HashMap::new();
        for n in &numberings.numberings {
            let target = resolved.get(&n.abstract_num_id)
                .ok_or_else(|| DocumentError::Invalid("dangling numbering abstract link".into()))?;
            let abstract_num = abstracts[target];
            let levels = match abstract_levels.entry(abstract_num.id) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::hash_map::Entry::Vacant(entry) => entry.insert(
                    abstract_num.levels.iter().map(|l| Ok((l.level, level(l)?)))
                        .collect::<Result<_, DocumentError>>()?
                ),
            };
            let mut levels = levels.clone();
            for depth in levels.keys() {
                counter_sources.insert((n.id, *depth), CounterOwner::Abstract(abstract_num.id));
            }
            for over in &n.level_overrides {
                counter_sources.insert((n.id, over.level), CounterOwner::Instance(n.id));
                if let Some(l) = &over.override_level {
                    levels.insert(over.level, level(l)?);
                }
                if let Some(start) = over.override_start {
                    if let Some(l) = levels.get_mut(&over.level) {
                        l.start = u32::try_from(start).map_err(|_| DocumentError::Invalid("invalid numbering override".into()))?;
                    }
                }
            }
            nums.insert(n.id, levels);
        }
        for levels in nums.values() {
            for (depth, level) in levels {
                if *depth > 8 || level.start > i32::MAX as u32 || level.restart.is_some_and(|r| r > 9) {
                    return Err(DocumentError::Invalid("invalid numbering value".into()));
                }
            }
        }
        Ok(Self { nums, counter_sources, default_style: None, styles: styles.clone(), counts: Default::default() })
    }
    fn paragraph(&mut self, p: &Paragraph) -> Result<String, DocumentError> {
        let mut prefix = String::new();
        let mut effective = p.property.numbering_property.clone();
        let mut style = p.property.style.as_ref().map(|s| s.val.clone()).or_else(|| self.default_style.clone());
        let mut seen = std::collections::HashSet::new();
        while let Some(id) = style {
            if !seen.insert(id.clone()) { return Err(DocumentError::Invalid("cyclic paragraph style".into())); }
            let Some(def) = self.styles.styles.iter().find(|s| s.style_id == id) else { break };
            if let Some(inherited) = &def.paragraph_property.numbering_property {
                if let Some(prop) = &mut effective {
                    if prop.id.is_none() { prop.id = inherited.id.clone(); }
                    if prop.level.is_none() { prop.level = inherited.level.clone(); }
                } else { effective = Some(inherited.clone()); }
            }
            style = def.based_on.as_ref().and_then(|v| serde_json::to_value(v).ok()).and_then(|v| v.as_str().map(str::to_owned));
        }
        if let Some(prop) = &effective {
            let id = prop.id.as_ref().map(|n| n.id).unwrap_or(0);
            let depth = prop.level.as_ref().map(|n| n.val).unwrap_or(0);
            if depth > 8 { return Err(DocumentError::Invalid("invalid numbering level".into())); }
            if let Some(levels) = self.nums.get(&id).filter(|_| id != 0) {
                if !levels.contains_key(&depth) { return Err(DocumentError::Invalid("undefined numbering level".into())); }
                if let Some(lvl) = levels.get(&depth) {
                    let key = (self.counter_sources[&(id, depth)], depth);
                    let next = match self.counts.get(&key) {
                        Some(value) => value.checked_add(1).ok_or_else(|| DocumentError::Invalid("numbering counter overflow".into()))?,
                        None => lvl.start,
                    };
                    self.counts.insert(key, next);
                    self.counts.retain(|(counter_source, d), _| {
                        self.counter_sources.get(&(id, *d)) != Some(counter_source) || *d <= depth
                            || levels.get(d).is_some_and(|l| {
                                l.restart == Some(0) || l.restart.is_some_and(|r| depth + 1 > r)
                            })
                    });
                    prefix = lvl.text.clone();
                    for d in 0..=8 {
                        if let Some(def) = levels.get(&d) {
                            let value = self.counter_sources.get(&(id, d)).and_then(|counter_source| self.counts.get(&(*counter_source, d))).copied().unwrap_or(def.start);
                            prefix = prefix.replace(
                                &format!("%{}", d + 1),
                                &format_number(value, &def.format),
                            );
                        }
                    }
                    if lvl.format == "none" {
                        prefix.clear();
                    }
                    if !prefix.is_empty() {
                        prefix.push(' ');
                    }
                }
            }
        }
        Ok(format!("{}{}", prefix, paragraph(p)))
    }
    fn table(&mut self, t: &Table) -> Result<String, DocumentError> {
        let mut rows = Vec::new();
        for TableChild::TableRow(row) in &t.rows {
            let mut cells = Vec::new();
            for TableRowChild::TableCell(cell) in &row.cells {
                let mut blocks = Vec::new();
                for child in &cell.children {
                    match child {
                        TableCellContent::Paragraph(p) => blocks.push(self.paragraph(p)?),
                        TableCellContent::Table(t) => blocks.push(self.table(t)?),
                        _ => (),
                    }
                }
                cells.push(blocks.join(" ").replace('|', "\\|").replace('\n', " "));
            }
            rows.push(format!("| {} |", cells.join(" | ")));
        }
        Ok(rows.join("\n"))
    }
}
fn format_number(mut value: u32, format: &str) -> String {
    match format {
        "lowerLetter" | "upperLetter" => {
            let mut text = String::new();
            while value > 0 {
                value -= 1;
                text.insert(0, (b'a' + (value % 26) as u8) as char);
                value /= 26;
            }
            if format == "upperLetter" {
                text.to_uppercase()
            } else {
                text
            }
        }
        "lowerRoman" | "upperRoman" => {
            let mut text = String::new();
            for (n, sym) in [
                (1000, "M"),
                (900, "CM"),
                (500, "D"),
                (400, "CD"),
                (100, "C"),
                (90, "XC"),
                (50, "L"),
                (40, "XL"),
                (10, "X"),
                (9, "IX"),
                (5, "V"),
                (4, "IV"),
                (1, "I"),
            ] {
                while value >= n {
                    text.push_str(sym);
                    value -= n;
                }
            }
            if format == "lowerRoman" {
                text.to_lowercase()
            } else {
                text
            }
        }
        "decimalZero" => format!("{value:02}"),
        _ => value.to_string(),
    }
}
