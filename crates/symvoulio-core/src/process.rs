// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
//! Shared child creation, streaming, deadlines, and process-tree cancellation.
use crate::{
    cli::ResolvedCli,
    provider::{CallError, CancelToken, StreamLine},
};
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, Command, Stdio},
    sync::{atomic::Ordering, mpsc},
    thread,
    time::{Duration, Instant},
};
pub fn command(cli: &ResolvedCli) -> Command {
    let mut cmd = Command::new(&cli.program);
    cmd.args(&cli.prefix_args);
    hide(&mut cmd);
    cmd
}
fn hide(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}
fn kill(child: &mut Child) {
    #[cfg(unix)]
    {
        // Every provider is launched as its own group; negative pid targets all descendants.
        unsafe {
            libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        let mut cmd = Command::new("taskkill");
        hide(&mut cmd);
        if let Ok(mut taskkill) = cmd
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            let deadline = Instant::now() + Duration::from_millis(500);
            while matches!(taskkill.try_wait(), Ok(None)) && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            let _ = taskkill.kill();
            let _ = taskkill.wait();
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}
enum Event {
    Line(bool, String),
    Done,
    Error(String),
}
fn reader(
    pipe: impl Read + Send + 'static,
    stderr: bool,
    tx: mpsc::Sender<Event>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(pipe).lines() {
            match line {
                Ok(s) => {
                    if tx.send(Event::Line(stderr, s)).is_err() {
                        return;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Event::Error(e.to_string()));
                    break;
                }
            }
        }
        let _ = tx.send(Event::Done);
    })
}
pub struct Captured {
    pub stdout: Vec<String>,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_s: f64,
}
pub fn run(
    mut cmd: Command,
    prompt: String,
    timeout: Duration,
    cancel: &CancelToken,
    sink: &mut dyn FnMut(StreamLine),
) -> Result<Captured, CallError> {
    if cancel.load(Ordering::SeqCst) {
        return Err(CallError::Cancelled);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000 | 0x00000004); // hidden and suspended until Job assignment
    }
    let start = Instant::now();
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| CallError::Io(e.to_string()))?;
    #[cfg(windows)]
    let job = match job::OwnedJob::attach(&child) {
        Ok(job) => job,
        Err(e) => {
            kill(&mut child);
            return Err(CallError::Io(e.to_string()));
        }
    };
    let mut stdin = child.stdin.take().expect("piped stdin");
    let writer = thread::spawn(move || {
        let result = stdin.write_all(prompt.as_bytes());
        drop(stdin);
        result
    });
    let (tx, rx) = mpsc::channel();
    let out = reader(child.stdout.take().expect("stdout"), false, tx.clone());
    let err = reader(child.stderr.take().expect("stderr"), true, tx);
    let mut stdout = vec![];
    let mut stderr = String::new();
    let mut done = 0;
    let result = loop {
        if cancel.load(Ordering::SeqCst) {
            kill(&mut child);
            break Err(CallError::Cancelled);
        }
        if start.elapsed() >= timeout {
            kill(&mut child);
            break Err(CallError::Timeout);
        }
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Event::Line(is_stderr, raw)) => {
                sink(StreamLine {
                    raw: raw.clone(),
                    readable: None,
                    is_stderr,
                });
                if is_stderr {
                    stderr.push_str(&raw);
                    stderr.push('\n');
                } else {
                    stdout.push(raw);
                }
            }
            Ok(Event::Done) => done += 1,
            Ok(Event::Error(e)) => {
                kill(&mut child);
                break Err(CallError::Io(e));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => (),
            Err(mpsc::RecvTimeoutError::Disconnected) => done = 2,
        }
        if done == 2 {
            match child.try_wait() {
                Ok(Some(status)) => {
                    break Ok(Captured {
                        stdout,
                        stderr,
                        exit_code: status.code(),
                        duration_s: start.elapsed().as_secs_f64(),
                    })
                }
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(e) => {
                    kill(&mut child);
                    break Err(CallError::Io(e.to_string()));
                }
            }
        }
    };
    #[cfg(windows)]
    drop(job); // closes the owned tree before joining pipe readers
    drop(rx);
    // A detached descendant may retain a pipe even after its parent has exited.
    // Share one deadline across all handles, then drop unfinished handles to detach.
    let deadline = Instant::now() + Duration::from_millis(1500);
    while !(out.is_finished() && err.is_finished() && writer.is_finished())
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(10));
    }
    if out.is_finished() {
        let _ = out.join();
    }
    if err.is_finished() {
        let _ = err.join();
    }
    if writer.is_finished() {
        let _ = writer.join();
    }
    result
}

#[cfg(windows)]
mod job {
    use std::{io, mem::size_of, os::windows::io::AsRawHandle, process::Child};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
        System::{Diagnostics::ToolHelp::*, JobObjects::*, Threading::*},
    };
    pub struct OwnedJob(HANDLE);
    impl Drop for OwnedJob {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    impl OwnedJob {
        pub fn attach(child: &Child) -> io::Result<Self> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let job = Self(handle);
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                if SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const _,
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                ) == 0
                    || AssignProcessToJobObject(handle, child.as_raw_handle() as HANDLE) == 0
                {
                    return Err(io::Error::last_os_error());
                }
                // std::process does not expose the primary thread. It is still suspended,
                // so enumerate its sole thread and resume only after tree ownership is secured.
                let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
                if snapshot == INVALID_HANDLE_VALUE {
                    return Err(io::Error::last_os_error());
                }
                let mut entry: THREADENTRY32 = std::mem::zeroed();
                entry.dwSize = size_of::<THREADENTRY32>() as u32;
                let mut found = false;
                let mut more = Thread32First(snapshot, &mut entry);
                while more != 0 {
                    if entry.th32OwnerProcessID == child.id() {
                        let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                        if !thread.is_null() {
                            found = ResumeThread(thread) != u32::MAX;
                            CloseHandle(thread);
                        }
                        break;
                    }
                    more = Thread32Next(snapshot, &mut entry);
                }
                CloseHandle(snapshot);
                if !found {
                    return Err(io::Error::other("Cannot resume owned provider"));
                }
                Ok(job)
            }
        }
    }
}
