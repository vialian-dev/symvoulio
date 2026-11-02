// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// Cruel-tester checks for card M2-ADAPTERS: safe paste (VAL9), raw keystrokes (VAL10), shell choice (VAL12).
// Runs in the Windows merge gate (src-tauri test target `command_layer`).
//
// VAL11 (replay without ESC[6n, split UTF-8 decoding) and the saved-shell branch of VAL12 sit behind private items
// of pty.rs (`Output`, `Utf8Decoder`, `choose_shell`); they cannot be reached from a sibling module. See the report.
use crate::pty::{shell_info, validate_keystrokes, validate_paste};
use proptest::prelude::*;

const ESC: &str = "\x1b";

// ---------- VAL9: validate_paste ----------

#[test]
fn test_dod1_val9_login_commands_pass_unchanged() {
    for command in ["claude auth login", "codex login --with-api-key"] {
        assert_eq!(validate_paste(command).unwrap(), command);
    }
}

#[test]
fn test_dod1_val9_every_control_char_is_refused() {
    for code in 0u32..0x20 {
        let c = char::from_u32(code).unwrap();
        for text in [format!("{c}"), format!("claude{c}"), format!("{c}claude"), format!("cl{c}aude")] {
            assert!(validate_paste(&text).is_err(), "control U+{code:04X} accepted in {text:?}");
        }
    }
    for text in ["\x7f", "claude\x7f", "x\x7fy"] {
        assert!(validate_paste(text).is_err(), "DEL accepted in {text:?}");
    }
}

#[test]
fn test_dod1_val9_cr_lf_tab_are_refused() {
    for text in ["x\r", "x\n", "x\t", "x\r\n", "\r", "\n", "\t", "a\nb", "a\rb", "a\tb"] {
        assert!(validate_paste(text).is_err(), "{text:?} accepted");
    }
}

#[test]
fn test_dod1_val9_length_boundary_512_chars() {
    assert_eq!(validate_paste(&"x".repeat(512)).unwrap(), "x".repeat(512));
    assert!(validate_paste(&"x".repeat(513)).is_err());
    assert!(validate_paste(&"x".repeat(100_000)).is_err());
}

#[test]
fn test_dod1_val9_length_counts_chars_not_bytes() {
    // 512 two-byte chars are 1024 bytes but 512 chars: allowed. 513 are refused.
    let ok = "\u{e9}".repeat(512);
    assert_eq!(validate_paste(&ok).unwrap(), ok);
    assert!(validate_paste(&"\u{e9}".repeat(513)).is_err());
    // Cyrillic and astral chars count as one char each.
    let cyr = "ж".repeat(512);
    assert_eq!(validate_paste(&cyr).unwrap(), cyr);
    let emoji = "\u{1f600}".repeat(512);
    assert_eq!(validate_paste(&emoji).unwrap(), emoji);
    assert!(validate_paste(&"\u{1f600}".repeat(513)).is_err());
}

#[test]
fn test_dod1_val9_text_passes_unchanged() {
    for text in ["a", "claude auth login --console", "echo \"hi\" ; ls", "путь к файлу", " leading and trailing "] {
        assert_eq!(validate_paste(text).unwrap(), text);
    }
}

#[test]
fn test_sub4_val9_paste_refuses_trailing_enter_and_bracketed_paste_controls() {
    assert!(validate_paste("claude auth login\r").is_err());
    assert!(validate_paste(&format!("{ESC}[200~claude auth login{ESC}[201~")).is_err());
    assert!(validate_paste(&format!("{ESC}[200~")).is_err());
}

// ---------- VAL10: validate_keystrokes ----------

#[test]
fn test_dod1_val10_single_enter_passes() {
    assert_eq!(validate_keystrokes("\r").unwrap(), "\r");
}

#[test]
fn test_dod1_val10_plain_keys_and_escape_sequences_pass() {
    for data in [
        "a",
        "hello",
        "\u{e9}",
        "ж",
        "\u{1f600}",
        "\x7f",
        "\t",
        "\x03",
        "\x1b",
        "\x1b[A",
        "\x1b[B",
        "\x1b[C",
        "\x1b[D",
        "\x1b[1;5C",
        "\x1bOA",
        "\x1b[3~",
        "\x1b[200",
        "\x1b[20~",
        "\x1b[2000~",
    ] {
        assert_eq!(validate_keystrokes(data).unwrap(), data, "{data:?}");
    }
}

#[test]
fn test_dod1_val10_cursor_position_report_passes() {
    for data in ["\x1b[1;1R", "\x1b[24;80R", "\x1b[12;40R", "\x1b[999;999R"] {
        assert_eq!(validate_keystrokes(data).unwrap(), data);
    }
}

#[test]
fn test_sub4_val10_multi_char_chunk_with_cr_or_lf_is_refused() {
    for data in [
        "x\r", "x\n", "\r\n", "\n\r", "\r\r", "\n\n", "hello\r", "one\ntwo", "\u{e9}\r", "ж\n", "\r\u{e9}", "\x1b[A\r",
        "a\r\nb",
    ] {
        assert!(validate_keystrokes(data).is_err(), "unsafe chunk accepted: {data:?}");
    }
}

#[test]
fn test_sub4_val10_bracketed_paste_markers_are_refused_anywhere() {
    for data in [
        "\x1b[200~",
        "\x1b[201~",
        "\x1b[200~hello",
        "hello\x1b[201~",
        "a\x1b[200~b",
        "\x1b[200~\x1b[201~",
        "\x1b[201~\x1b[200~",
        "x\x1b[A\x1b[200~",
    ] {
        assert!(validate_keystrokes(data).is_err(), "bracketed paste accepted: {data:?}");
    }
}

