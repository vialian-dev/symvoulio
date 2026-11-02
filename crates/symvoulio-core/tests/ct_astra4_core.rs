// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! CARD AST-F (cruel tester): the external review's pass-4 backend failures R42, R43, R44. Expectations come from the card.
//!
//! Rules tested (the card leaves them open, so they are stated here):
//! * R42  faults AFTER the state commit (publication of `output/final`, of `output/audit`, staging cleanup). After
//!   `decide` returns: `Ok` must carry the committed decision and every artifact must agree with `run_state.json`;
//!   an `Err` while `run_state.json` holds the decision is allowed only when the error text says «committed»
//!   (explicitly visible); an `Err` with nothing committed must leave every artifact agreeing. After the fault is
//!   cleared and `export_report` succeeds (if it refuses, `load` repairs first and the export must then succeed),
//!   `run_state.json`, `final/human_decision.json` and `audit/run_manifest.json` must hold the same decision and state.
//!   Every test proves its fault fired and fails otherwise (L55).
//! * R43  numbering through a paragraph STYLE (own numPr, basedOn chain) extracts like direct numbering; a list
//!   starting at 0 extracts «0. First», «1. Second»; both survive export -> reimport.
//! * R44  out-of-range numbering values (start 4294967295, 4294967296, -1, «abc»; a level beyond the defined levels
//!   (5) and beyond the OOXML maximum (9)) make `extract` return Err, in-process, with no unwinding.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use symvoulio_core::{
    engine::{decide, export_report, load, start, RunConfig},
    extract,
    output::OutputPackage,
    provider::{CallError, CallOutcome, CallRequest, CancelToken, ProviderClient, StreamLine},
    DecisionKind, Policy, Provider, Role, RunState,
};

static LOCK: Mutex<()> = Mutex::new(());

// ---------------------------------------------------------------- scripted providers

type Handler = Arc<dyn Fn(&CallRequest, &mut dyn FnMut(StreamLine)) -> Result<CallOutcome, CallError> + Send + Sync>;
struct Fake {
    provider: Provider,
    handler: Handler,
}
impl ProviderClient for Fake {
    fn provider(&self) -> Provider {
        self.provider
    }
    fn call(&self, req: &CallRequest, sink: &mut dyn FnMut(StreamLine), _cancel: &CancelToken) -> Result<CallOutcome, CallError> {
        (self.handler)(req, sink)
    }
}
fn outcome(structured: serde_json::Value) -> CallOutcome {
    CallOutcome {
        structured,
        model: None,
        session_id: None,
        usage: None,
        stdout_lines: vec![],
        stderr: String::new(),
        duration_s: 0.1,
        exit_code: Some(0),
    }
}

const DECIDER: &str = "Zelda-Approver-4471";
const NOTE: &str = "Private-Note-8812";

/// A one-round run that completes (no findings, no questions).
fn completed_run(root: &Path) -> PathBuf {
    let primary = root.join("doc.md");
    fs::write(&primary, "# Doc\n\noriginal").unwrap();
    let cfg = RunConfig {
        run_id: "ct4-run".into(),
        name: "ct4".into(),
        run_dir: root.join("run"),
        author: Provider::Claude,
        objective: "Improve".into(),
        criteria: vec!["c".into()],
        policy: Policy { max_rounds: 1, min_rounds: 1, new_content_limit_pct: 100.0, ..Policy::default() },
        primary,
        supporting: vec![],
        call_timeout: Duration::from_secs(5),
        author_model: None,
        reviewer_model: None,
    };
    let run = cfg.run_dir.clone();
    let h: Handler = Arc::new(|req, _| {
        Ok(outcome(match req.role {
            Role::Author => serde_json::json!({
                "revised_document": format!("# Doc\n\nrevision {}", req.round),
                "changes_made": ["c"], "responses": [], "new_claims": [], "human_decisions_required": []
            }),
            Role::Reviewer => serde_json::json!({
                "summary": "s", "issues": [],
                "criteria_results": [{"criterion": "c", "met": true, "note": "n"}]
            }),
        }))
    });
    let clients: (Arc<dyn ProviderClient>, Arc<dyn ProviderClient>) = (
        Arc::new(Fake { provider: Provider::Claude, handler: h.clone() }),
        Arc::new(Fake { provider: Provider::Codex, handler: h }),
    );
    let s = start(cfg, clients, Arc::default(), |_| {}).unwrap();
    assert_eq!(s.state, RunState::Completed, "setup: {:?}", s.last_error);
    run
}

