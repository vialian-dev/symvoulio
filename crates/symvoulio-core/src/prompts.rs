// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Public example prompts, with independent reviewer context.
use crate::{Contribution, Issue, Policy};
fn context(
    round: u32,
    objective: &str,
    criteria: &[String],
    doc: &str,
    supporting: &[(String, String)],
) -> String {
    let mut s=format!("ROUND: {round}\nObjective: {objective}\nCriteria:\n{}\n<<<CURRENT_DOCUMENT\n{doc}\nCURRENT_DOCUMENT>>>\n",criteria.join("\n"));
    for (name, text) in supporting {
        s.push_str(&format!("<<<FILE {name}\n{text}\nFILE>>>\n"));
    }
    s
}
#[allow(clippy::too_many_arguments)]
pub fn author_prompt(
    round: u32,
    objective: &str,
    criteria: &[String],
    policy: &Policy,
    original_doc: &str,
    current_doc: &str,
    supporting: &[(String, String)],
    open_findings: &[Issue],
) -> String {
    let mut s = context(round, objective, criteria, current_doc, supporting)
        + &format!("<<<ORIGINAL_DOCUMENT\n{original_doc}\nORIGINAL_DOCUMENT>>>\n");
    s.push_str("Return the whole revised document as Markdown in revised_document. Answer every open finding with accepted, rejected_with_evidence, partially_accepted, or human_decision_required. Do not add unsupported claims. List every new claim in new_claims. For human_decisions_required, issue_id is an ignored placeholder; the application assigns each new question its own Q identifier.\n");
    let mode = match policy.contribution {
        Contribution::Verify => "Verify: change nothing beyond fixing findings",
        Contribution::Correct => "Correct: limited new text",
        Contribution::Improve => "Improve: restructuring allowed",
    };
    s.push_str(&format!(
        "{mode}. Always within {} % new text.\n<<<OPEN_FINDINGS\n{}\nOPEN_FINDINGS>>>\n",
        policy.new_content_limit_pct,
        serde_json::to_string(open_findings).expect("issues")
    ));
    s
}
#[allow(clippy::too_many_arguments)]
pub fn reviewer_prompt(
    round: u32,
    objective: &str,
    criteria: &[String],
    policy: &Policy,
    original_doc: &str,
    current_doc: &str,
    supporting: &[(String, String)],
    previously_open: &[Issue],
) -> String {
    let mut s = context(round, objective, criteria, current_doc, supporting)
        + &format!("<<<ORIGINAL_DOCUMENT\n{original_doc}\nORIGINAL_DOCUMENT>>>\n");
    s.push_str(&format!(
        "Reviewer contribution mode: {:?}; new content limit {} %.\n",
        policy.contribution, policy.new_content_limit_pct
    ));
    s.push_str(&format!("You did not write this document. Judge it only against the criteria and sources. Re-report every previously open issue with the same id, as open or resolved. New issues get ids R{round:02}-<nnn>.\nCritical: invalidates the decision or creates unacceptable risk.\nHigh: material error requiring correction.\nMedium: meaningful weakness requiring attention.\nLow: minor improvement.\nPreviously open (ids and findings only):\n"));
    for i in previously_open {
        s.push_str(&format!("{}: {}\n", i.issue_id, i.finding));
    }
    s
}

/// Human implementation instructions and explicit risk waivers, shared with both roles.
pub fn human_instructions(ledger: &crate::IssueLedger) -> String {
    let mut text = String::from("\nHuman instructions and risk waivers:\n");
    for decision in ledger.human_decisions() {
        let meaning = match decision.choice {
            crate::HumanChoice::Accept => "implement; remains open until reviewer resolves",
            crate::HumanChoice::Keep => "waived; explicit human risk waiver",
        };
        text.push_str(&format!(
            "{}: {}: {}\n",
            decision.issue_id, meaning, decision.note
        ));
    }
    text.push_str("Answers from the human:\n");
    for question in ledger.human_questions() {
        if let Some(answer) = &question.answer {
            text.push_str(&format!(
                "- Q ({}): {}\n  A: {}\n  answered by {} at {}\n",
                question.issue_id,
                question.question,
                answer,
                question.answered_by.as_deref().unwrap_or("unknown"),
                question.answered_at.as_deref().unwrap_or("unknown")
            ));
        }
    }
    text
}
