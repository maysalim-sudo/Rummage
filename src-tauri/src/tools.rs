//! Running the external tools: ffmpeg / ffprobe and ImageMagick.
//!
//! Release builds ship them in a `tools` resource folder (`tools/ffmpeg`, `tools/ffprobe`,
//! and ImageMagick as `tools/magick.tar.gz` holding its AppRun on Linux, or
//! `tools/magick/magick.exe` on Windows).
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

/// Unpacks `tools/magick.tar.gz` into the cache folder once per app version and returns
/// the folder. The AppImage ships ImageMagick packed: unpacked, the AppImage packager
/// would copy ImageMagick's own (older) libraries over the app's and break its startup.
fn unpacked_magick(dir: &std::path::Path) -> Option<PathBuf> {
    static UNPACKED: OnceLock<Option<PathBuf>> = OnceLock::new();
    UNPACKED
        .get_or_init(|| {
            let archive = dir.join("magick.tar.gz");
            if !archive.is_file() {
                return None;
            }
            let base = dirs::cache_dir()?.join("rummage").join("tools");
            let dest = base.join(format!("magick-{}", env!("CARGO_PKG_VERSION")));
            if dest.join("AppRun").is_file() {
                return Some(dest);
            }
            let partial = base.join(format!("magick-{}.partial", env!("CARGO_PKG_VERSION")));
            let _ = std::fs::remove_dir_all(&partial);
            std::fs::create_dir_all(&partial).ok()?;
            let unpacked = Command::new("tar")
                .arg("-xzf")
                .arg(&archive)
                .arg("-C")
                .arg(&partial)
                .status()
                .is_ok_and(|s| s.success());
            if !unpacked || std::fs::rename(&partial, &dest).is_err() {
                eprintln!("cannot unpack the bundled ImageMagick into {}", base.display());
                let _ = std::fs::remove_dir_all(&partial);
                return None;
            }
            // Copies left by earlier versions of the app.
            for old in std::fs::read_dir(&base).into_iter().flatten().flatten() {
                if old.path() != dest && old.file_name().to_string_lossy().starts_with("magick-") {
                    let _ = std::fs::remove_dir_all(old.path());
                }
            }
            Some(dest)
        })
        .clone()
}

fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
}

fn program(name: &str) -> PathBuf {
    if let Some(dir) = BUNDLED_DIR.get() {
        // On Linux an installed ImageMagick wins: the official Linux build is made on an
        // older Ubuntu whose libheif can't read recent iPhone HEIC photos.
        if name == "magick" && !cfg!(windows) && on_path("magick") {
            return PathBuf::from(name);
        }
        if name == "magick" {
            if let Some(magick) = unpacked_magick(dir) {
                return magick.join("AppRun");
            }
        }
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