// ---------------------------------------------------------------- VAL-R42

#[derive(Clone, Copy, Debug)]
enum Fault {
    FinalPublish,
    AuditPublish,
    Cleanup,
}
impl Fault {
    fn dest(self, run: &Path) -> PathBuf {
        match self {
            Fault::FinalPublish => run.join("output/final/human_decision.json"),
            Fault::AuditPublish => run.join("output/audit/run_manifest.json"),
            Fault::Cleanup => run.join(".publication"),
        }
    }
}

/// Fires when the approval shows up in the STAGED decision, i.e. before the state commit: whatever it does is therefore
/// in the window AFTER the commit by the time the engine reaches the publication. Final/Audit: the destination file is
/// moved aside (outside output/) and replaced by a directory, so the rename onto it fails. Cleanup: files keep
/// arriving in the stage's unpublished `output/revisions`, so the stage cannot be removed.
fn inject(run: PathBuf, fault: Fault, done: Arc<AtomicBool>, count: Arc<AtomicUsize>, aside: PathBuf) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let staged = run.join(".publication/output/final/human_decision.json");
        let t0 = Instant::now();
        while !done.load(Ordering::SeqCst) && t0.elapsed() < Duration::from_secs(30) {
            if fs::read_to_string(&staged).is_ok_and(|t| t.contains(DECIDER)) {
                match fault {
                    Fault::FinalPublish | Fault::AuditPublish => {
                        let dest = fault.dest(&run);
                        fs::create_dir_all(&aside).unwrap();
                        if fs::rename(&dest, aside.join(dest.file_name().unwrap())).is_ok() && fs::create_dir(&dest).is_ok() {
                            count.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                    Fault::Cleanup => {
                        let late = run.join(".publication/output/revisions");
                        for n in 0..50_000 {
                            if fs::write(late.join(format!("late-{n}")), b"x").is_err() {
                                break;
                            }
                            count.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                }
                return;
            }
            thread::yield_now();
        }
    })
}

fn state_json(run: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(run.join("run_state.json")).unwrap()).unwrap()
}
/// `run_state.json`, `final/human_decision.json` and `audit/run_manifest.json` hold the same decision and state.
fn coherence(run: &Path) -> Result<(), String> {
    let st = state_json(run);
    let want = st["snapshot"]["decision"].clone();
    let read = |rel: &str| -> Result<serde_json::Value, String> {
        let bytes = fs::read(run.join(rel)).map_err(|e| format!("{rel}: {e}"))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("{rel}: {e}"))
    };
    let fin = read("output/final/human_decision.json")?;
    let man = read("output/audit/run_manifest.json")?;
    let mut bad = vec![];
    if fin != want {
        bad.push(format!("final/human_decision.json = {fin}; run_state.json = {want}"));
    }
    if man["human_decision"] != want {
        bad.push(format!("audit/run_manifest.json human_decision = {}; run_state.json = {want}", man["human_decision"]));
    }
    if man["state"] != st["snapshot"]["state"] {
        bad.push(format!("audit/run_manifest.json state = {}; run_state.json = {}", man["state"], st["snapshot"]["state"]));
    }
    if bad.is_empty() {
        Ok(())
    } else {
        Err(bad.join("; "))
    }
}

