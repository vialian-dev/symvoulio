// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Public JSON contracts and validated run policy.
use serde::{Deserialize, Serialize};

macro_rules! wire_enum {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        #[doc = concat!(stringify!($name), " wire values.")]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $(#[doc = stringify!($variant)] $variant),+ }
    };
}
wire_enum!(Severity {
    Critical,
    High,
    Medium,
    Low
});
wire_enum!(Status { Open, Resolved });
wire_enum!(IssueResponse {
    Accepted,
    RejectedWithEvidence,
    PartiallyAccepted,
    HumanDecisionRequired
});
wire_enum!(Provider { Claude, Codex });
impl Provider {
    /// Return the independent counterpart.
    pub fn other(self) -> Self {
        match self {
            Self::Claude => Self::Codex,
            Self::Codex => Self::Claude,
        }
    }
}
wire_enum!(Role { Author, Reviewer });
wire_enum!(RunState {
    Running,
    Paused,
    Stopped,
    Completed,
    Approved,
    Rejected,
    Returned
});
wire_enum!(StopReason {
    GateNoNewIssues,
    GateClean,
    GateMaxRounds,
    GateHumanReviewRequired,
    UserStop,
    ProviderError,
    Cancelled
});
wire_enum!(Contribution {
    Verify,
    Correct,
    Improve
});
wire_enum!(InputRole {
    Primary,
    Supporting
});
wire_enum!(DecisionKind {
    Approved,
    Rejected,
    Returned
});
// Wire names are stable: Accept instructs the author to implement the finding;
// only reviewer resolution closes it. Keep explicitly waives the risk.
wire_enum!(HumanChoice { Accept, Keep });
wire_enum!(TimelineKind {
    RunStarted,
    AuthorStarted,
    AuthorCompleted,
    ReviewStarted,
    ReviewCompleted,
    Gate,
    Paused,
    Resumed,
    Stopped,
    ProviderError,
    HumanDecision,
    Exported
});

macro_rules! record {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[doc = concat!(stringify!($name), " JSON record.")]
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name { $(#[doc = stringify!($field)] pub $field: $ty),* }
    };
}
record!(Review { summary: String, issues: Vec<Issue>, criteria_results: Vec<CriterionResult> });
record!(Issue {
    issue_id: String,
    severity: Severity,
    criterion: String,
    finding: String,
    evidence: String,
    proposed_action: String,
    human_decision_required: bool,
    status: Status
});
record!(CriterionResult {
    criterion: String,
    met: bool,
    note: String
});
record!(AuthorResponse { revised_document: String, changes_made: Vec<String>, responses: Vec<Response>, new_claims: Vec<String>, human_decisions_required: Vec<HumanQuestion> });
record!(Response {
    issue_id: String,
    response: IssueResponse,
    explanation: String,
    evidence: String
});
record!(HumanQuestion {
    issue_id: String,
    question: String
});
record!(Participant { provider: Provider, cli_version: Option<String>, model: Option<String> });
record!(InputFile {
    file_name: String,
    role: InputRole,
    bytes: u64,
    sha256: String,
    extracted_chars: u64
});
record!(StopRecord {
    no_critical: bool,
    no_high: bool,
    no_new: bool,
    max_rounds: bool
});
record!(Policy {
    max_rounds: u32,
    min_rounds: u32,
    contribution: Contribution,
    new_content_limit_pct: f64,
    stop: StopRecord
});
/// The manifest uses the same policy contract as the gate.
pub type PolicyRecord = Policy;
record!(HumanDecision {
    kind: DecisionKind,
    name: String,
    note: String,
    at: String,
    after_round: u32,
    revision: u32,
    document_sha256: String
});
record!(RunManifest { schema_version: u32, run_id: String, name: String, created_at: String, updated_at: String, os: String, app_version: String, author: Participant, reviewer: Participant, inputs: Vec<InputFile>, objective: String, criteria: Vec<String>, policy: PolicyRecord, rounds_completed: u32, state: RunState, stop_reason: Option<StopReason>, human_decision: Option<HumanDecision> });
record!(Usage { input_tokens: u64, output_tokens: u64, cache_read_tokens: u64, cache_creation_tokens: Option<u64> });
record!(TimelineEvent { attempt: Option<u32>, ts: String, kind: TimelineKind, round: Option<u32>, role: Option<Role>, provider: Option<Provider>, detail: String, duration_s: Option<f64>, usage: Option<Usage> });
impl Default for Policy {
    fn default() -> Self {
        Self {
            max_rounds: 5,
            min_rounds: 2,
            contribution: Contribution::Correct,
            new_content_limit_pct: 10.0,
            stop: StopRecord {
                no_critical: true,
                no_high: true,
                no_new: true,
                max_rounds: true,
            },
        }
    }
}
impl Policy {
    /// Validate round bounds and the finite percentage permitted by the schema.
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=5).contains(&self.max_rounds)
            || !(1..=5).contains(&self.min_rounds)
            || self.min_rounds > self.max_rounds
        {
            return Err("Rounds must be in 1..=5, with min_rounds <= max_rounds".into());
        }
        if !self.new_content_limit_pct.is_finite()
            || !(0.0..=100.0).contains(&self.new_content_limit_pct)
        {
            return Err("new_content_limit_pct must be a finite percentage in 0..=100".into());
        }
        Ok(())
    }
}
