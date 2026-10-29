//! The server binary: where it listens, what it refuses to start with, and
//! a clean exit on SIGTERM.

#![cfg(unix)]

use std::{
    io::{BufRead, BufReader},
    net::SocketAddr,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

const BINARY: &str = env!("CARGO_BIN_EXE_splitcard-server");
const SECRET: &str = "5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e";

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("splitcard-process-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn command(database: &Path) -> Command {
    let mut command = Command::new(BINARY);
    command
        .env_clear()
        .env("DATABASE_URL", database)
        .env("PUBLIC_BASE_URL", "https://splitcard.test/api")
        .env("RELAYS", "wss://relay.invalid")
        .env("SERVER_SECRET_KEY", SECRET)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn without_ansi(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Starts the server and returns it with its listening address and a
/// channel of its log lines. The caller waits on the child, in [`terminate`].
#[allow(clippy::zombie_processes)]
fn start(mut command: Command) -> (Child, SocketAddr, mpsc::Receiver<String>) {
    let mut child = command.spawn().expect("server starts");
    let (sender, lines) = mpsc::channel();
    for stream in [
        Box::new(child.stdout.take().expect("stdout")) as Box<dyn std::io::Read + Send>,
        Box::new(child.stderr.take().expect("stderr")),
    ] {
        let sender = sender.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if sender.send(without_ansi(&line)).is_err() {
                    break;
                }
            }
        });
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let line = lines
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("the server logs that it is listening");
        if line.contains("Split Card server listening") {
            let addr = line
                .split_whitespace()
                .find_map(|word| word.strip_prefix("bind_addr="))
                .expect("bind_addr in the log line")
                .parse()
                .expect("a socket address");
            return (child, addr, lines);
        }
    }
}

fn health(addr: SocketAddr) -> String {
    let port = addr.port();
    let output = Command::new("curl")
        .args(["-s", &format!("http://127.0.0.1:{port}/health")])
        .output()
        .expect("curl runs");
    String::from_utf8(output.stdout).expect("utf-8")
}

/// Sends SIGTERM and waits for the exit, collecting the remaining log lines.
fn terminate(mut child: Child, lines: &mpsc::Receiver<String>) -> (bool, Vec<String>, Duration) {
    let started = Instant::now();
    let status = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("kill runs");
    assert!(status.success());
    let exit = loop {
        if let Some(exit) = child.try_wait().expect("wait") {
            break exit;
        }
        assert!(
            started.elapsed() < Duration::from_secs(25),
            "the server did not exit within 25 s of SIGTERM"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    let elapsed = started.elapsed();
    // The readers end at EOF, closing the channel once all output is read.
    let mut rest = Vec::new();
    while let Ok(line) = lines.recv_timeout(Duration::from_secs(5)) {
        rest.push(line);
    }
    (exit.success(), rest, elapsed)
}

#[test]
fn port_listens_on_every_interface_and_sigterm_shuts_down_cleanly() {
    let dir = temp_dir("port");
    let mut command = command(&dir.join("splitcard.db"));
    command.env("PORT", "0").env("BIND_ADDR", "127.0.0.1:1");
    let (child, addr, lines) = start(command);
    assert!(addr.ip().is_unspecified(), "{addr}");
    assert_eq!(health(addr), r#"{"status":"ok"}"#);

    let (success, logs, elapsed) = terminate(child, &lines);
    let logs = logs.join("\n");
    assert!(success, "{logs}");
    assert!(elapsed < Duration::from_secs(20), "{elapsed:?}");
    for line in [
        "shutdown requested",
        "relay outbox stopped",
        "timestamp submitter stopped",
        "timestamp upgrader stopped",
        "shut down cleanly",
    ] {
        assert!(logs.contains(line), "{line} not in {logs}");
    }
    std::fs::remove_dir_all(&dir).expect("clean up");
}

#[test]
fn without_port_it_listens_on_bind_addr() {
    let dir = temp_dir("bind");
    let mut command = command(&dir.join("splitcard.db"));
    command.env("BIND_ADDR", "127.0.0.1:0");
    let (child, addr, lines) = start(command);
    assert_eq!(addr.ip().to_string(), "127.0.0.1");
    assert_eq!(health(addr), r#"{"status":"ok"}"#);
    let (success, _, _) = terminate(child, &lines);
    assert!(success);
    std::fs::remove_dir_all(&dir).expect("clean up");
}

#[test]
fn it_refuses_to_start_without_a_usable_database_directory_or_https() {
    let dir = temp_dir("refuse");
    let missing = dir.join("no-such-disk/splitcard.db");
    let output = command(&missing).output().expect("runs");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not exist"), "{stderr}");
    assert!(!missing.parent().expect("parent").exists());

    let database = dir.join("splitcard.db");
    let mut insecure = command(&database);
    insecure.env("PUBLIC_BASE_URL", "http://localhost:3000/api");
    let output = insecure.output().expect("runs");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("must be https"), "{stderr}");
    assert!(!stderr.contains(SECRET));

    // The development flag allows http for local runs.
    let mut development = command(&database);
    development
        .env("PUBLIC_BASE_URL", "http://localhost:3000/api")
        .env("SPLITCARD_DEV", "1")
        .env("BIND_ADDR", "127.0.0.1:0");
    let (child, _, lines) = start(development);
    let (success, _, _) = terminate(child, &lines);
    assert!(success);
    std::fs::remove_dir_all(&dir).expect("clean up");
}
