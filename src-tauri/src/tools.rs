//! Running the external tools: ffmpeg / ffprobe and ImageMagick.
//!
//! Release builds ship them in a `tools` resource folder (`tools/ffmpeg`, `tools/ffprobe`,
//! and ImageMagick as `tools/magick/AppRun` on Linux or `tools/magick/magick.exe` on Windows).
//! Without that folder, as in development, they are looked up on the PATH.

use anyhow::{bail, Context, Result};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

static BUNDLED_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Uses the tools in `dir` when it exists (called once at startup with the resource folder).
pub fn set_bundled_dir(dir: PathBuf) {
    if dir.is_dir() {
        let _ = BUNDLED_DIR.set(dir);
    }
}

fn program(name: &str) -> PathBuf {
    if let Some(dir) = BUNDLED_DIR.get() {
        let candidates = if cfg!(windows) {
            [dir.join(format!("{name}.exe")), dir.join(name).join(format!("{name}.exe"))]
        } else {
            [dir.join(name), dir.join(name).join("AppRun")]
        };
        if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
            return found;
        }
    }
    PathBuf::from(name)
}

/// A command for the tool `name`: the bundled copy if there is one, else the one on the PATH.
pub fn command(name: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program(name));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Runs `cmd` and returns its stdout, killing it after `timeout` (corrupt files can hang ffmpeg).
pub fn run(mut cmd: Command, timeout: Duration) -> Result<Vec<u8>> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("cannot start ffmpeg/ffprobe; is ffmpeg installed?")?;

    // Drain both pipes on threads so a full pipe never blocks the child.
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let err = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stderr.read_to_string(&mut buf);
        buf
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("timed out after {}s", timeout.as_secs());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    if !status.success() {
        let line = stderr.lines().find(|l| !l.trim().is_empty()).unwrap_or("unknown error");
        bail!("{line}");
    }
    Ok(stdout)
}
