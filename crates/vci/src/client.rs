//! The app's side: the helper (`pts-vci32`) started in the background when it's first needed, and asked things one
//! at a time. If it stops (or never started), the next request says so and the one after starts it again.

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use crate::proto::{PROGRAM, Reply, Request};

struct Running {
    child: Child,
    stdin: ChildStdin,
    out: Lines<BufReader<ChildStdout>>,
}

/// The helper (shared: one at a time asks it)
#[derive(Clone, Default)]
pub struct Bridge {
    running: Arc<Mutex<Option<Running>>>,
}

impl std::fmt::Debug for Bridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Bridge")
    }
}

/// The helper's program: beside the app's (installed together). While developing, the 32-bit build in `target\`
/// (`cargo build -p vci-bridge --target i686-pc-windows-msvc`) is found too: only that one can load Ford's drivers.
pub fn program() -> PathBuf {
    let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) else {
        return PathBuf::from(PROGRAM);
    };
    let beside = dir.join(PROGRAM);
    if beside.exists() {
        return beside;
    }
    // target\debug\pts-workbench-app.exe → target\i686-pc-windows-msvc\{debug,release}\pts-vci32.exe
    if let Some(target) = dir.parent() {
        for build in ["debug", "release"] {
            let p = target.join("i686-pc-windows-msvc").join(build).join(PROGRAM);
            if p.exists() {
                return p;
            }
        }
    }
    beside
}

fn start() -> Result<Running, String> {
    let path = program();
    if !path.exists() {
        return Err(format!("{PROGRAM} isn't installed beside the app"));
    }
    let mut cmd = Command::new(&path);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        // no console window for it
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| format!("Couldn't start {PROGRAM}: {e}"))?;
    let stdin = child.stdin.take().ok_or("no input for the helper")?;
    let stdout = child.stdout.take().ok_or("no output from the helper")?;
    Ok(Running {
        child,
        stdin,
        out: BufReader::new(stdout).lines(),
    })
}

impl Bridge {
    /// Asks the helper something, and its answer (its own errors as `Reply::Error`; the helper not working as Err)
    pub async fn call(&self, req: Request) -> Result<Reply, String> {
        let mut guard = self.running.lock().await;
        if guard.is_none() {
            *guard = Some(start()?);
        }
        let limit = Duration::from_millis(req.time_limit_ms());
        let line = serde_json::to_string(&req).map_err(|e| e.to_string())? + "\n";
        let r = guard.as_mut().expect("started");
        let asked = async {
            r.stdin.write_all(line.as_bytes()).await?;
            r.stdin.flush().await?;
            r.out.next_line().await
        };
        let got = tokio::time::timeout(limit, asked).await;
        match got {
            Ok(Ok(Some(text))) => serde_json::from_str(&text).map_err(|e| format!("The helper's answer wasn't readable ({e})")),
            Ok(Ok(None)) => {
                *guard = None;
                Err("The VCI helper stopped".into())
            }
            Ok(Err(e)) => {
                *guard = None;
                Err(format!("The VCI helper stopped ({e})"))
            }
            Err(_) => {
                // stuck (a driver call that never came back): stopped, started again next time
                if let Some(mut r) = guard.take() {
                    let _ = r.child.start_kill();
                }
                Err("The VCI didn't answer in time".into())
            }
        }
    }

    /// Stops the helper (the VCI closed first, so it isn't left in use)
    pub async fn stop(&self) {
        let mut guard = self.running.lock().await;
        if let Some(mut r) = guard.take() {
            let line = serde_json::to_string(&Request::Close).unwrap_or_default() + "\n";
            let _ = r.stdin.write_all(line.as_bytes()).await;
            let _ = r.stdin.flush().await;
            let _ = tokio::time::timeout(Duration::from_secs(3), r.out.next_line()).await;
            let _ = r.child.start_kill();
        }
    }
}
