//! Runs the bundled game server (dungerra-api in desktop mode) as a child
//! process. The contract is documented in dungerra-api's
//! docs/decisions/desktop-version.md ("Sidecar contract").

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

/// Prefix of the one stdout line the server prints once it accepts connections.
const READY_PREFIX: &str = "DUNGERRA_READY ";
/// The first start creates and migrates the database, so allow some time.
const READY_TIMEOUT: Duration = Duration::from_secs(30);
/// The server saves every active session before it exits.
const STOP_TIMEOUT: Duration = Duration::from_secs(15);
/// Server log lines kept for error messages shown in the launcher.
const LOG_TAIL_LINES: usize = 30;
/// The server writes this when it cannot bind its port (dungerra-api main.go).
const BIND_FAILURE_MARKER: &str = "Cannot listen on";

/// Environment the server must not inherit: a stray MongoDB URL or secret in
/// the user's environment would silently change where data goes.
const SCRUBBED_ENV: &[&str] = &[
    "DB_DRIVER",
    "MONGODB_URL",
    "SQLITE_PATH",
    "SECRET_KEY",
    "HOST",
    "PORT",
    "USE_TLS",
    "TLS_CERT",
    "TLS_KEY",
    "GOOGLE_CLIENT_ID",
    "GOOGLE_CLIENT_SECRET",
    "ENGINE_DEBUG_MODE",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Reachable from this computer only.
    Solo,
    /// Reachable from the local network.
    Host,
}

impl Mode {
    fn bind_host(self) -> &'static str {
        match self {
            Mode::Solo => "127.0.0.1",
            Mode::Host => "0.0.0.0",
        }
    }
}

/// Where the server binary, its data and the web client live.
pub struct Paths {
    pub binary: PathBuf,
    pub data_dir: PathBuf,
    pub web_dir: PathBuf,
}

#[derive(Debug)]
pub struct StartError {
    pub message: String,
    /// The port was taken; the caller may retry with another one.
    pub port_in_use: bool,
}

struct Running {
    child: Child,
    /// Held open for the server's lifetime; dropping it asks the server to stop.
    stdin: Option<ChildStdin>,
}

#[derive(Default)]
pub struct ServerManager {
    running: Mutex<Option<Running>>,
}

#[derive(Deserialize)]
struct ReadyInfo {
    addr: String,
}

/// Extracts the port from a ready line's payload, e.g. `{"addr":"0.0.0.0:8080"}`.
fn parse_ready_port(payload: &str) -> Option<u16> {
    let info: ReadyInfo = serde_json::from_str(payload).ok()?;
    info.addr.rsplit_once(':')?.1.parse().ok()
}

impl ServerManager {
    /// Stops any running server, then starts one and waits until it accepts
    /// connections. Returns the bound port (useful when `port` is 0).
    pub fn start(&self, paths: &Paths, mode: Mode, port: u16) -> Result<u16, StartError> {
        self.stop();

        let fail = |message: String| StartError {
            message,
            port_in_use: false,
        };
        std::fs::create_dir_all(&paths.data_dir)
            .map_err(|e| fail(format!("Cannot create {}: {e}", paths.data_dir.display())))?;

        let mut cmd = Command::new(&paths.binary);
        for key in SCRUBBED_ENV {
            cmd.env_remove(key);
        }
        cmd.args([
            "-host",
            mode.bind_host(),
            "-port",
            &port.to_string(),
            "-ready-line",
            "-shutdown-on-stdin-close",
            "-no-dotenv",
        ])
        .current_dir(&paths.data_dir)
        .env("DATA_DIR", &paths.data_dir)
        .env("STATIC_DIR", &paths.web_dir)
        .env("LOCAL_AUTH_ENABLED", "true")
        .env("GOOGLE_AUTH_ENABLED", "false")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000; // no console window for the server
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| fail(format!("Cannot start {}: {e}", paths.binary.display())))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");

        // The server logs to stderr: keep the latest run in server.log and the
        // last lines in memory for error messages.
        let tail = Arc::new(Mutex::new(VecDeque::with_capacity(LOG_TAIL_LINES)));
        let mut log_file = File::create(paths.data_dir.join("server.log")).ok();
        let stderr_tail = Arc::clone(&tail);
        let stderr_thread = thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if let Some(file) = log_file.as_mut() {
                    let _ = writeln!(file, "{line}");
                }
                eprintln!("[server] {line}");
                let mut tail = stderr_tail.lock().unwrap();
                if tail.len() == LOG_TAIL_LINES {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
        });

        // The sender is dropped when stdout closes (the server exited), which
        // ends the wait below early.
        let (ready_tx, ready_rx) = mpsc::channel::<String>();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(payload) = line.strip_prefix(READY_PREFIX) {
                    let _ = ready_tx.send(payload.to_string());
                }
            }
        });

        let ready = ready_rx.recv_timeout(READY_TIMEOUT);
        if let Ok(payload) = &ready {
            if let Some(bound_port) = parse_ready_port(payload) {
                *self.running.lock().unwrap() = Some(Running { child, stdin });
                return Ok(bound_port);
            }
        }

        // Not ready: make sure it is gone, then report what it logged
        let _ = child.kill();
        let _ = child.wait();
        let _ = stderr_thread.join(); // stderr is closed now, so this returns
        let log = tail
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        let reason = match ready {
            Ok(payload) => format!("unexpected ready line {payload:?}"),
            Err(mpsc::RecvTimeoutError::Timeout) => "it did not become ready in time".into(),
            Err(mpsc::RecvTimeoutError::Disconnected) => "it exited".into(),
        };
        Err(StartError {
            port_in_use: log.contains(BIND_FAILURE_MARKER),
            message: format!("The game server failed to start: {reason}.\n\n{log}"),
        })
    }

    /// Asks the server to save and exit by closing its stdin, and kills it if
    /// it does not exit in time. Does nothing when no server runs.
    pub fn stop(&self) {
        let Some(mut running) = self.running.lock().unwrap().take() else {
            return;
        };
        drop(running.stdin.take());
        let deadline = Instant::now() + STOP_TIMEOUT;
        loop {
            match running.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
                _ => break,
            }
        }
        eprintln!("[desktop] server did not stop in time, killing it");
        let _ = running.child.kill();
        let _ = running.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ready_port() {
        assert_eq!(
            parse_ready_port(r#"{"addr":"127.0.0.1:53123"}"#),
            Some(53123)
        );
        assert_eq!(parse_ready_port(r#"{"addr":"0.0.0.0:8080"}"#), Some(8080));
        assert_eq!(parse_ready_port(r#"{"addr":"[::]:8080"}"#), Some(8080));
        assert_eq!(parse_ready_port(r#"{"addr":"nonsense"}"#), None);
        assert_eq!(parse_ready_port("not json"), None);
    }
}