#[test]
fn test_dod1_val10_enter_split_from_text_is_each_fine_but_joined_is_refused() {
    assert!(validate_keystrokes("x").is_ok());
    assert!(validate_keystrokes("\r").is_ok());
    assert!(validate_keystrokes("x\r").is_err());
}

// ---------- VAL12: shell choice (what is reachable through the public `shell_info`) ----------

#[cfg(windows)]
mod shell {
    use super::*;
    use std::sync::Mutex;
    static PATH_LOCK: Mutex<()> = Mutex::new(());

    fn with_path<R>(dir: &std::path::Path, f: impl FnOnce() -> R) -> R {
        let _guard = PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var_os("PATH");
        std::env::set_var("PATH", dir);
        let result = f();
        match old {
            Some(v) => std::env::set_var("PATH", v),
            None => std::env::remove_var("PATH"),
        }
        result
    }
    fn probe() -> serde_json::Value {
        serde_json::to_value(shell_info().unwrap()).unwrap()
    }

    #[test]
    fn test_dod1_val12_pwsh_present_is_chosen() {
        let dir = std::env::temp_dir().join(format!("ct-pty-both-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("pwsh.exe"), b"").unwrap();
        std::fs::write(dir.join("powershell.exe"), b"").unwrap();
        let v = with_path(&dir, probe);
        assert_eq!(v["pwsh"], true);
        assert_eq!(v["powershell"], true);
        assert_eq!(v["chosen"], "pwsh");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_dod1_val12_falls_back_to_powershell_without_pwsh() {
        let dir = std::env::temp_dir().join(format!("ct-pty-ps-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("powershell.exe"), b"").unwrap();
        let v = with_path(&dir, probe);
        assert_eq!(v["pwsh"], false);
        assert_eq!(v["powershell"], true);
        assert_eq!(v["chosen"], "powershell");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_dod1_val12_powershell_is_the_default_when_neither_is_found() {
        let dir = std::env::temp_dir().join(format!("ct-pty-none-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let v = with_path(&dir, probe);
        assert_eq!(v["pwsh"], false);
        assert_eq!(v["powershell"], false);
        assert_eq!(v["chosen"], "powershell");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ---------- DoD2: property tests ----------

fn has_control(s: &str) -> bool {
    s.chars().any(|c| (c as u32) < 0x20 || c == '\x7f')
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    // No control char ever passes paste; whatever passes is returned unchanged and is at most 512 chars.
    #[test]
    fn test_dod2_paste_never_passes_a_control_char(s in any::<String>()) {
        if let Ok(out) = validate_paste(&s) {
            prop_assert_eq!(out, s.as_str());
            prop_assert!(!has_control(out));
            prop_assert!(out.chars().count() <= 512);
        }
    }

    // The converse, from the card: everything else passes unchanged.
    #[test]
    fn test_dod2_paste_accepts_clean_text_up_to_512(s in "[^\\x00-\\x1f\\x7f]{1,600}") {
        let n = s.chars().count();
        let r = validate_paste(&s);
        if n <= 512 {
            prop_assert_eq!(r.unwrap(), s.as_str());
        } else {
            prop_assert!(r.is_err());
        }
    }

    // Inserting any one control char anywhere into clean text makes paste refuse it.
    #[test]
    fn test_dod2_paste_refuses_an_injected_control(
        head in "[a-z ]{0,20}",
        tail in "[a-z ]{0,20}",
        code in prop_oneof![0u32..0x20, Just(0x7fu32)],
    ) {
        let text = format!("{head}{}{tail}", char::from_u32(code).unwrap());
        prop_assert!(validate_paste(&text).is_err());
    }

    // No CR/LF-carrying multi-char chunk ever passes keystrokes, and no bracketed-paste marker either.
    #[test]
    fn test_dod2_keystrokes_never_pass_multi_char_cr_lf(s in any::<String>()) {
        if let Ok(out) = validate_keystrokes(&s) {
            prop_assert_eq!(out, s.as_str());
            prop_assert!(!(s.chars().count() > 1 && s.contains(['\r', '\n'])));
            prop_assert!(!s.contains("\x1b[200~") && !s.contains("\x1b[201~"));
        }
    }

    // Adversarial alphabet built from the interesting characters, checked against the card as an oracle.
    #[test]
    fn test_dod2_keystrokes_match_the_card_oracle(
        chars in proptest::collection::vec(
            prop_oneof![
                Just('\r'), Just('\n'), Just('\x1b'), Just('['), Just('2'), Just('0'), Just('1'),
                Just('~'), Just('R'), Just(';'), Just('a'), Just('\u{e9}'), Just('\t')
            ],
            0..14,
        )
    ) {
        let s: String = chars.into_iter().collect();
        let refused = (s.chars().count() > 1 && s.contains(['\r', '\n']))
            || s.contains("\x1b[200~")
            || s.contains("\x1b[201~");
        prop_assert_eq!(validate_keystrokes(&s).is_err(), refused, "{:?}", s);
    }

    // A real arrow/CPR sequence passes; appending CR or LF to it never does.
    #[test]
    fn test_dod2_cursor_position_report_passes_but_not_with_enter(r in 1u16..1000, c in 1u16..1000) {
        let report = format!("\x1b[{r};{c}R");
        prop_assert!(validate_keystrokes(&report).is_ok());
        let with_cr = format!("{report}\r");
        let with_lf = format!("{report}\n");
        prop_assert!(validate_keystrokes(&with_cr).is_err());
        prop_assert!(validate_keystrokes(&with_lf).is_err());
    }
}
