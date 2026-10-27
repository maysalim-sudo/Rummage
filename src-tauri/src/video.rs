//! A tiny local HTTP server that streams videos to the preview panel's `<video>` player.
//!
//! The webview's media engine (GStreamer on Linux, Media Foundation on Windows) handles
//! plain HTTP with byte ranges reliably, which custom URI schemes don't guarantee.
//! It listens on 127.0.0.1 only and every URL needs a random per-run token, so other
//! programs and web pages cannot read files through it.

use anyhow::Result;
use serde::Serialize;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

#[derive(Clone, Debug, Serialize)]
pub struct MediaServer {
    pub port: u16,
    pub token: String,
}

/// Maps a file id to its absolute path (None if unknown or its drive is unplugged).
pub type Resolve = Arc<dyn Fn(i64) -> Option<PathBuf> + Send + Sync>;

/// Starts the server on a free port; serves `GET /v/<file id>?t=<token>`.
pub fn start(resolve: Resolve) -> Result<MediaServer> {
    let server = Server::http("127.0.0.1:0").map_err(|e| anyhow::anyhow!("cannot start video server: {e}"))?;
    let port = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(0);
    let info = MediaServer { port, token: uuid::Uuid::new_v4().simple().to_string() };
    let token = info.token.clone();
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let resolve = resolve.clone();
            let token = token.clone();
            // Players open several connections at once while seeking.
            std::thread::spawn(move || {
                let _ = handle(request, &resolve, &token);
            });
        }
    });
    Ok(info)
}

fn handle(request: Request, resolve: &Resolve, token: &str) -> std::io::Result<()> {
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
    if !query.split('&').any(|kv| kv == format!("t={token}")) {
        return request.respond(Response::empty(StatusCode(403)));
    }
    let Some(path) = path.strip_prefix("/v/").and_then(|id| id.parse().ok()).and_then(|id| resolve(id)) else {
        return request.respond(Response::empty(StatusCode(404)));
    };
    let Ok(mut file) = File::open(&path) else {
        return request.respond(Response::empty(StatusCode(404)));
    };
    let len = file.metadata()?.len();
    let range = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Range"))
        .and_then(|h| parse_range(h.value.as_str(), len));

    let mut headers = vec![
        header("Content-Type", content_type(&path.to_string_lossy())),
        header("Accept-Ranges", "bytes"),
    ];
    let (status, start, count) = match range {
        Some((start, end)) => {
            headers.push(header("Content-Range", &format!("bytes {start}-{end}/{len}")));
            (206, start, end - start + 1)
        }
        None if request.headers().iter().any(|h| h.field.equiv("Range")) => {
            headers.push(header("Content-Range", &format!("bytes */{len}")));
            return request.respond(Response::new(StatusCode(416), headers, std::io::empty(), Some(0), None));
        }
        None => (200, 0, len),
    };
    if *request.method() == Method::Head {
        return request.respond(Response::new(StatusCode(status), headers, std::io::empty(), Some(count as usize), None));
    }
    file.seek(SeekFrom::Start(start))?;
    request.respond(Response::new(StatusCode(status), headers, file.take(count), Some(count as usize), None))
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

/// Parses a single `bytes=` range into inclusive (start, end) within a file of `len` bytes.
fn parse_range(value: &str, len: u64) -> Option<(u64, u64)> {
    let spec = value.trim().strip_prefix("bytes=")?.split(',').next()?.trim();
    let (start, end) = spec.split_once('-')?;
    let (start, end) = match (start.trim(), end.trim()) {
        ("", suffix) => {
            let n: u64 = suffix.parse().ok()?;
            (len.saturating_sub(n), len.checked_sub(1)?)
        }
        (start, "") => (start.parse().ok()?, len.checked_sub(1)?),
        (start, end) => (start.parse().ok()?, end.parse::<u64>().ok()?.min(len.checked_sub(1)?)),
    };
    (start <= end && start < len).then_some((start, end))
}

fn content_type(path: &str) -> &'static str {
    match crate::paths::ext_lower(path).as_str() {
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "wmv" => "video/x-ms-wmv",
        "ts" | "mts" | "m2ts" => "video/mp2t",
        "mpg" | "mpeg" | "vob" => "video/mpeg",
        "3gp" => "video/3gpp",
        "ogv" => "video/ogg",
        "flv" => "video/x-flv",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpStream;

    #[test]
    fn ranges() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(parse_range("bytes=900-", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=990-2000", 1000), Some((990, 999)));
        assert_eq!(parse_range("bytes=1000-", 1000), None);
        assert_eq!(parse_range("items=0-1", 1000), None);
    }

    fn get(port: u16, path: &str, range: Option<&str>) -> (String, Vec<u8>) {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let range = range.map(|r| format!("Range: {r}\r\n")).unwrap_or_default();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: localhost\r\n{range}Connection: close\r\n\r\n").unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).unwrap();
        let split = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        (String::from_utf8_lossy(&raw[..split]).to_string(), raw[split + 4..].to_vec())
    }

    /// Manual check: a real media client streams and seeks a real video through the server.
    /// `MC_VIDEO=/path/clip.mp4 cargo test stream_real_video -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn stream_real_video() {
        let file = PathBuf::from(std::env::var("MC_VIDEO").unwrap());
        let resolve: Resolve = Arc::new(move |_| Some(file.clone()));
        let server = start(resolve).unwrap();
        let url = format!("http://127.0.0.1:{}/v/1?t={}", server.port, server.token);

        let probe = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "format=duration,format_name", "-of", "compact", &url])
            .output()
            .unwrap();
        println!("ffprobe over HTTP: {}", String::from_utf8_lossy(&probe.stdout).trim());
        assert!(probe.status.success());

        // Seeking to the middle needs working byte ranges.
        let duration: f64 = String::from_utf8_lossy(&probe.stdout)
            .split('|')
            .find_map(|kv| kv.strip_prefix("duration=")?.trim().parse().ok())
            .unwrap();
        let frame = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-ss", &format!("{:.1}", duration / 2.0), "-i", &url])
            .args(["-frames:v", "1", "-f", "image2pipe", "-c:v", "mjpeg", "-"])
            .output()
            .unwrap();
        println!("frame at {:.1}s: {} bytes", duration / 2.0, frame.stdout.len());
        assert!(frame.status.success() && !frame.stdout.is_empty());
    }

    #[test]
    fn serves_ranges_with_token() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("clip.mp4");
        let data: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
        std::fs::write(&file, &data).unwrap();
        let resolve: Resolve = Arc::new(move |id| (id == 7).then(|| file.clone()));
        let server = start(resolve).unwrap();
        let t = &server.token;

        let (head, body) = get(server.port, &format!("/v/7?t={t}"), Some("bytes=100-199"));
        assert!(head.starts_with("HTTP/1.1 206"), "{head}");
        assert!(head.contains("Content-Range: bytes 100-199/5000"));
        assert!(head.contains("Content-Type: video/mp4"));
        assert_eq!(body, &data[100..200]);

        let (head, body) = get(server.port, &format!("/v/7?t={t}"), None);
        assert!(head.starts_with("HTTP/1.1 200"));
        assert_eq!(body.len(), 5000);

        assert!(get(server.port, "/v/7?t=wrong", None).0.starts_with("HTTP/1.1 403"));
        assert!(get(server.port, &format!("/v/8?t={t}"), None).0.starts_with("HTTP/1.1 404"));
    }
}