fn r42(fault: Fault) {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut notes = vec![];
    let mut fired_once = false;
    for attempt in 1..=25 {
        let dir = tempfile::tempdir().unwrap();
        let run = completed_run(dir.path());
        let aside = dir.path().join("aside");
        let (done, injected) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicUsize::new(0)));
        let watcher = inject(run.clone(), fault, done.clone(), injected.clone(), aside.clone());
        let result = decide(&run, DecisionKind::Approved, DECIDER, NOTE);
        done.store(true, Ordering::SeqCst);
        watcher.join().unwrap();
        let injections = injected.load(Ordering::SeqCst);
        let dest = fault.dest(&run);
        let fired = match fault {
            Fault::Cleanup => injections > 0 && (result.is_err() || dest.exists()),
            _ => injections > 0 && dest.is_dir(),
        };
        if !fired {
            notes.push(format!("attempt {attempt}: injections={injections}, decide ok={}", result.is_ok()));
            continue;
        }
        fired_once = true;
        if matches!(fault, Fault::Cleanup) {
            // AST-F 4: a stage left behind holds no private decision data (or it is removed).
            let mut left = vec![dest.clone()];
            let mut leaked = vec![];
            while let Some(p) = left.pop() {
                if p.is_dir() {
                    left.extend(fs::read_dir(&p).unwrap().flatten().map(|e| e.path()));
                } else if let Ok(bytes) = fs::read(&p) {
                    let text = String::from_utf8_lossy(&bytes);
                    if text.contains(DECIDER) || text.contains(NOTE) {
                        leaked.push(p.display().to_string());
                    }
                }
            }
            assert!(leaked.is_empty(), "Cleanup: the stage left behind holds the private decision (name or note): {leaked:?}");
        }
        let st = state_json(&run);
        let committed =!st["snapshot"]["decision"].is_null();
        // 1. what decide reported
        match &result {
            Ok(snap) => {
                assert_eq!(
                    serde_json::to_value(&snap.decision).unwrap(),
                    st["snapshot"]["decision"],
                    "{fault:?}: decide returned Ok but its snapshot does not hold the committed decision"
                );
                if let Err(e) = coherence(&run) {
                    panic!("{fault:?}: decide returned Ok and the artifacts contradict each other: {e}");
                }
            }
            Err(e) => {
                let msg = format!("{e} {e:?}").to_lowercase();
                if committed {
                    assert!(
                        msg.contains("committed"),
                        "{fault:?}: decide returned a plain error ({e}) while run_state.json holds the decision: the committed approval is not visible in the result"
                    );
                } else if let Err(why) = coherence(&run) {
                    panic!("{fault:?}: nothing was committed but the artifacts contradict each other: {why}");
                }
            }
        }
        // 2. clear the fault, export, and compare again
        if matches!(fault, Fault::FinalPublish | Fault::AuditPublish) {
            fs::remove_dir(&dest).unwrap();
            fs::rename(aside.join(dest.file_name().unwrap()), &dest).unwrap();
        }
        if export_report(&run).is_err() {
            load(&run).unwrap();
            export_report(&run).unwrap_or_else(|e| panic!("{fault:?}: export_report still refuses after the repair: {e}"));
        }
        if let Err(e) = coherence(&run) {
            panic!("{fault:?}: contradictory artifacts after a successful export (committed={committed}, decide ok={}): {e}", result.is_ok());
        }
        break;
    }
    assert!(fired_once, "L55: the {fault:?} fault never fired in 25 attempts, nothing was tested: {notes:#?}");
}

#[test]
fn test_dod1_val_r42_fault_in_final_publication_after_state_commit_leaves_one_coherent_decision() {
    r42(Fault::FinalPublish);
}
#[test]
fn test_dod1_val_r42_fault_in_audit_publication_after_state_commit_leaves_one_coherent_decision() {
    r42(Fault::AuditPublish);
}
#[test]
fn test_dod1_val_r42_fault_in_staging_cleanup_after_state_commit_leaves_one_coherent_decision() {
    r42(Fault::Cleanup);
}

