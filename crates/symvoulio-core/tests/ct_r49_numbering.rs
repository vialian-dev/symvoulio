// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! CARD R49 family (cruel tester): .docx numbering must follow Word. The owner chose to FIX numbering (removal is not approved).
//!
//! Every document is built in the test from WordprocessingML parts (no fixture binaries) and read with the real
//! `documents::extract`; round trips go through the real `OutputPackage` writer.
//! * R49  paragraphs with NO `pStyle` take the style marked `w:default="1"` (and its `basedOn` chain).
//! * numStyleLink / styleLink resolve to the linked abstractNum's levels, also through a paragraph style.
//! * `startOverride` applies once per `w:num` instance (it is the first number, not a restart at every paragraph).
//! * `numId=0` cancels style numbering for that paragraph and does not consume a number.
//! * R37/R43 extract -> export -> re-extract keeps the same prefixes in the same order.
//! * Malformed links (cyclic numStyleLink, link to a missing style) are a visible `Invalid` error, no panic, no hang.
//! * B3 (documents.rs .docx expansion limits): 32 MiB per entry and 64 MiB in total, each exactly at the limit and 1 over.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use symvoulio_core::{documents::DocumentError, extract, output::OutputPackage};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/></Types>"#;
const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
const DOC_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>"#;

// ---------------------------------------------------------------- builders

