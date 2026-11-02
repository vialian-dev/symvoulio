// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Cruel-tester property checks for card M2-ADAPTERS, DoD2: redact is idempotent and never leaves an `sk-` key
// of length >= 20. (The paste/keystroke properties are in src-tauri/src/ct_pty.rs, next to the code they test.)
use proptest::prelude::*;
use regex::Regex;
use symvoulio_core::redact;

fn leftover_key() -> Regex {
    Regex::new(r"\bsk-[A-Za-z0-9_-]{17,}").unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn test_dod2_redact_is_idempotent_on_arbitrary_text(s in any::<String>()) {
        let once = redact(&s);
        prop_assert_eq!(redact(&once), once);
    }

    #[test]
    fn test_dod2_redact_is_idempotent_on_secret_shaped_text(
        s in "([a-zA-Z0-9+/=_.\\-]| |\"|:|\\{|\\}|Bearer |sk-|sk-ant-|api_key|password|access_token|refresh_token){0,40}"
    ) {
        let once = redact(&s);
        prop_assert_eq!(redact(&once), once);
    }

    #[test]
    fn test_dod2_redact_never_leaves_an_sk_key_in_arbitrary_text(s in any::<String>()) {
        let out = redact(&s);
        prop_assert!(!leftover_key().is_match(&out), "key left in {:?}", out);
    }

    #[test]
    fn test_dod2_redact_removes_an_embedded_sk_key(
        head in "[ :,;=(\\[{\"'>]{0,3}[a-z ]{0,12}[ :,;=(\\[{\"'>]",
        body in "[A-Za-z0-9]{17,60}",
        ant in any::<bool>(),
        tail in "[ :,;=)\\]}\"'.<][a-z ]{0,12}",
    ) {
        let key = format!("{}{body}", if ant { "sk-ant-" } else { "sk-" });
        prop_assert!(key.len() >= 20);
        let out = redact(&format!("{head}{key}{tail}"));
        prop_assert!(!out.contains(&key), "key survived in {:?}", out);
        prop_assert!(!out.contains(&body), "key body survived in {:?}", out);
        prop_assert!(out.contains("[REDACTED]"));
        prop_assert!(!leftover_key().is_match(&out));
    }

    #[test]
    fn test_dod2_redact_keeps_secret_free_prose(words in proptest::collection::vec("[a-zA-Z]{1,10}", 0..12)) {
        let text = words.join(" ");
        prop_assert_eq!(redact(&text), text);
    }

    #[test]
    fn test_dod2_redact_run_length_boundary(n in 1usize..80, ch in "[A-Za-z0-9]") {
        let run = ch.repeat(n);
        let out = redact(&format!("x {run} y"));
        if n >= 32 {
            prop_assert_eq!(out, "x [REDACTED] y");
        } else {
            prop_assert_eq!(out, format!("x {run} y"));
        }
    }

    #[test]
    fn test_dod2_redact_hides_json_secret_values(
        key in prop_oneof![Just("api_key"), Just("access_token"), Just("refresh_token"), Just("password")],
        value in "[A-Za-z0-9 !#$%&()*,./:;<=>?@^~-]{1,20}",
    ) {
        let out = redact(&format!("{{\"{key}\":\"{value}\"}}"));
        prop_assert_eq!(out, format!("{{\"{key}\":\"[REDACTED]\"}}"));
    }
}
