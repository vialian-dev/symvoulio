// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Deterministic gate matching the prototype check order.
use crate::{
    ledger::{Counts, IssueLedger},
    model::{Policy, StopReason},
};
use serde::{Deserialize, Serialize};
/// Gate disposition; stopped requires human review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateKind {
    /// Another round is needed.
    Continue,
    /// Automatic review has ended.
    Stop,
    /// The round cap leaves blocking findings.
    Stopped,
}
/// Deterministic explanation, never a human approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateReason {
    /// Blocking issues remain.
    ContinueOpen,
    /// Minimum rounds not reached.
    ContinueMin,
    /// Two rounds without new material not yet reached.
    ContinueNew,
    /// The revision exceeded the permitted new content.
    ContinueContribution,
    /// Two consecutive rounds without new material.
    StopNoNew,
    /// Enabled blocking checks are clear.
    StopClean,
    /// Round cap reached without critical or high issues.
    StopMax,
    /// Round cap reached with critical or high issues.
    StoppedHuman,
}
impl GateReason {
    /// Map terminal gate reasons to the manifest vocabulary.
    pub fn stop_reason(self) -> Option<StopReason> {
        match self {
            Self::StopNoNew => Some(StopReason::GateNoNewIssues),
            Self::StopClean => Some(StopReason::GateClean),
            Self::StopMax => Some(StopReason::GateMaxRounds),
            Self::StoppedHuman => Some(StopReason::GateHumanReviewRequired),
            _ => None,
        }
    }
}
/// A gate verdict and the open counts used to derive it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gate {
    /// Continue, stop, or require a human.
    pub kind: GateKind,
    /// Ordered rule that matched.
    pub reason: GateReason,
    /// Open issue totals.
    pub counts: Counts,
}
/// Evaluate after a round; the cap is enforced even if stop.max_rounds is false, as in the UI.
pub fn gate_after(round: u32, policy: &Policy, ledger: &IssueLedger) -> Gate {
    use GateKind::*;
    use GateReason::*;
    let counts = ledger.open_counts();
    let human_pending = ledger.human_questions().iter().any(|q| !q.resolved);
    let blocking = (policy.stop.no_critical && counts.critical > 0)
        || (policy.stop.no_high && counts.high > 0);
    let (kind, reason) = if round >= policy.max_rounds {
        if blocking || counts.critical + counts.high > 0 || human_pending {
            (Stopped, StoppedHuman)
        } else {
            (Stop, StopMax)
        }
    } else if blocking || human_pending {
        (Continue, ContinueOpen)
    } else if round < policy.min_rounds {
        (Continue, ContinueMin)
    } else if policy.stop.no_new {
        if round >= 2
            && ledger.new_material_in(round) == 0
            && ledger.new_material_in(round - 1) == 0
        {
            (Stop, StopNoNew)
        } else {
            (Continue, ContinueNew)
        }
    } else {
        (Stop, StopClean)
    };
    Gate {
        kind,
        reason,
        counts,
    }
}
