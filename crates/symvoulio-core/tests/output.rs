// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
use std::{fs, path::PathBuf};
use symvoulio_core::*;
fn manifest() -> RunManifest {
    RunManifest {
        schema_version: 1,
        run_id: "test".into(),
        name: "Northwind <script>alert(1)</script>".into(),
        created_at: "2026-10-30T12:00:00Z".into(),
        updated_at: "2026-10-30T12:00:00Z".into(),
        os: "windows x86_64".into(),
        app_version: "0.1.0".into(),
        author: Participant {
            provider: Provider::Claude,
            cli_version: None,
            model: None,
        },
        reviewer: Participant {
            provider: Provider::Codex,
            cli_version: None,
            model: None,
        },
        inputs: vec![],
        objective: "Improve".into(),
        criteria: vec!["Reconcile".into()],
        policy: Policy::default(),
        rounds_completed: 2,
        state: RunState::Completed,
        stop_reason: Some(StopReason::GateNoNewIssues),
        human_decision: None,
    }
}
#[test]
fn full_two_round_package_tree_contract_and_docx_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let primary = dir.path().join("primary.md");
    let support = dir.path().join("capex.csv");
    fs::write(&primary, "# Original\n\nOriginal text.").unwrap();
    fs::write(&support, "cost,amount\ncapex,4600000\n").unwrap();
    let p = OutputPackage::create(dir.path(), &primary, std::slice::from_ref(&support)).unwrap();
    let root = dir.path().join("output");
    let mut l = IssueLedger::default();
    let mut review: Review =
        serde_json::from_str(include_str!("../../../samples/fixtures/review_01.json")).unwrap();
    let mut author: AuthorResponse = serde_json::from_str(include_str!(
        "../../../samples/fixtures/author_response_01.json"
    ))
    .unwrap();
    let mut rounds = vec![];
    for n in 1..=2 {
        let markdown=format!("# Revision {n}\n\n## Section\n\nA **bold** and *italic* paragraph.\n\n- Bullet item\n\n### Detail\n\nLast text {n}.");
        author.revised_document = markdown.clone();
        if n == 2 {
            review.issues[0].status = Status::Resolved;
            l.apply_human("R01-002", HumanChoice::Keep, "Retained by owner", 2);
        }
        p.write_revision(n, &markdown).unwrap();
        p.write_author_response(n, &author).unwrap();
        p.write_review(n, &review).unwrap();
        l.apply_author(n, &author);
        l.apply_review(n, &review);
        rounds.push((n, gate_after(n, &Policy::default(), &l)));
        for (role, provider) in [
            (Role::Author, Provider::Claude),
            (Role::Reviewer, Provider::Codex),
        ] {
            p.write_provider_output(
                n,
                role,
                provider,
                1,
                &["{\"raw\":true}".into()],
                if n == 1 && role == Role::Author {
                    "diagnostic"
                } else {
                    ""
                },
            )
            .unwrap();
        }
    }
    let mut m = manifest();
    p.write_manifest(&m).unwrap();
    p.write_timeline(&[TimelineEvent {
        attempt: None,
        ts: m.created_at.clone(),
        kind: TimelineKind::RunStarted,
        round: None,
        role: None,
        provider: None,
        detail: "Started".into(),
        duration_s: None,
        usage: Some(Usage {
            input_tokens: 1,
            output_tokens: 2,
            cache_read_tokens: 0,
            cache_creation_tokens: None,
        }),
    }])
    .unwrap();
    p.write_final(&m, &l, Some(&review), &rounds).unwrap();
    let mut expected = vec![
        "original/primary.md",
        "original/supporting/capex.csv",
        "final/final_document.docx",
        "final/final_document.md",
        "final/final_review.html",
        "final/unresolved_issues.json",
        "final/human_decision.json",
        "audit/run_manifest.json",
        "audit/timeline.json",
        "audit/provider_output/round_01_author_claude_a01.stderr.txt",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();
    for n in 1..=2 {
        for (folder, name, ext) in [
            ("revisions", "revision", "docx"),
            ("revisions", "revision", "md"),
            ("reviews", "review", "json"),
            ("reviews", "author_response", "json"),
        ] {
            expected.push(format!("{folder}/{name}_{n:02}.{ext}"));
        }
        for call in ["author_claude", "reviewer_codex"] {
            expected.push(format!(
                "audit/provider_output/round_{n:02}_{call}_a01.jsonl"
            ));
        }
    }
    expected.sort();
    assert_eq!(p.tree(), expected);
    assert_eq!(
        fs::read(root.join("original/primary.md")).unwrap(),
        fs::read(primary).unwrap()
    );
    assert_eq!(
        fs::read(root.join("original/supporting/capex.csv")).unwrap(),
        fs::read(support).unwrap()
    );
    for (path, text) in [
        ("revisions/revision_01.docx", "Last text 1."),
        ("revisions/revision_02.docx", "Last text 2."),
        ("final/final_document.docx", "Last text 2."),
    ] {
        let bytes = fs::read(root.join(path)).unwrap();
        let doc = docx_rs::read_docx(&bytes).unwrap();
        assert!(serde_json::to_string(&doc).unwrap().contains(text));
        assert!(extract(&root.join(path))
            .unwrap()
            .text
            .contains("Bullet item"));
    }
    let doc =
        docx_rs::read_docx(&fs::read(root.join("final/final_document.docx")).unwrap()).unwrap();
    let json = serde_json::to_string(&doc).unwrap();
    assert!(json.contains("Heading3"));
    let paragraphs: Vec<_> = doc
        .document
        .children
        .iter()
        .filter_map(|child| {
            if let docx_rs::DocumentChild::Paragraph(p) = child {
                Some(p)
            } else {
                None
            }
        })
        .collect();
    assert!(paragraphs
        .iter()
        .any(|p| p.property.numbering_property.is_some()));
    for (text, bold) in [("bold", true), ("italic", false)] {
        assert!(paragraphs.iter().any(|p| p.children.iter().any(|child| {
            if let docx_rs::ParagraphChild::Run(run) = child {
                run.children
                    .iter()
                    .any(|child| matches!(child, docx_rs::RunChild::Text(t) if t.text == text))
                    && if bold {
                        run.run_property.bold.is_some()
                    } else {
                        run.run_property.italic.is_some()
                    }
            } else {
                false
            }
        })));
    }
    assert_eq!(
        fs::read_to_string(root.join("final/final_document.md")).unwrap(),
        author.revised_document
    );
    let schema =
        serde_json::from_str(include_str!("../../../schemas/run-manifest.schema.json")).unwrap();
    let value =
        serde_json::from_slice(&fs::read(root.join("audit/run_manifest.json")).unwrap()).unwrap();
    jsonschema::validator_for(&schema)
        .unwrap()
        .validate(&value)
        .unwrap();
    let summary: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("reviews/author_response_02.json")).unwrap())
            .unwrap();
    assert!(summary.get("revised_document").is_none());
    let html = fs::read_to_string(root.join("final/final_review.html")).unwrap();
    assert!(html.contains("Not decided — this is not an approval."));
    assert!(html.contains("Model agreement is evidence, not approval."));
    assert!(!html.contains("<script>"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(html.contains("Round 2:"));
    let unresolved: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("final/unresolved_issues.json")).unwrap())
            .unwrap();
    assert_eq!(unresolved["open_issues"].as_array().unwrap().len(), 1);
    assert_eq!(unresolved["human_issue_decisions"][0]["choice"], "keep");
    assert_eq!(
        fs::read_to_string(root.join("final/human_decision.json")).unwrap(),
        "null"
    );
    m.human_decision = Some(HumanDecision {
        kind: DecisionKind::Approved,
        name: "Owner <&>".into(),
        note: "Reviewed".into(),
        at: m.updated_at.clone(),
        after_round: 2,
        revision: 2,
        document_sha256: {
            use sha2::{Digest, Sha256};
            format!(
                "{:x}",
                Sha256::digest(fs::read(root.join("final/final_document.md")).unwrap())
            )
        },
    });
    p.write_manifest(&m).unwrap();
    p.write_final(&m, &l, Some(&review), &rounds).unwrap();
    assert!(fs::read_to_string(root.join("final/human_decision.json"))
        .unwrap()
        .contains("approved"));
    assert!(fs::read_to_string(root.join("final/final_review.html"))
        .unwrap()
        .contains("Owner &lt;&amp;&gt;"));
    assert!(p.tree().iter().all(|p| !p.ends_with(".tmp")));
}
#[test]
fn no_revision_uses_original_and_no_support_directory() {
    let dir = tempfile::tempdir().unwrap();
    let primary = dir.path().join("input.txt");
    fs::write(&primary, "Original text").unwrap();
    let p = OutputPackage::create(dir.path(), &primary, &[]).unwrap();
    let mut original_manifest = manifest();
    original_manifest.rounds_completed = 0;
    p.write_final(&original_manifest, &IssueLedger::default(), None, &[])
        .unwrap();
    assert!(!dir.path().join("output/original/supporting").exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("output/final/final_document.md")).unwrap(),
        "Original text"
    );
    assert!(
        extract(&dir.path().join("output/final/final_document.docx"))
            .unwrap()
            .text
            .contains("Original text")
    );
    assert!(p.write_revision(0, "bad").is_err());
    assert!(OutputPackage::create(dir.path(), &primary, &[]).is_err());
}
#[test]
fn primary_csv_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.csv");
    fs::write(&path, "a,b").unwrap();
    assert!(OutputPackage::create(dir.path(), &path, &[]).is_err());
}
#[test]
fn duplicate_support_names_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.md");
    fs::write(&path, "text").unwrap();
    let support: PathBuf = path.clone();
    assert!(OutputPackage::create(dir.path(), &path, &[support.clone(), support]).is_err());
}
