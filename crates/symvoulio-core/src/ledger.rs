// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Persistent issue history across review rounds.
use crate::{model::*, now_utc_rfc3339};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Open issue totals by severity.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    /// Open critical issues.
    pub critical: usize,
    /// Open high issues.
    pub high: usize,
    /// Open medium issues.
    pub medium: usize,
    /// Open low issues.
    pub low: usize,
}
/// An explicit human instruction or risk waiver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HumanIssueDecision {
    /// Issue identifier.
    pub issue_id: String,
    /// Accept requires implementation; Keep explicitly waives the risk.
    pub choice: HumanChoice,
    /// Human rationale.
    pub note: String,
    /// Round after which the choice was made.
    pub round: u32,
}
/// Full reviewer and author history for one issue.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IssueHistory {
    /// First round this identifier was seen.
    pub first_round: u32,
    /// Reviewer snapshots in application order.
    pub reviews: Vec<(u32, Issue)>,
    /// Author replies keyed by their round.
    pub responses: BTreeMap<u32, Response>,
    /// Human instruction or explicit waiver if present.
    pub decision: Option<HumanIssueDecision>,
}
/// An actionable author question persisted independently of reviewer findings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HumanQuestionRecord {
    /// Identifier used by decide_issue.
    pub issue_id: String,
    /// Decision requested by the author.
    pub question: String,
    /// Round in which the question was raised.
    pub round: u32,
    /// Whether a human has answered it.
    pub resolved: bool,
    /// Human answer, retained across resume and restart.
    #[serde(default)]
    pub answer: Option<String>,
    /// Answer timestamp.
    #[serde(default, rename = "answeredAt")]
    pub answered_at: Option<String>,
    /// Decider name, when known.
    #[serde(default, rename = "answeredBy")]
    pub answered_by: Option<String>,
}
/// The register retains omitted issues until explicitly resolved or closed by a human.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct IssueLedger {
    #[serde(default)]
    questions: Vec<HumanQuestionRecord>,
    issues: BTreeMap<String, IssueHistory>,
}
impl IssueHistory {
    /// The first reviewer snapshot reported this finding as already closed.
    pub fn resolved_on_arrival(&self) -> Option<u32> {
        self.reviews.first().and_then(|(round, issue)|
            (issue.status == Status::Resolved).then_some(*round))
    }
}
impl IssueLedger {
    /// Apply reviewer snapshots without implicitly closing omitted findings.
    pub fn apply_review(&mut self, round: u32, review: &Review) {
        for issue in &review.issues {
            let h = self.issues.entry(issue.issue_id.clone()).or_default();
            if h.reviews.is_empty() {
                h.first_round = round;
            }
            h.reviews.push((round, issue.clone()));
        }
    }
    fn next_question_id(&self, round: u32) -> String {
        let mut number = 1;
        loop {
            let id = format!("Q{round:02}-{number:03}");
            if !self.questions.iter().any(|q| q.issue_id == id) { return id; }
            number += 1;
        }
    }
    /// Map legacy question identifiers deterministically without changing issue references.
    pub fn migrate_question_ids(&mut self) {
        for index in 0..self.questions.len() {
            if !self.questions[index].issue_id.starts_with('Q') {
                self.questions[index].issue_id = self.next_question_id(self.questions[index].round);
            }
        }
    }
    /// Attach author responses, including replies received before a review snapshot.
    pub fn apply_author(&mut self, round: u32, author: &AuthorResponse) {
        for q in &author.human_decisions_required {
            if !self
                .questions
                .iter()
                .any(|old| old.question == q.question)
            {
                self.questions.push(HumanQuestionRecord {
                    issue_id: self.next_question_id(round),
                    question: q.question.clone(),
                    round,
                    resolved: false,
                    answer: None,
                    answered_at: None,
                    answered_by: None,
                });
            }
        }
        for response in &author.responses {
            self.issues
                .entry(response.issue_id.clone())
                .or_default()
                .responses
                .insert(round, response.clone());
        }
    }
    /// Record an implementation instruction (Accept) or risk waiver (Keep); unknown identifiers return false.
    pub fn apply_human(
        &mut self,
        issue_id: &str,
        choice: HumanChoice,
        note: &str,
        round: u32,
    ) -> bool {
        let mut answered = false;
        let standalone = self
            .issues
            .get(issue_id)
            .is_none_or(|h| h.reviews.is_empty());
        if let Some(q) = self.questions.iter_mut().find(|q| q.issue_id == issue_id) {
            if choice != HumanChoice::Accept || note.trim().is_empty() {
                if standalone {
                    return false;
                }
            } else {
                q.resolved = true;
                q.answer = Some(note.to_owned());
                q.answered_at = Some(now_utc_rfc3339());
                q.answered_by = None;
                answered = true;
            }
        }
        let Some(h) = self.issues.get_mut(issue_id) else {
            return answered;
        };
        if h.reviews.is_empty() {
            return answered;
        }
        h.decision = Some(HumanIssueDecision {
            issue_id: issue_id.into(),
            choice,
            note: note.into(),
            round,
        });
        true
    }
    /// Attach the known decider identity to an answer without inventing one.
    pub fn set_question_answerer(&mut self, issue_id: &str, name: Option<&str>) {
        if let Some(question) = self
            .questions
            .iter_mut()
            .find(|q| q.issue_id == issue_id && q.answer.is_some())
        {
            question.answered_by = name
                .filter(|name| !name.trim().is_empty())
                .map(str::to_owned);
        }
    }
    /// All actionable author questions, including their resolution state.
    pub fn human_questions(&self) -> &[HumanQuestionRecord] {
        &self.questions
    }
    /// Return currently open issues in identifier order.
    pub fn open(&self) -> Vec<&Issue> {
        self.issues
            .values()
            .filter(|h| {
                h.decision
                    .as_ref()
                    .is_none_or(|d| d.choice != HumanChoice::Keep)
            })
            .filter_map(|h| h.reviews.last().map(|(_, i)| i))
            .filter(|i| i.status == Status::Open)
            .collect()
    }
    /// Count currently open findings.
    pub fn open_counts(&self) -> Counts {
        let mut c = Counts::default();
        for i in self.open() {
            match i.severity {
                Severity::Critical => c.critical += 1,
                Severity::High => c.high += 1,
                Severity::Medium => c.medium += 1,
                Severity::Low => c.low += 1,
            }
        }
        c
    }
    /// Material means critical, high, or medium; low findings never count as new material.
    pub fn new_material_in(&self, round: u32) -> usize {
        self.issues
            .values()
            .map(|h| {
                h.reviews
                    .iter()
                    .enumerate()
                    .filter(|(index, (r, issue))| {
                        *r == round
                            && issue.status == Status::Open
                            && issue.severity != Severity::Low
                            && (*index == 0 || {
                                let previous = &h.reviews[*index - 1].1;
                                previous.status == Status::Resolved
                                    || previous.severity == Severity::Low
                            })
                    })
                    .count()
            })
            .sum()
    }
    /// Retrieve every reviewer snapshot, author reply, and human choice for an issue.
    pub fn history(&self, issue_id: &str) -> Option<&IssueHistory> {
        self.issues.get(issue_id)
    }
    /// All registered histories in identifier order.
    pub fn histories(&self) -> impl Iterator<Item = &IssueHistory> {
        self.issues.values()
    }
    /// Return explicit human issue decisions in identifier order.
    pub fn human_decisions(&self) -> Vec<&HumanIssueDecision> {
        self.issues
            .values()
            .filter_map(|h| h.decision.as_ref())
            .collect()
    }
}