// ---------------------------------------------------------------- DOCX fixtures (hand-built XML)

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/></Types>"#;
const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
const DOC_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>"#;

/// ListA carries the numPr itself; ListB is based on ListA and ListC on ListB (inheritance only); ListD carries a numPr
/// with a numId and no ilvl.
fn styles_xml() -> String {
    let style = |id: &str, based: &str, ppr: &str| {
        format!(r#"<w:style w:type="paragraph" w:styleId="{id}"><w:name w:val="{id}"/>{based}{ppr}</w:style>"#)
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{W}">{}{}{}{}</w:styles>"#,
        style("ListA", "", r#"<w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr>"#),
        style("ListB", r#"<w:basedOn w:val="ListA"/>"#, ""),
        style("ListC", r#"<w:basedOn w:val="ListB"/>"#, ""),
        style("ListD", "", r#"<w:pPr><w:numPr><w:numId w:val="1"/></w:numPr></w:pPr>"#),
    )
}
/// numId 1 -> abstract list with two defined levels (0: «%1.» from `start`, 1: «%1.%2» from 1).
fn numbering_xml(start: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering xmlns:w="{W}"><w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="multilevel"/><w:lvl w:ilvl="0"><w:start w:val="{start}"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/></w:lvl><w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1.%2"/><w:lvlJc w:val="left"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num></w:numbering>"#
    )
}
#[derive(Clone, Copy)]
enum Num {
    Direct(&'static str),
    Style(&'static str),
}
fn document_xml(paras: &[(&str, Num)]) -> String {
    let body: String = paras
        .iter()
        .map(|(text, num)| {
            let ppr = match num {
                Num::Direct(lvl) => format!(r#"<w:pPr><w:numPr><w:ilvl w:val="{lvl}"/><w:numId w:val="1"/></w:numPr></w:pPr>"#),
                Num::Style(id) => format!(r#"<w:pPr><w:pStyle w:val="{id}"/></w:pPr>"#),
            };
            format!("<w:p>{ppr}<w:r><w:t>{text}</w:t></w:r></w:p>")
        })
        .collect();
    format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{W}"><w:body>{body}</w:body></w:document>"#)
}
fn fixture(dir: &Path, name: &str, start: &str, paras: &[(&str, Num)]) -> PathBuf {
    let path = dir.join(format!("{name}.docx"));
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    let parts = [
        ("[Content_Types].xml", CONTENT_TYPES.to_owned()),
        ("_rels/.rels", ROOT_RELS.to_owned()),
        ("word/_rels/document.xml.rels", DOC_RELS.to_owned()),
        ("word/document.xml", document_xml(paras)),
        ("word/styles.xml", styles_xml()),
        ("word/numbering.xml", numbering_xml(start)),
    ];
    for (part, body) in parts {
        zip.start_file(part, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    path
}
fn has_identified(text: &str, id: &str, body: &str) -> bool {
    let re = regex::Regex::new(&format!(r"(?m)(?:^|\s){}[.)]?[ \t]+{}[ \t]*$", regex::escape(id.trim_end_matches('.')), regex::escape(body))).unwrap();
    re.is_match(text)
}
fn two(a: Num, b: Num) -> Vec<(&'static str, Num)> {
    vec![("First", a), ("Second", b)]
}
fn reimport(dir: &Path, text: &str) -> String {
    let primary = dir.join("p.md");
    fs::write(&primary, "# P").unwrap();
    let run = dir.join("run");
    let pkg = OutputPackage::create(&run, &primary, &[]).unwrap();
    pkg.write_revision(1, text).unwrap();
    extract(&run.join("output/revisions/revision_01.docx")).unwrap().text
}

// ---------------------------------------------------------------- VAL-R43

#[test]
fn test_dod1_val_r43_style_numbering_and_basedon_inheritance_extract_like_direct_numbering() {
    let dir = tempfile::tempdir().unwrap();
    let variants: Vec<(&str, Vec<(&str, Num)>)> = vec![
        ("direct numbering (control)", two(Num::Direct("0"), Num::Direct("0"))),
        ("style ListA carries numPr", two(Num::Style("ListA"), Num::Style("ListA"))),
        ("style ListB basedOn ListA", two(Num::Style("ListB"), Num::Style("ListB"))),
        ("style ListC basedOn ListB basedOn ListA", two(Num::Style("ListC"), Num::Style("ListC"))),
        ("style ListD numPr with numId only", two(Num::Style("ListD"), Num::Style("ListD"))),
        ("direct then style ListB in one list", two(Num::Direct("0"), Num::Style("ListB"))),
        ("style ListB then direct in one list", two(Num::Style("ListB"), Num::Direct("0"))),
    ];
    let mut bad = vec![];
    let mut direct_text = String::new();
    for (i, (label, paras)) in variants.iter().enumerate() {
        let text = extract(&fixture(dir.path(), &format!("v{i}"), "3", paras)).unwrap().text;
        if i == 0 {
            direct_text = text.clone();
            assert!(
                text.lines().any(|l| l == "3. First") && text.lines().any(|l| l == "4. Second"),
                "control: direct numbering must extract «3. First» / «4. Second»:\n{text}"
            );
        }
        if !(text.lines().any(|l| l == "3. First") && text.lines().any(|l| l == "4. Second")) {
            bad.push(format!("{label}: expected «3. First» and «4. Second», got {text:?}"));
        }
        if text != direct_text {
            bad.push(format!("{label}: differs from the direct-numbering extraction {direct_text:?}: {text:?}"));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn test_dod1_val_r43_zero_start_list_extracts_from_zero_directly_and_through_a_style() {
    let dir = tempfile::tempdir().unwrap();
    let mut bad = vec![];
    for (label, paras) in [
        ("direct", two(Num::Direct("0"), Num::Direct("0"))),
        ("style ListA", two(Num::Style("ListA"), Num::Style("ListA"))),
        ("style ListB basedOn ListA", two(Num::Style("ListB"), Num::Style("ListB"))),
    ] {
        let text = extract(&fixture(dir.path(), &format!("zero-{}", label.replace(' ', "-")), "0", &paras)).unwrap().text;
        if !(text.lines().any(|l| l == "0. First") && text.lines().any(|l| l == "1. Second")) {
            bad.push(format!("{label}: a list starting at ZERO must extract «0. First» / «1. Second», got {text:?}"));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn test_dod1_val_r43_style_and_zero_start_numbering_survive_export_then_reimport() {
    let dir = tempfile::tempdir().unwrap();
    let mut bad = vec![];
    for (label, start, paras, ids) in [
        ("style ListB, start 3", "3", two(Num::Style("ListB"), Num::Style("ListB")), [("3.", "First"), ("4.", "Second")]),
        ("direct, start 0", "0", two(Num::Direct("0"), Num::Direct("0")), [("0.", "First"), ("1.", "Second")]),
        ("style ListA, start 0", "0", two(Num::Style("ListA"), Num::Style("ListA")), [("0.", "First"), ("1.", "Second")]),
    ] {
        let sub = dir.path().join(label.replace([' ', ','], "-"));
        fs::create_dir_all(&sub).unwrap();
        let text = extract(&fixture(&sub, "src", start, &paras)).unwrap().text;
        let again = reimport(&sub, &text);
        for (id, body) in ids {
            if !has_identified(&text, id, body) {
                bad.push(format!("{label}: first import lacks {id:?} {body:?}: {text:?}"));
            } else if !has_identified(&again, id, body) {
                bad.push(format!("{label}: after export and reimport {id:?} {body:?} is lost: {again:?} (first import {text:?})"));
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

// ---------------------------------------------------------------- VAL-R44

/// (label, numbering start, paragraphs)
type BadCase = (&'static str, &'static str, Vec<(&'static str, Num)>);

/// No `catch_unwind` here: a panic fails the test, and the call must return Err in this very process.
#[test]
fn test_dod1_val_r44_out_of_range_numbering_returns_a_document_error_without_unwinding() {
    let dir = tempfile::tempdir().unwrap();
    // The same builder with sane values must extract: the Err below is not a broken fixture.
    let ok = extract(&fixture(dir.path(), "control", "3", &two(Num::Direct("0"), Num::Direct("0")))).unwrap();
    assert!(ok.text.lines().any(|l| l == "3. First"), "control: {:?}", ok.text);
    let ok = extract(&fixture(dir.path(), "control-level-1", "3", &two(Num::Direct("1"), Num::Direct("1")))).unwrap();
    assert!(ok.text.contains("First"), "control level 1: {:?}", ok.text);

    let mut bad = vec![];
    let cases: Vec<BadCase> = vec![
        ("start 4294967295, two paragraphs", "4294967295", two(Num::Direct("0"), Num::Direct("0"))),
        ("start 4294967295, one paragraph", "4294967295", vec![("First", Num::Direct("0"))]),
        ("start 4294967295 through a style", "4294967295", two(Num::Style("ListB"), Num::Style("ListB"))),
        ("start 4294967296 (above u32)", "4294967296", two(Num::Direct("0"), Num::Direct("0"))),
        ("negative start -1", "-1", two(Num::Direct("0"), Num::Direct("0"))),
        ("non-numeric start abc", "abc", two(Num::Direct("0"), Num::Direct("0"))),
        ("level 5, beyond the two defined levels", "3", two(Num::Direct("5"), Num::Direct("5"))),
        ("level 9, beyond the OOXML maximum", "3", two(Num::Direct("9"), Num::Direct("9"))),
    ];
    for (i, (label, start, paras)) in cases.iter().enumerate() {
        let path = fixture(dir.path(), &format!("bad{i}"), start, paras);
        match extract(&path) {
            Err(_) => {}
            Ok(x) => bad.push(format!("{label}: extract returned Ok({:?}) instead of a DocumentError", x.text)),
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

// ---------------------------------------------------------------- VAL-R46

struct Knobs;
impl Knobs {
    fn set(vars: &[(&str, &str)]) -> Self {
        std::env::set_var("SYMVOULIO_FAKE_CLI", env!("CARGO_BIN_EXE_fake-cli"));
        for (k, v) in vars {
            std::env::set_var(k, v);
        }
        Self
    }
}
impl Drop for Knobs {
    fn drop(&mut self) {
        for k in ["SYMVOULIO_FAKE_CLI", "FAKE_CLI_REVIEWER_ISSUE_ID", "FAKE_CLI_REVIEWER_SEVERITY", "FAKE_CLI_SCENARIO"] {
            std::env::remove_var(k);
        }
    }
}
/// One round, Claude author, Codex reviewer, both through the product's own CLI adapters on the fake CLI.
fn cli_round(root: &Path) -> (symvoulio_core::engine::RunSnapshot, PathBuf) {
    let primary = root.join("doc.md");
    fs::write(&primary, "# Doc\n\noriginal").unwrap();
    let cfg = RunConfig {
        run_id: "ct46-run".into(),
        name: "ct46".into(),
        run_dir: root.join("run"),
        author: Provider::Claude,
        objective: "Improve".into(),
        criteria: vec!["c".into()],
        policy: Policy { max_rounds: 1, min_rounds: 1, new_content_limit_pct: 100.0, ..Policy::default() },
        primary,
        supporting: vec![],
        call_timeout: Duration::from_secs(10),
        author_model: None,
        reviewer_model: None,
    };
    let run = cfg.run_dir.clone();
    let clients: (Arc<dyn ProviderClient>, Arc<dyn ProviderClient>) = (Arc::new(symvoulio_core::provider::ClaudeCli), Arc::new(symvoulio_core::provider::CodexCli));
    (start(cfg, clients, Arc::default(), |_| {}).unwrap(), run)
}

/// A codex reviewer's decoder may let through an issue_id that the schema pattern ^R[0-9]{2}-[0-9]{3}$ rejects. The
/// review must not be lost to a generic «schema validation failed»: the id is normalised to «R01-001» and the round
/// proceeds, or the run pauses with a controlled reason that names the issue_id FORMAT.
#[test]
fn test_dod1_val_r46_reviewer_issue_id_format() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut bad = vec![];
    for wrong in ["R1-001", "R01-1"] {
        let _k = Knobs::set(&[("FAKE_CLI_REVIEWER_ISSUE_ID", wrong)]);
        let dir = tempfile::tempdir().unwrap();
        let (s, run) = cli_round(dir.path());
        let probe = s.issues.iter().find(|i| i.issue.finding.contains("Probe finding for the issue id format"));
        if let Some(p) = probe {
            // Normalised: the round proceeded with the id R01-001.
            if p.issue.issue_id != "R01-001" {
                bad.push(format!("{wrong}: the finding entered the ledger as {:?}, expected R01-001", p.issue.issue_id));
            }
            if s.state == RunState::Paused {
                bad.push(format!("{wrong}: normalised but the run paused: {:?}", s.last_error));
            }
            continue;
        }
        // Not normalised: the run must pause with a named, controlled reason.
        let Some(err) = s.last_error.as_ref().filter(|_| s.state == RunState::Paused) else {
            bad.push(format!("{wrong}: neither normalised (no probe finding in the ledger) nor paused with an error: state {:?}, issues {}", s.state, s.issues.len()));
            continue;
        };
        let msg = err.message.to_lowercase();
        if err.kind != "malformed" {
            bad.push(format!("{wrong}: the knob did not reach the validator, error kind {:?}: {}", err.kind, err.message));
        }
        if msg.contains("schema validation failed") {
            bad.push(format!("{wrong}: the review is lost to the generic «schema validation failed»: {}", err.message));
        }
        if !(msg.contains("issue_id") && msg.contains("format")) {
            bad.push(format!("{wrong}: the reason does not say that the issue_id format is wrong: {}", err.message));
        }
        if run.join("output/reviews/review_01.json").exists() {
            bad.push(format!("{wrong}: a rejected review was written"));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// Control: an invalid severity enum is still rejected.
#[test]
fn test_dod1_val_r46_control_invalid_severity_enum_is_still_rejected() {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _k = Knobs::set(&[("FAKE_CLI_REVIEWER_SEVERITY", "urgent")]);
    let dir = tempfile::tempdir().unwrap();
    let (s, run) = cli_round(dir.path());
    assert_eq!(s.state, RunState::Paused, "a review with severity «urgent» was accepted: {:?}", s.last_error);
    let err = s.last_error.expect("a rejection reason");
    assert_eq!(err.kind, "malformed", "{err:?}");
    assert!(!err.message.contains("urgent"), "the provider's value is echoed: {}", err.message);
    assert!(s.issues.is_empty(), "the rejected review entered the ledger");
    assert!(!run.join("output/reviews/review_01.json").exists(), "a rejected review was written");
}

// ---------------------------------------------------------------- DoD3

#[test]
fn test_dod3_no_skip_or_ignore_in_this_file() {
    let src = include_str!("ct_astra4_core.rs");
    let code: String = src.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
    for banned in [format!("#[{}", "ignore"), format!("x{}", "fail"), format!("should_{}", "panic"), format!("catch_{}", "unwind")] {
        assert!(!code.contains(banned.as_str()), "found {banned}");
    }
}
