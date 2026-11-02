// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Subscription connection probes; never retain raw authentication responses.
use crate::{cli, process, provider::CancelToken, Provider};
use serde::{Deserialize, Serialize};
use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    Subscription,
    ApiKey,
    Other,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detection {
    pub provider: Provider,
    pub installed: bool,
    pub version: Option<String>,
    pub connected: Option<bool>,
    pub method: Option<AuthMethod>,
    pub status_exit_code: Option<i32>,
    pub error: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeAuth {
    logged_in: bool,
    auth_method: String,
}
pub fn detect(provider: Provider) -> Detection {
    let mut d = Detection {
        provider,
        installed: false,
        version: None,
        connected: None,
        method: None,
        status_exit_code: None,
        error: None,
    };
    let Some(cli) = cli::resolve(provider) else {
        return d;
    };
    let cancel: CancelToken = Arc::new(AtomicBool::new(false));
    let mut cmd = process::command(&cli);
    cmd.arg("--version");
    match process::run(
        cmd,
        String::new(),
        Duration::from_secs(20),
        &cancel,
        &mut |_| {},
    ) {
        Ok(o) if o.exit_code == Some(0) => {
            d.installed = true;
            d.version = crate::redact::version(&o.stdout.join("\n"));
        }
        _ => {
            d.error = Some("version probe failed".into());
            return d;
        }
    }
    let mut cmd = process::command(&cli);
    cmd.args(match provider {
        Provider::Claude => ["auth", "status"],
        Provider::Codex => ["login", "status"],
    });
    match process::run(
        cmd,
        String::new(),
        Duration::from_secs(20),
        &cancel,
        &mut |_| {},
    ) {
        Err(_) => d.error = Some("status probe failed or timed out".into()),
        Ok(o) => {
            d.status_exit_code = o.exit_code;
            if provider == Provider::Claude {
                match serde_json::from_str::<ClaudeAuth>(&o.stdout.join("\n")) {
                    Ok(a) => {
                        d.connected =
                            Some(o.exit_code == Some(0) && a.logged_in && a.auth_method != "none");
                        d.method = Some(match a.auth_method.as_str() {
                            "claude.ai" | "oauth_token" => AuthMethod::Subscription,
                            "api_key" | "api_key_helper" => AuthMethod::ApiKey,
                            _ => AuthMethod::Other,
                        });
                    }
                    Err(_) => {
                        d.connected = Some(false);
                        d.error = Some("invalid authentication status".into());
                    }
                }
            } else {
                let text = format!("{} {}", o.stdout.join("\n"), o.stderr).to_ascii_lowercase();
                d.connected = Some(o.exit_code == Some(0));
                d.method = Some(if text.contains("chatgpt") {
                    AuthMethod::Subscription
                } else if text.contains("api key") {
                    AuthMethod::ApiKey
                } else {
                    AuthMethod::Other
                });
            }
        }
    }
    d
}