/// `<w:style>`; `kind` is «paragraph» or «numbering».
fn style(kind: &str, id: &str, default: bool, based: Option<&str>, ppr: &str) -> String {
    let default = if default { r#" w:default="1""# } else { "" };
    let based = based.map(|b| format!(r#"<w:basedOn w:val="{b}"/>"#)).unwrap_or_default();
    format!(r#"<w:style w:type="{kind}"{default} w:styleId="{id}"><w:name w:val="{id}"/>{based}{ppr}</w:style>"#)
}
fn num_pr(id: u32, lvl: Option<u32>) -> String {
    let lvl = lvl.map(|l| format!(r#"<w:ilvl w:val="{l}"/>"#)).unwrap_or_default();
    format!(r#"<w:numPr>{lvl}<w:numId w:val="{id}"/></w:numPr>"#)
}
fn lvl(ilvl: u32, start: u32, fmt: &str, text: &str) -> String {
    format!(r#"<w:lvl w:ilvl="{ilvl}"><w:start w:val="{start}"/><w:numFmt w:val="{fmt}"/><w:lvlText w:val="{text}"/><w:lvlJc w:val="left"/></w:lvl>"#)
}
fn abstract_num(id: u32, inner: &str) -> String {
    format!(r#"<w:abstractNum w:abstractNumId="{id}"><w:multiLevelType w:val="multilevel"/>{inner}</w:abstractNum>"#)
}
fn num(id: u32, abstract_id: u32, overrides: &str) -> String {
    format!(r#"<w:num w:numId="{id}"><w:abstractNumId w:val="{abstract_id}"/>{overrides}</w:num>"#)
}
fn start_override(ilvl: u32, start: u32) -> String {
    format!(r#"<w:lvlOverride w:ilvl="{ilvl}"><w:startOverride w:val="{start}"/></w:lvlOverride>"#)
}
/// A paragraph: optional `pStyle`, optional direct `numPr` (numId, ilvl).
fn p(text: &str, pstyle: Option<&str>, direct: Option<(u32, u32)>) -> String {
    let style = pstyle.map(|s| format!(r#"<w:pStyle w:val="{s}"/>"#)).unwrap_or_default();
    let num = direct.map(|(id, l)| num_pr(id, Some(l))).unwrap_or_default();
    format!("<w:p><w:pPr>{style}{num}</w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>")
}
/// Write a package; `extra` entries are stored, zero filled blobs of the given size (bounded memory).
fn build(dir: &Path, name: &str, styles: &str, numbering: &str, body: &str, extra: &[(&str, u64)]) -> PathBuf {
    let path = dir.join(format!("{name}.docx"));
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    let parts = [
        ("[Content_Types].xml", CONTENT_TYPES.to_owned()),
        ("_rels/.rels", ROOT_RELS.to_owned()),
        ("word/_rels/document.xml.rels", DOC_RELS.to_owned()),
        (
            "word/document.xml",
            format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{W}"><w:body>{body}</w:body></w:document>"#),
        ),
        (
            "word/styles.xml",
            format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{W}">{styles}</w:styles>"#),
        ),
        (
            "word/numbering.xml",
            format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering xmlns:w="{W}">{numbering}</w:numbering>"#),
        ),
    ];
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (part, body) in parts {
        zip.start_file(part, options).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    let chunk = vec![0_u8; 1024 * 1024];
    for (part, size) in extra {
        zip.start_file(*part, options).unwrap();
        let mut left = *size;
        while left > 0 {
            let n = left.min(chunk.len() as u64) as usize;
            zip.write_all(&chunk[..n]).unwrap();
            left -= n as u64;
        }
    }
    zip.finish().unwrap();
    path
}
fn text_of(path: &Path) -> String {
    extract(path).unwrap_or_else(|e| panic!("extract {}: {e}", path.display())).text
}
fn reimport(dir: &Path, text: &str) -> String {
    let primary = dir.join("p.md");
    fs::write(&primary, "# P").unwrap();
    let run = dir.join("run");
    let pkg = OutputPackage::create(&run, &primary, &[]).unwrap();
    pkg.write_revision(1, text).unwrap();
    text_of(&run.join("output/revisions/revision_01.docx"))
}

const DECIMAL: &str = "decimal";

fn simple_numbering() -> String {
    format!("{}{}", abstract_num(0, &lvl(0, 1, DECIMAL, "%1.")), num(1, 0, ""))
}

// ---------------------------------------------------------------- 1. default paragraph style

#[test]
fn test_r49_1_paragraphs_without_pstyle_take_the_numbering_of_the_default_style() {
    let dir = tempfile::tempdir().unwrap();
    let numbering = simple_numbering();
    let numbered = num_pr(1, Some(0));
    let ppr = format!("<w:pPr>{numbered}</w:pPr>");
    let plain = style("paragraph", "Plain", false, None, "");
    let variants = [
        ("default style carries numPr", format!("{}{plain}", style("paragraph", "Body", true, None, &ppr))),
        (
            "default style basedOn Root, Root carries numPr",
            format!(
                "{}{}{plain}",
                style("paragraph", "Root", false, None, &ppr),
                style("paragraph", "Body", true, Some("Root"), "")
            ),
        ),
        (
            "default style basedOn Mid basedOn Root",
            format!(
                "{}{}{}{plain}",
                style("paragraph", "Root", false, None, &ppr),
                style("paragraph", "Mid", false, Some("Root"), ""),
                style("paragraph", "Body", true, Some("Mid"), "")
            ),
        ),
    ];
    let mut bad = vec![];
    for (i, (label, styles)) in variants.iter().enumerate() {
        let implicit = build(
            dir.path(),
            &format!("implicit{i}"),
            styles,
            &numbering,
            &(p("First", None, None) + &p("Second", None, None) + &p("Note", Some("Plain"), None)),
            &[],
        );
        let explicit = build(
            dir.path(),
            &format!("explicit{i}"),
            styles,
            &numbering,
            &(p("First", Some("Body"), None) + &p("Second", Some("Body"), None) + &p("Note", Some("Plain"), None)),
            &[],
        );
        let (implicit, explicit) = (text_of(&implicit), text_of(&explicit));
        let want = "1. First\n\n2. Second\n\nNote";
        if explicit != want {
            bad.push(format!("{label}: control, explicit pStyle=Body expected {want:?}, got {explicit:?}"));
        }
        if implicit != explicit {
            bad.push(format!("{label}: no pStyle must equal explicit default style: {implicit:?} vs {explicit:?}"));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

// ---------------------------------------------------------------- 2. numStyleLink / styleLink

/// abstract 0 only carries numStyleLink=LinkNum; the numbering style LinkNum points at num 2 whose abstract 1 carries
/// styleLink=LinkNum and the levels.
fn linked_numbering() -> (String, String) {
    let numbering = format!(
        "{}{}{}{}",
        format_args!(r#"<w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="multilevel"/><w:numStyleLink w:val="LinkNum"/></w:abstractNum>"#),
        format_args!(
            r#"<w:abstractNum w:abstractNumId="1"><w:multiLevelType w:val="multilevel"/><w:styleLink w:val="LinkNum"/>{}{}</w:abstractNum>"#,
            lvl(0, 1, DECIMAL, "%1)"),
            lvl(1, 1, "lowerLetter", "%2.")
        ),
        num(1, 0, ""),
        num(2, 1, ""),
    );
    let styles = style("numbering", "LinkNum", false, None, &format!("<w:pPr>{}</w:pPr>", num_pr(2, None)));
    (numbering, styles)
}

#[test]
fn test_r49_2_numstylelink_resolves_to_the_linked_levels_directly_and_through_a_paragraph_style() {
    let dir = tempfile::tempdir().unwrap();
    let (numbering, link_style) = linked_numbering();
    let styles = format!(
        "{link_style}{}",
        style("paragraph", "LinkedList", false, None, &format!("<w:pPr>{}</w:pPr>", num_pr(1, Some(0))))
    );
    let direct = build(
        dir.path(),
        "direct",
        &styles,
        &numbering,
        &(p("First", None, Some((1, 0))) + &p("Sub", None, Some((1, 1))) + &p("Second", None, Some((1, 0)))),
        &[],
    );
    let want = "1) First\n\na. Sub\n\n2) Second";
    assert_eq!(text_of(&direct), want, "numId=1 -> numStyleLink -> LinkNum levels (%1) then %2.)");
    let via_style = build(
        dir.path(),
        "via-style",
        &styles,
        &numbering,
        &(p("First", Some("LinkedList"), None) + &p("Sub", Some("LinkedList"), Some((1, 1))) + &p("Second", Some("LinkedList"), None)),
        &[],
    );
    assert_eq!(text_of(&via_style), want, "paragraph style LinkedList (numPr numId=1) must give the same text");
}

// ---------------------------------------------------------------- 3. startOverride once per num instance

#[test]
fn test_r49_3_start_override_applies_once_per_num_instance() {
    let dir = tempfile::tempdir().unwrap();
    let numbering = format!(
        "{}{}{}",
        abstract_num(0, &lvl(0, 1, DECIMAL, "%1.")),
        num(1, 0, ""),
        num(2, 0, &start_override(0, 5))
    );
    let body = [("a1", 1), ("a2", 1), ("a3", 1), ("b1", 2), ("b2", 2), ("a4", 1)]
        .iter()
        .map(|(t, id)| p(t, None, Some((*id, 0))))
        .collect::<String>();
    let path = build(dir.path(), "override", "", &numbering, &body, &[]);
    assert_eq!(
        text_of(&path),
        "1. a1\n\n2. a2\n\n3. a3\n\n5. b1\n\n6. b2\n\n4. a4",
        "list A 1,2,3; list B starts at 5 once (5,6, not 5,5); list A continues with 4"
    );
}

/// ECMA-376 Part 1, 17.9 (quoted from memory, not re-checked against the PDF here): 17.9.15 `num` says a numbering
/// definition instance "specifies a unique instance of numbering information that can be referenced by zero or more
/// paragraphs", and 17.9.1 `abstractNum` holds the "abstract numbering definition" (the shared level definitions)
/// that instances reference. Only `lvlOverride`/`startOverride` (17.9.8, 17.9.26) give an instance its own start.
/// The spec has no rule that a second `num` restarts the count, and Word continues one count across instances of
/// the same abstractNum that have no override, which is what this test expects.
/// Orchestrator check 22:10Z against ISO/IEC 29500-1:2008 as reproduced in the Open XML SDK 3.0.1 reference
/// (learn.microsoft.com, NumberingInstance and StartOverrideNumberingValue, updated 2024-01-12): `num` text matches
/// the quote above; the startOverride example says «Subsequent numbered paragraphs with this abstract numbering
/// definition and at this level would have their numbering symbols incremented», i.e. the count is per abstract
/// definition and level; startOverride applies «when this level initially starts ... as well as whenever it is
/// restarted via ... lvlRestart (§17.9.11)». (The SDK reference numbers lvlOverride §17.9.9, abstractNumId §17.9.2.)
#[test]
fn test_r49_3b_two_nums_on_one_abstract_without_override_share_one_count() {
    let dir = tempfile::tempdir().unwrap();
    let numbering = format!("{}{}{}", abstract_num(0, &lvl(0, 1, DECIMAL, "%1.")), num(1, 0, ""), num(2, 0, ""));
    let body = [("a1", 1), ("a2", 1), ("b1", 2), ("b2", 2), ("a3", 1)]
        .iter()
        .map(|(t, id)| p(t, None, Some((*id, 0))))
        .collect::<String>();
    let path = build(dir.path(), "shared", "", &numbering, &body, &[]);
    assert_eq!(
        text_of(&path),
        "1. a1\n\n2. a2\n\n3. b1\n\n4. b2\n\n5. a3",
        "order A A B B A on one abstractNum, no override: 1,2,3,4,5"
    );
}

// ---------------------------------------------------------------- 4. numId=0 cancels style numbering

#[test]
fn test_r49_4_numid_zero_cancels_style_numbering_and_does_not_consume_a_number() {
    let dir = tempfile::tempdir().unwrap();
    let numbering = simple_numbering();
    let styles = format!(
        "{}{}",
        style("paragraph", "ListX", false, None, &format!("<w:pPr>{}</w:pPr>", num_pr(1, Some(0)))),
        style("paragraph", "ListY", false, Some("ListX"), "")
    );
    let cancel = |text: &str, pstyle: &str| {
        format!(
            r#"<w:p><w:pPr><w:pStyle w:val="{pstyle}"/>{}</w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#,
            num_pr(0, None)
        )
    };
    let body = p("one", Some("ListX"), None)
        + &cancel("plain", "ListX")
        + &p("two", Some("ListX"), None)
        + &cancel("plain too", "ListY")
        + &p("three", Some("ListY"), None);
    let path = build(dir.path(), "cancel", &styles, &numbering, &body, &[]);
    assert_eq!(
        text_of(&path),
        "1. one\n\nplain\n\n2. two\n\nplain too\n\n3. three",
        "numId=0 paragraphs carry no prefix (also through basedOn) and the count continues 1,2,3"
    );
}

// ---------------------------------------------------------------- 5. export -> reimport

fn assert_round_trip(dir: &Path, path: &Path, want: &str) {
    let first = text_of(path);
    assert_eq!(first, want, "first extraction");
    let again = reimport(dir, &first);
    assert_eq!(again, first, "extract -> product writer -> extract must keep every prefix and the order");
}

#[test]
fn test_r49_5a_round_trip_explicit_multilevel_and_restart_numbering() {
    let dir = tempfile::tempdir().unwrap();
    let numbering = format!(
        "{}{}{}",
        abstract_num(0, &(lvl(0, 1, DECIMAL, "%1.") + &lvl(1, 1, DECIMAL, "%1.%2"))),
        num(1, 0, ""),
        num(2, 0, &start_override(0, 7))
    );
    let styles = style("paragraph", "Plain", false, None, "");
    let body = p("Intro", Some("Plain"), None)
        + &p("A one", None, Some((1, 0)))
        + &p("A two", None, Some((1, 0)))
        + &p("A two sub", None, Some((1, 1)))
        + &p("Between", Some("Plain"), None)
        + &p("B one", None, Some((2, 0)))
        + &p("B two", None, Some((2, 0)))
        + &p("Outro", Some("Plain"), None);
    let path = build(dir.path(), "explicit", &styles, &numbering, &body, &[]);
    assert_round_trip(
        dir.path(),
        &path,
        "Intro\n\n1. A one\n\n2. A two\n\n2.1 A two sub\n\nBetween\n\n7. B one\n\n8. B two\n\nOutro",
    );
}

#[test]
fn test_r49_5b_round_trip_implicit_default_style_numbering() {
    let dir = tempfile::tempdir().unwrap();
    let numbering = simple_numbering();
    let styles = format!(
        "{}{}",
        style("paragraph", "Body", true, None, &format!("<w:pPr>{}</w:pPr>", num_pr(1, Some(0)))),
        style("paragraph", "Plain", false, None, "")
    );
    let body = p("Lead", Some("Plain"), None)
        + &p("D one", None, None)
        + &p("D two", None, None)
        + &p("Tail", Some("Plain"), None);
    let path = build(dir.path(), "implicit", &styles, &numbering, &body, &[]);
    assert_round_trip(dir.path(), &path, "Lead\n\n1. D one\n\n2. D two\n\nTail");
}

// ---------------------------------------------------------------- 6. malformed links stay rejected

/// Runs `extract` on a thread: a panic or a hang is reported as a failure instead of taking the harness down.
fn bounded_extract(path: PathBuf) -> Result<String, String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(extract(&path).map(|e| e.text));
    });
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(Ok(text)) => Ok(text),
        Ok(Err(DocumentError::Invalid(why))) => Err(why),
        Ok(Err(other)) => panic!("expected DocumentError::Invalid, got {other:?}"),
        Err(mpsc::RecvTimeoutError::Timeout) => panic!("extract did not return within 20 s (hang)"),
        Err(mpsc::RecvTimeoutError::Disconnected) => panic!("extract panicked instead of returning an error"),
    }
}

#[test]
fn test_r49_6_cyclic_and_dangling_numstylelink_are_a_visible_invalid_error() {
    let dir = tempfile::tempdir().unwrap();
    let link = |style_id: &str| {
        format!(
            r#"<w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="multilevel"/><w:numStyleLink w:val="{style_id}"/></w:abstractNum>{}"#,
            num(1, 0, "")
        )
    };
    let body = p("First", None, Some((1, 0)));
    // LinkNum -> numId 1 -> abstract 0 -> numStyleLink LinkNum: a cycle.
    let cyclic_styles = style("numbering", "LinkNum", false, None, &format!("<w:pPr>{}</w:pPr>", num_pr(1, None)));
    let cyclic = build(dir.path(), "cyclic", &cyclic_styles, &link("LinkNum"), &body, &[]);
    let dangling = build(dir.path(), "dangling", "", &link("NoSuchStyle"), &body, &[]);
    let mut bad = vec![];
    for (label, path) in [("cyclic numStyleLink", cyclic), ("numStyleLink to a missing style", dangling)] {
        match bounded_extract(path) {
            Err(why) if why.contains("undefined numbering level") => {
                bad.push(format!("{label}: rejected by accident, not for the link: {why:?}"))
            }
            Err(why) if !why.trim().is_empty() => (),
            Err(why) => bad.push(format!("{label}: Invalid error with an empty explanation {why:?}")),
            Ok(text) => bad.push(format!("{label}: must be rejected as Invalid, extracted {text:?}")),
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
    // Control: the same builder with a well formed link extracts.
    let (numbering, link_style) = linked_numbering();
    let good = build(dir.path(), "good", &link_style, &numbering, &p("First", None, Some((1, 0))), &[]);
    assert_eq!(bounded_extract(good).as_deref(), Ok("1) First"), "control: a well formed numStyleLink must extract");
}

// ---------------------------------------------------------------- 7. review findings (R49c)

/// A chain of `links` abstractNums: abstract k carries numStyleLink=Link_k, abstract k+1 carries styleLink=Link_k
/// (and numStyleLink=Link_k+1, except the last, which carries the levels). `nums` w:num all point at the head.
fn chain_numbering(links: usize, nums: usize) -> (String, String) {
    let mut numbering = String::new();
    let mut styles = String::new();
    for k in 0..=links {
        let style_link = if k > 0 { format!(r#"<w:styleLink w:val="Link{}"/>"#, k - 1) } else { String::new() };
        let tail = if k < links {
            format!(r#"<w:numStyleLink w:val="Link{k}"/>"#)
        } else {
            lvl(0, 1, DECIMAL, "%1)")
        };
        numbering.push_str(&format!(
            r#"<w:abstractNum w:abstractNumId="{k}"><w:multiLevelType w:val="multilevel"/>{style_link}{tail}</w:abstractNum>"#
        ));
        if k < links {
            styles.push_str(&style("numbering", &format!("Link{k}"), false, None, ""));
        }
    }
    for n in 1..=nums {
        numbering.push_str(&num(n as u32, 0, ""));
    }
    (numbering, styles)
}

#[test]
fn test_r49_7a_long_acyclic_numstylelink_chain_with_many_nums_returns_in_bounded_time() {
    let dir = tempfile::tempdir().unwrap();
    let (numbering, styles) = chain_numbering(5000, 5000);
    let body = p("First", None, Some((1, 0)));
    let path = build(dir.path(), "chain-big", &styles, &numbering, &body, &[]);
    let size = fs::metadata(&path).unwrap().len();
    assert!(size < 4 * 1024 * 1024, "crafted package must stay small to be a realistic attack, is {size} bytes");
    let started = Instant::now();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(extract(&path).map(|e| e.text));
    });
    // 10 s: a linear pass over a few MB takes well under a second here; the quadratic walk per w:num needs ~10^11 steps.
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(_)) | Ok(Err(DocumentError::Invalid(_))) => eprintln!("returned after {:?}", started.elapsed()),
        Ok(Err(other)) => panic!("expected Ok or Invalid, got {other:?}"),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!("extract did not return within 10 s on a {size} byte package (5000 chain links x 5000 w:num)")
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => panic!("extract panicked"),
    }
}

#[test]
fn test_r49_7b_short_numstylelink_chain_gives_the_right_numbering() {
    let dir = tempfile::tempdir().unwrap();
    let (numbering, styles) = chain_numbering(50, 3);
    let body = p("First", None, Some((1, 0))) + &p("Second", None, Some((2, 0)));
    let path = build(dir.path(), "chain-small", &styles, &numbering, &body, &[]);
    assert_eq!(bounded_extract(path).as_deref(), Ok("1) First\n\n2) Second"), "50 links resolve to the final levels");
}

/// ISO/IEC 29500-1 (ECMA-376 Part 1) 17.7.4.17 `style`, attribute `default`: when more than one style of a type is
/// marked default, "the last" one is used. Orchestrator check 23:33Z: ECMA-376 Part 1 17.7.4.17 as copied at webapp.docx4java.org/OnlineDemo/ecma376/WordML/style_1.html reads «If this attribute is specified by multiple styles, then the last instance of a style with this property shall be used.»
/// Word (MS-OI29500 2.1.242, Part 1 Section 17.7.4.17, updated 2024-04-16) lists no deviation for `default`; its three notes are: «Word ignores any child elements of a style element with id "NoList", "DefaultParagraphFont", or "TableNormal".», «Word ignores the value of the styleId attribute if it exceeds 253 characters in length.», «Word does not open a document containing more than 4,079 styles.» So Word follows the standard here.
#[test]
fn test_r49_7c_with_two_default_paragraph_styles_the_last_one_applies() {
    let dir = tempfile::tempdir().unwrap();
    let numbering = format!(
        "{}{}{}{}",
        abstract_num(0, &lvl(0, 1, DECIMAL, "%1.")),
        abstract_num(1, &lvl(0, 1, "lowerLetter", "%1)")),
        num(1, 0, ""),
        num(2, 1, "")
    );
    let styles = style("paragraph", "FirstDefault", true, None, &format!("<w:pPr>{}</w:pPr>", num_pr(1, Some(0))))
        + &style("paragraph", "LastDefault", true, None, &format!("<w:pPr>{}</w:pPr>", num_pr(2, Some(0))));
    let body = p("First", None, None) + &p("Second", None, None);
    let path = build(dir.path(), "two-defaults", &styles, &numbering, &body, &[]);
    assert_eq!(text_of(&path), "a) First\n\nb) Second", "the LAST default style (lowerLetter) applies, not the first");
}

// ---------------------------------------------------------------- 8. unused / repeated link defects (R49d)

fn link_only_abstract(id: u32, style_link: Option<&str>, num_style_link: &str) -> String {
    let style_link = style_link.map(|s| format!(r#"<w:styleLink w:val="{s}"/>"#)).unwrap_or_default();
    format!(
        r#"<w:abstractNum w:abstractNumId="{id}"><w:multiLevelType w:val="multilevel"/>{style_link}<w:numStyleLink w:val="{num_style_link}"/></w:abstractNum>"#
    )
}

#[test]
fn test_r49_8a_unused_abstractnum_with_a_broken_link_does_not_fail_the_import() {
    let dir = tempfile::tempdir().unwrap();
    let body = p("First", None, Some((1, 0))) + &p("Second", None, Some((1, 0)));
    let used = abstract_num(0, &lvl(0, 1, DECIMAL, "%1.")) + &num(1, 0, "");
    // No w:num points at abstract 5 (dangling) or at 6 / 7 (a cycle through two numbering styles).
    let dangling = used.clone() + &link_only_abstract(5, None, "Gone");
    let cyclic = used + &link_only_abstract(6, Some("CycB"), "CycA") + &link_only_abstract(7, Some("CycA"), "CycB");
    let cyclic_styles = style("numbering", "CycA", false, None, "") + &style("numbering", "CycB", false, None, "");
    let mut bad = vec![];
    for (label, styles, numbering) in [("dangling", String::new(), dangling), ("cyclic", cyclic_styles, cyclic)] {
        let path = build(dir.path(), label, &styles, &numbering, &body, &[]);
        match bounded_extract(path).as_deref() {
            Ok("1. First\n\n2. Second") => (),
            other => bad.push(format!("unused {label} abstractNum: expected the used list numbered 1. 2., got {other:?}")),
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn test_r49_8b_two_different_link_defects_give_the_same_error_every_time() {
    let dir = tempfile::tempdir().unwrap();
    let numbering = link_only_abstract(0, None, "GoneA") + &link_only_abstract(1, None, "GoneB") + &num(1, 0, "") + &num(2, 1, "");
    let body = p("First", None, Some((1, 0))) + &p("Second", None, Some((2, 0)));
    let path = build(dir.path(), "two-defects", "", &numbering, &body, &[]);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..20 {
        seen.insert(format!("{:?}", bounded_extract(path.clone())));
    }
    assert_eq!(seen.len(), 1, "20 extractions of one document must give one result, got {seen:#?}");
}

// ---------------------------------------------------------------- B3: .docx expansion limits (documents.rs:92)

const MIB: u64 = 1024 * 1024;

/// Size of the package parts `build` always writes (everything except `extra`), measured through the zip itself.
fn base_parts_size(dir: &Path) -> u64 {
    let path = build(dir, "measure", "", &simple_numbering(), &p("x", None, Some((1, 0))), &[]);
    let mut archive = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    (0..archive.len()).map(|i| archive.by_index(i).unwrap().size()).sum()
}
fn limit_case(dir: &Path, name: &str, extra: &[(&str, u64)]) -> Result<String, DocumentError> {
    let path = build(dir, name, "", &simple_numbering(), &p("x", None, Some((1, 0))), extra);
    let result = extract(&path).map(|e| e.text);
    fs::remove_file(path).unwrap();
    result
}
fn assert_expansion_error(label: &str, result: Result<String, DocumentError>) {
    match result {
        Err(DocumentError::Invalid(why)) => {
            assert!(why.contains("expansion"), "{label}: rejected, but not by the expansion limit: {why}")
        }
        other => panic!("{label}: must be rejected by the expansion limit, got {other:?}"),
    }
}

#[test]
fn test_b3_single_entry_over_32_mib_alone_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    // total is far below 64 MiB: only the per-entry limit can reject (kills `||` -> `&&`, `>` -> `==`, `*` -> `+`).
    assert_expansion_error("one entry of 32 MiB + 1", limit_case(dir.path(), "entry-over", &[("customXml/blob.bin", 32 * MIB + 1)]));
}

#[test]
fn test_b3_total_just_over_64_mib_with_every_entry_under_32_mib_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let base = base_parts_size(dir.path());
    // 32 MiB (at the entry limit) + (32 MiB - base + 1): every entry <= 32 MiB, total = 64 MiB + 1 exactly.
    assert_expansion_error(
        "total 64 MiB + 1",
        limit_case(dir.path(), "total-over", &[("customXml/a.bin", 32 * MIB), ("customXml/b.bin", 32 * MIB - base + 1)]),
    );
    // Many small entries: same total, no entry anywhere near the per-entry limit.
    assert_expansion_error(
        "total 64 MiB + 1 over four 16 MiB entries",
        limit_case(
            dir.path(),
            "total-over-small",
            &[
                ("customXml/a.bin", 16 * MIB),
                ("customXml/b.bin", 16 * MIB),
                ("customXml/c.bin", 16 * MIB),
                ("customXml/d.bin", 16 * MIB - base + 1),
            ],
        ),
    );
}

#[test]
fn test_b3_exactly_at_each_limit_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let base = base_parts_size(dir.path());
    let ok = |label: &str, result: Result<String, DocumentError>| {
        assert_eq!(result.as_deref().map_err(|e| e.to_string()), Ok("1. x"), "{label}: exactly at the limit must be accepted");
    };
    ok("one entry of exactly 32 MiB", limit_case(dir.path(), "entry-at", &[("customXml/blob.bin", 32 * MIB)]));
    ok(
        "total of exactly 64 MiB (32 MiB entry + the rest)",
        limit_case(dir.path(), "total-at", &[("customXml/a.bin", 32 * MIB), ("customXml/b.bin", 32 * MIB - base)]),
    );
}
