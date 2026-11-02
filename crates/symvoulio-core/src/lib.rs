// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Symvoulio review contracts, deterministic gate, document extraction, and output packages.
pub mod cli;
pub mod contribution;
pub mod detect;
pub mod documents;
pub mod engine;
pub mod gate;
pub mod ledger;
pub mod model;
pub mod output;
pub mod process;
pub mod prompts;
pub mod provider;
pub mod redact;
pub use redact::redact;

/// UTC audit timestamp, with seconds precision.
pub fn now_utc_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(time::macros::format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second]Z"
        ))
        .expect("valid UTC timestamp")
}

pub use contribution::{new_sections, new_text_pct};
pub use documents::{extract, DocumentError, DocumentKind, Extracted};
pub use gate::{gate_after, Gate, GateKind, GateReason};
pub use ledger::{Counts, IssueLedger};
pub use model::*;
pub use output::OutputPackage;
