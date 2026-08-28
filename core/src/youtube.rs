use std::{fs, path::Path};

#[cfg(feature = "youtube")]
use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

#[cfg(feature = "youtube")]
pub struct VideoInfo {
    pub title: String,
    pub uploader: Option<String>,
    pub duration: Option<Duration>,
}

#[cfg(feature = "youtube")]
pub fn resolve_ytdlp_binary(binaries_dir: &Path) -> Result<PathBuf, Error> {
    let cached = binaries_dir.join("yt-dlp");
    if cached.is_file() {
        return Ok(cached);
    }

    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("yt-dlp");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    Err(Error::NotFound)
}

#[cfg(feature = "youtube")]
pub fn ffmpeg_available() -> bool {
    which_exists("ffmpeg")
}

#[cfg(feature = "youtube")]
fn which_exists(program: &str) -> bool {
    let Ok(path_var) = std::env::var("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(program))
        .any(|candidate| candidate.is_file())
}

#[cfg(feature = "youtube")]
pub fn fetch_and_download(
    url: &str,
    binaries_dir: &Path,
    scratch_dir: &Path,
    on_info: impl FnOnce(VideoInfo),
    on_progress: impl Fn(f64) + Send + 'static,
) -> Result<PathBuf, Error> {
    if !ffmpeg_available() {
        return Err(Error::FfmpegMissing);
    }

    let ytdlp = resolve_ytdlp_binary(binaries_dir)?;
    let info = fetch_video_info(&ytdlp, url)?;

    if info.is_live {
        return Err(Error::Live);
    }
    on_info(VideoInfo {
        title: info.title.clone(),
        uploader: info.uploader.clone(),
        duration: info.duration,
    });

    let output_template = scratch_dir.join("lyre-dl.%(ext)s");

    let mut child = Command::new(&ytdlp)
        .args([
            "-x",
            "--audio-format",
            "mp3",
            "--no-playlist",
            "--newline",
            "--no-warnings",
            "-o",
        ])
        .arg(&output_template)
        .arg(url)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(Error::Spawn)?;

    spawn_progress_reader(child.stdout.take(), on_progress);

    let status = child.wait().map_err(Error::Spawn)?;

    match status.code() {
        Some(0) => {}
        Some(code) => return Err(Error::DownloadFailed(code)),
        None => return Err(Error::DownloadFailed(-1)),
    }

    let downloaded = newest_mp3_in(scratch_dir)?.ok_or(Error::OutputMissing)?;
    Ok(downloaded)
}

#[cfg(feature = "youtube")]
fn spawn_progress_reader(
    stdout: Option<std::process::ChildStdout>,
    on_progress: impl Fn(f64) + Send + 'static,
) {
    use std::thread;

    let Some(stdout) = stdout else {
        return;
    };
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if let Some(percent) = parse_progress_percent(&line) {
                on_progress(percent.min(100.0));
            }
        }
    });
}

#[cfg(feature = "youtube")]
struct RawVideoInfo {
    title: String,
    uploader: Option<String>,
    duration: Option<Duration>,
    is_live: bool,
}

#[cfg(feature = "youtube")]
fn fetch_video_info(ytdlp: &Path, url: &str) -> Result<RawVideoInfo, Error> {
    let output = Command::new(ytdlp)
        .args(["--dump-json", "--no-warnings", "--no-playlist", url])
        .output()
        .map_err(Error::Spawn)?;

    if !output.status.success() {
        return Err(Error::FetchFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| Error::FetchFailed(e.to_string()))?;

    Ok(RawVideoInfo {
        title: json
            .get("title")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| Error::FetchFailed("missing title in metadata".into()))?
            .to_string(),
        uploader: json
            .get("uploader")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        duration: json
            .get("duration")
            .and_then(serde_json::Value::as_f64)
            .map(Duration::from_secs_f64),
        is_live: json
            .get("is_live")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    })
}

#[cfg(feature = "youtube")]
fn parse_progress_percent(line: &str) -> Option<f64> {
    let rest = line.strip_prefix("[download]")?;
    let percent_token = rest.split_whitespace().next()?;
    percent_token
        .strip_suffix('%')
        .and_then(|num| num.parse::<f64>().ok())
}

#[cfg(feature = "youtube")]
fn newest_mp3_in(dir: &Path) -> Result<Option<PathBuf>, Error> {
    let entries = fs::read_dir(dir).map_err(Error::Temp)?;
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "mp3") {
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            if best
                .as_ref()
                .is_none_or(|(best_time, _)| modified > *best_time)
            {
                best = Some((modified, path));
            }
        }
    }
    Ok(best.map(|(_, path)| path))
}

pub fn generate_file_name(artist: &str, title: &str) -> String {
    format!(
        "{}-{}.mp3",
        generate_name_chunk(artist),
        generate_name_chunk(title)
    )
}

fn generate_name_chunk(field: &str) -> String {
    field
        .split(|c: char| c.is_whitespace() || c == '-')
        .map(capitalize_and_strip)
        .collect::<Vec<_>>()
        .concat()
}

fn capitalize_and_strip(word: &str) -> String {
    let mut chars = word.chars();
    let capitalized = match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    };
    capitalized
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

pub fn finalize_download(temp_path: &Path, dest_path: &Path) -> Result<(), Error> {
    if fs::rename(temp_path, dest_path).is_err() {
        fs::copy(temp_path, dest_path).map_err(Error::Finalize)?;
        let _ = fs::remove_file(temp_path);
    }
    Ok(())
}

pub fn discard_temp_file(path: &Path) {
    let _ = fs::remove_file(path);
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(feature = "youtube")]
    #[error("this video is a live stream and cannot be downloaded")]
    Live,
    #[cfg(feature = "youtube")]
    #[error("failed to start yt-dlp: {0}")]
    Spawn(std::io::Error),
    #[cfg(feature = "youtube")]
    #[error("yt-dlp was not found on PATH — install it with your system package manager")]
    NotFound,
    #[cfg(feature = "youtube")]
    #[error(
        "ffmpeg is required for downloading but was not found — install it with your system package manager"
    )]
    FfmpegMissing,
    #[cfg(feature = "youtube")]
    #[error("failed to read video metadata: {0}")]
    FetchFailed(String),
    #[cfg(feature = "youtube")]
    #[error("yt-dlp exited with an error (code {0})")]
    DownloadFailed(i32),
    #[cfg(feature = "youtube")]
    #[error("could not locate the downloaded audio file")]
    OutputMissing,
    #[cfg(feature = "youtube")]
    #[error("failed to work with temporary files: {0}")]
    Temp(std::io::Error),
    #[error("failed to move the downloaded file into place")]
    Finalize(#[source] std::io::Error),
}
