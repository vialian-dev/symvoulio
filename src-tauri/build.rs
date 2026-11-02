// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
fn main() {
    // Unit tests also link Tauri's Windows helpers, which import comctl32 v6.
    // tauri-build embeds it for the app binary only; for tests add it here (the bin must not get it twice).
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
    }
    tauri_build::build()
}
