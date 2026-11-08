// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
// MAC-05 and MAC-07: an app opened from Finder gets launchd's PATH (/usr/bin:/bin:/usr/sbin:/sbin), not the
// user's shell PATH. Provider CLIs in Homebrew, npm global and the official installers' folders must still be found,
// and an npm CLI (`#!/usr/bin/env node`) must find its interpreter. The pure parts run on every OS; the end-to-end
// parts run on macOS and Linux with stand-in CLIs only (no real provider is ever called).
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};
use symvoulio_core::cli;

const FINDER_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

fn dirs(launch: &str, login: Option<&str>, home: Option<&Path>) -> Vec<PathBuf> {
    // split_paths uses ';' on Windows; build the launch value with the host separator.
    let launch = std::env::join_paths(launch.split(':').filter(|s| !s.is_empty())).unwrap();
    let login = login.map(|l| std::env::join_paths(l.split(':')).unwrap());
    cli::unix_search_dirs(&launch, login.as_deref(), home)
}

#[test]
fn mac05_finder_path_gains_homebrew_and_user_install_folders() {
    let home = PathBuf::from("/Users/someone");
    let d = dirs(FINDER_PATH, None, Some(&home));
    for want in [
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        home.join(".local/bin"),
        home.join(".npm-global/bin"),
        home.join(".volta/bin"),
    ] {
        assert!(d.contains(&want), "{want:?} missing from {d:?}");
    }
    // The launch PATH stays first and in its own order: an explicit choice wins.
    assert_eq!(&d[..4], &["/usr/bin", "/bin", "/usr/sbin", "/sbin"].map(PathBuf::from));
}

#[test]
fn mac05_login_shell_path_comes_after_launch_path_and_before_fixed_folders() {
    let d = dirs("/usr/bin:/bin", Some("/custom/node/bin:/usr/bin"), None);
    let pos = |p: &str| d.iter().position(|x| x == Path::new(p)).unwrap();
    assert!(pos("/bin") < pos("/custom/node/bin"));
    assert!(pos("/custom/node/bin") < pos("/opt/homebrew/bin"));
    assert_eq!(d.iter().filter(|x| *x == Path::new("/usr/bin")).count(), 1, "duplicates removed: {d:?}");
}

#[test]
fn mac05_relative_and_empty_entries_are_dropped() {
    let launch = std::env::join_paths(["", ".", "bin", "/usr/bin"]).unwrap();
    let d = cli::unix_search_dirs(&launch, None, None);
    assert!(d.iter().all(|p| p.has_root()), "relative entry kept: {d:?}");
}

#[test]
fn mac05_nvm_versions_are_searched_newest_name_first() {
    let home = tempfile::tempdir().unwrap();
    // v9 sorts after v22 as a number, although "v9" > "v22" as text (review finding).
    for v in ["v9.11.2", "v20.11.0", "v22.23.4", "v22.3.0", "system"] {
        std::fs::create_dir_all(home.path().join(".nvm/versions/node").join(v).join("bin")).unwrap();
    }
    let d = cli::unix_search_dirs(OsStr::new(""), None, Some(home.path()));
    let order: Vec<String> = d
        .iter()
        .filter(|p| p.to_string_lossy().contains(".nvm"))
        .map(|p| p.parent().unwrap().file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(order, ["v22.23.4", "v22.3.0", "v20.11.0", "v9.11.2", "system"]);
}

#[test]
fn mac05_login_path_is_read_after_the_marker_despite_banners() {
    let out = "Welcome!\n\n__SYMVOULIO_LOGIN_PATH__=/opt/homebrew/bin:/usr/bin\n";
    assert_eq!(cli::parse_login_path(out).unwrap(), "/opt/homebrew/bin:/usr/bin");
    assert!(cli::parse_login_path("no marker here\n").is_none());
    assert!(cli::parse_login_path("__SYMVOULIO_LOGIN_PATH__=\n").is_none());
}

#[test]
fn f1_codex_standalone_package_folder_is_searched_after_local_bin() {
    let home = PathBuf::from("/Users/someone");
    let d = dirs(FINDER_PATH, None, Some(&home));
    let pos = |p: PathBuf| d.iter().position(|x| *x == p).unwrap_or_else(|| panic!("{p:?} missing from {d:?}"));
    // https://chatgpt.com/codex/install.sh (read 2026-11-08): ~/.local/bin/codex -> ~/.codex/packages/standalone/current/bin/codex
    assert!(pos(home.join(".local/bin")) < pos(home.join(".codex/packages/standalone/current/bin")));
}

#[cfg(windows)]
#[test]
fn mac02_windows_child_processes_keep_the_inherited_path() {
    let r = cli::ResolvedCli { program: "C:/x/claude.exe".into(), prefix_args: vec![], shim: None };
    assert!(cli::child_path(&r).is_none(), "Windows behaviour must stay unchanged");
}

#[cfg(unix)]
mod finder_launch {
    use super::*;
    use std::{
        env, fs,
        os::unix::fs::PermissionsExt,
        sync::Mutex,
        time::{Duration, Instant},
    };
    use symvoulio_core::{detect, process, Provider};

    static ENV: Mutex<()> = Mutex::new(());
    const KEYS: [&str; 7] = ["PATH", "HOME", "SYMVOULIO_FAKE_CLI", "SHELL", "ZDOTDIR", "CODEX_HOME", "CODEX_INSTALL_DIR"];

    struct Finder {
        _guard: std::sync::MutexGuard<'static, ()>,
        saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
        home: tempfile::TempDir,
    }
    impl Finder {
        /// The environment of an app opened from Finder: launchd's PATH and a home with no shell set-up.
        fn new() -> Self {
            let guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
            let saved = KEYS.iter().map(|k| (*k, env::var_os(k))).collect();
            let home = tempfile::tempdir().unwrap();
            env::remove_var("SYMVOULIO_FAKE_CLI");
            env::remove_var("CODEX_HOME");
            env::remove_var("CODEX_INSTALL_DIR");
            // The login-shell probe must not read the real user's zsh start-up files (review finding): no ZDOTDIR,
            // and /bin/sh as the login shell, which reads only /etc/profile and ~/.profile of the empty home.
            env::remove_var("ZDOTDIR");
            env::set_var("SHELL", "/bin/sh");
            env::set_var("PATH", FINDER_PATH);
            env::set_var("HOME", home.path());
            Self { _guard: guard, saved, home }
        }
    }
    impl Drop for Finder {
        fn drop(&mut self) {
            for (k, v) in &self.saved {
                match v {
                    Some(v) => env::set_var(k, v),
                    None => env::remove_var(k),
                }
            }
        }
    }
    fn script(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    /// A real CLI in a shared folder would be found first; the stand-in test is then meaningless here.
    fn real_cli_present(name: &str) -> bool {
        ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"].iter().any(|d| Path::new(d).join(name).exists())
    }

    #[test]
    fn mac05_mac07_npm_codex_with_env_node_is_detected_from_a_finder_launch() {
        if real_cli_present("codex") {
            eprintln!("skipped: a real codex is installed in a shared folder");
            return;
        }
        let f = Finder::new();
        let bin = f.home.path().join(".npm-global/bin");
        // npm writes `#!/usr/bin/env node`; the interpreter lives next to the CLI, not on launchd's PATH.
        script(&bin.join("codex"), "#!/usr/bin/env symvoulio-test-node\n");
        script(
            &bin.join("symvoulio-test-node"),
            "#!/bin/sh\nshift\ncase \"$1\" in\n  --version) echo 'codex-cli 0.98.0' ;;\n  login) echo 'Logged in using ChatGPT' ;;\n  *) exit 2 ;;\nesac\n",
        );
        let d = detect::detect(Provider::Codex);
        assert!(d.installed, "codex in ~/.npm-global/bin not detected: {d:?}");
        assert_eq!(d.version.as_deref(), Some("codex-cli 0.98.0"), "{d:?}");
        assert_eq!(d.connected, Some(true), "{d:?}");
        assert_eq!(d.method, Some(detect::AuthMethod::Subscription), "{d:?}");
    }

    #[test]
    fn mac07_native_claude_in_local_bin_is_detected_and_signed_in() {
        if real_cli_present("claude") {
            eprintln!("skipped: a real claude is installed in a shared folder");
            return;
        }
        let f = Finder::new();
        script(
            &f.home.path().join(".local/bin/claude"),
            "#!/bin/sh\ncase \"$1\" in\n  --version) echo '2.1.200 (Claude Code)' ;;\n  auth) echo '{\"loggedIn\":true,\"authMethod\":\"claude.ai\"}' ;;\n  *) exit 2 ;;\nesac\n",
        );
        let d = detect::detect(Provider::Claude);
        assert!(d.installed, "{d:?}");
        assert_eq!(d.version.as_deref(), Some("2.1.200 (Claude Code)"), "{d:?}");
        assert_eq!(d.connected, Some(true), "{d:?}");
        assert_eq!(d.method, Some(detect::AuthMethod::Subscription), "{d:?}");
    }

    #[test]
    fn mac07_installed_but_signed_out_claude_is_not_connected() {
        if real_cli_present("claude") {
            eprintln!("skipped: a real claude is installed in a shared folder");
            return;
        }
        let f = Finder::new();
        script(
            &f.home.path().join(".local/bin/claude"),
            "#!/bin/sh\ncase \"$1\" in\n  --version) echo '2.1.200 (Claude Code)' ;;\n  auth) echo '{\"loggedIn\":false,\"authMethod\":\"none\"}'; exit 1 ;;\nesac\n",
        );
        let d = detect::detect(Provider::Claude);
        assert!(d.installed);
        assert_eq!(d.connected, Some(false), "{d:?}");
    }

    #[test]
    fn mac07_absent_cli_is_reported_not_installed() {
        if real_cli_present("claude") {
            eprintln!("skipped: a real claude is installed in a shared folder");
            return;
        }
        let _f = Finder::new();
        let d = detect::detect(Provider::Claude);
        assert!(!d.installed, "{d:?}");
        assert_eq!(d.connected, None);
    }

    #[test]
    fn f1_cli_installed_while_the_app_runs_is_found_on_check_again() {
        if real_cli_present("claude") {
            eprintln!("skipped: a real claude is installed in a shared folder");
            return;
        }
        let f = Finder::new();
        // First check: nothing installed yet (the user opens the app before installing).
        let before = detect::detect(Provider::Claude);
        assert!(!before.installed, "{before:?}");
        // The official installer then puts the CLI into ~/.local/bin; «Check again» in the same app run finds it.
        script(
            &f.home.path().join(".local/bin/claude"),
            "#!/bin/sh
case \"$1\" in
  --version) echo '2.1.319 (Claude Code)' ;;
  auth) echo '{\"loggedIn\":true,\"authMethod\":\"claude.ai\"}' ;;
esac
",
        );
        let after = detect::detect(Provider::Claude);
        assert!(after.installed, "a CLI installed after start-up must be found without a restart: {after:?}");
        assert_eq!(after.connected, Some(true), "{after:?}");
    }

    #[test]
    fn f1_codex_in_the_standalone_package_is_found_without_the_local_bin_link() {
        if real_cli_present("codex") {
            eprintln!("skipped: a real codex is installed in a shared folder");
            return;
        }
        let f = Finder::new();
        script(
            &f.home.path().join(".codex/packages/standalone/current/bin/codex"),
            "#!/bin/sh
case \"$1\" in
  --version) echo 'codex-cli 0.167.0' ;;
  login) echo 'Logged in using ChatGPT' ;;
esac
",
        );
        let d = detect::detect(Provider::Codex);
        assert!(d.installed, "{d:?}");
        assert_eq!(d.connected, Some(true), "{d:?}");
    }

    #[test]
    fn f1_child_path_on_a_finder_launch_contains_the_installer_folders() {
        let f = Finder::new();
        let r = cli::ResolvedCli { program: f.home.path().join(".local/bin/claude"), prefix_args: vec![], shim: None };
        let path = cli::child_path(&r).expect("PATH for provider processes on macOS");
        let d: Vec<PathBuf> = env::split_paths(&path).collect();
        assert_eq!(d[0], f.home.path().join(".local/bin"), "{d:?}");
        assert!(d.contains(&f.home.path().join(".codex/packages/standalone/current/bin")), "{d:?}");
        for launchd in ["/usr/bin", "/bin", "/usr/sbin", "/sbin"] {
            assert!(d.contains(&PathBuf::from(launchd)), "{launchd} missing: {d:?}");
        }
        // The panel PATH carries the same folders, so a CLI installed into them runs in the panel.
        let panel: Vec<PathBuf> = env::split_paths(&cli::panel_path()).collect();
        assert!(panel.contains(&f.home.path().join(".local/bin")), "{panel:?}");
    }

    #[test]
    fn mac05_child_process_path_contains_homebrew_and_the_cli_folder() {
        let f = Finder::new();
        let program = f.home.path().join("tools/codex");
        let r = cli::ResolvedCli { program: program.clone(), prefix_args: vec![], shim: None };
        let cmd = process::command(&r);
        let path = cmd
            .get_envs()
            .find(|(k, _)| *k == "PATH")
            .and_then(|(_, v)| v)
            .expect("PATH is set for provider processes")
            .to_owned();
        let d: Vec<PathBuf> = env::split_paths(&path).collect();
        assert_eq!(d[0], f.home.path().join("tools"), "the CLI's own folder comes first: {d:?}");
        assert!(d.contains(&PathBuf::from("/opt/homebrew/bin")), "{d:?}");
        assert!(d.contains(&PathBuf::from("/usr/local/bin")), "{d:?}");
    }

    fn alive(pid: &str) -> bool {
        std::process::Command::new("kill").args(["-0", pid]).stderr(std::process::Stdio::null()).status().unwrap().success()
    }

    #[test]
    fn mac05_a_login_shell_that_hangs_is_abandoned_and_its_group_killed() {
        let dir = tempfile::tempdir().unwrap();
        let shell = dir.path().join("slow-shell");
        let pidfile = dir.path().join("child.pid");
        // A start-up file that starts a child and waits for it: the probe must kill both (review finding).
        script(&shell, &format!("#!/bin/sh\nsleep 30 &\necho $! > '{}'\nwait\n", pidfile.display()));
        let start = Instant::now();
        assert!(cli::probe_login_path(&shell, Duration::from_millis(700)).is_none());
        assert!(start.elapsed() < Duration::from_secs(5), "probe waited {:?}", start.elapsed());
        let pid = fs::read_to_string(&pidfile).unwrap().trim().to_owned();
        let gone = Instant::now() + Duration::from_secs(3);
        while alive(&pid) && Instant::now() < gone {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!alive(&pid), "the start-up file's child {pid} survived the probe deadline");
    }

    #[test]
    fn mac05_a_background_program_holding_the_pipe_does_not_block_the_probe() {
        let dir = tempfile::tempdir().unwrap();
        let shell = dir.path().join("daemon-shell");
        let pidfile = dir.path().join("daemon.pid");
        // The shell prints the marker and exits; a background program keeps stdout open (review finding).
        script(
            &shell,
            &format!("#!/bin/sh\nsleep 30 &\necho $! > '{}'\nprintf '\\n__SYMVOULIO_LOGIN_PATH__=/held/open\\n'\nexit 0\n", pidfile.display()),
        );
        let start = Instant::now();
        let path = cli::probe_login_path(&shell, Duration::from_secs(10));
        let waited = start.elapsed();
        let pid = fs::read_to_string(&pidfile).unwrap().trim().to_owned();
        let _ = std::process::Command::new("kill").arg(&pid).status();
        assert_eq!(path.as_deref(), Some(OsStr::new("/held/open")));
        assert!(waited < Duration::from_secs(3), "probe waited {waited:?} for a pipe held by a background program");
    }

    #[test]
    fn mac05_child_path_of_an_nvm_cli_starts_with_its_own_node_folder() {
        let f = Finder::new();
        let bin = f.home.path().join(".nvm/versions/node/v22.23.4/bin");
        for v in ["v9.11.2", "v22.23.4"] {
            fs::create_dir_all(f.home.path().join(".nvm/versions/node").join(v).join("bin")).unwrap();
        }
        let r = cli::ResolvedCli { program: bin.join("codex"), prefix_args: vec![], shim: None };
        let path = cli::child_path(&r).expect("PATH for provider processes");
        assert_eq!(env::split_paths(&path).next().unwrap(), bin, "node next to the CLI must win");
    }

    #[test]
    fn mac05_login_shell_banner_does_not_corrupt_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let shell = dir.path().join("chatty-shell");
        // A startup file that prints a banner; the probe's own command runs after it.
        script(&shell, "#!/bin/sh\necho 'Last login: today'\nshift\nshift\nPATH=/from/login:/usr/bin\nexport PATH\nexec /bin/sh -c \"$1\"\n");
        let path = cli::probe_login_path(&shell, Duration::from_secs(5)).expect("PATH printed");
        assert_eq!(path, "/from/login:/usr/bin");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac05_real_zsh_login_shell_reports_a_path() {
        let path = cli::probe_login_path(Path::new("/bin/zsh"), Duration::from_secs(10)).expect("zsh -l printed PATH");
        assert!(env::split_paths(&path).any(|d| d == Path::new("/usr/bin")), "{path:?}");
    }
}
