mod error;

use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{LazyLock, OnceLock};

pub use error::BlurayError;
use regex::{Captures, Regex};
use rosettacue_diagnostics::{DiagnosticEvent, DiagnosticLevel};
use rosettacue_domain::{BlurayDiscInfo, BlurayTitleInfo};

const MEDIA_TOOL_DIRECTORY_ENV: &str = "ROSETTACUE_MEDIA_TOOLS_DIR";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaToolOrigin {
    Configured,
    Bundled,
    Path,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MediaToolDiagnostic {
    pub name: String,
    pub required_for: String,
    pub available: bool,
    pub path: Option<String>,
    pub origin: Option<MediaToolOrigin>,
    pub version: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedMediaTool {
    path: PathBuf,
    origin: MediaToolOrigin,
}

static TITLE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^index:\s*(?<index>\d+)\s+duration:\s*(?<hours>\d+):(?<minutes>\d+):(?<seconds>\d+)\s+chapters:\s*(?<chapters>\d+)\s+angles:\s*(?<angles>\d+)\s+clips:\s*(?<clips>\d+)\s+\(playlist:\s*(?<playlist>\d+)\.mpls\)\s+V:(?<video>\d+)\s+A:(?<audio>\d+)\s+PG:(?<pgs>\d+)",
    )
    .expect("valid title listing regex")
});
static APPLICATION_TOOLS_DIRECTORY: OnceLock<PathBuf> = OnceLock::new();

/// Registers the platform resource directory containing packaged media tools.
///
/// The first successful registration wins so command resolution stays stable for the process.
pub fn configure_media_tools_directory(path: impl Into<PathBuf>) {
    let _ = APPLICATION_TOOLS_DIRECTORY.set(path.into());
}

/// Inspects a Blu-ray backup through libbluray's stable title-listing utility.
///
/// # Errors
///
/// Returns an error when the input is not a BDMV backup, `bd_list_titles` is
/// unavailable, or libbluray returns output that cannot be parsed.
pub fn inspect_disc(input: impl AsRef<Path>) -> Result<BlurayDiscInfo, BlurayError> {
    let started = std::time::Instant::now();
    let root = normalize_disc_root(input.as_ref())?;
    let tool = resolve_tool("bd_list_titles").ok_or(BlurayError::ToolNotFound("bd_list_titles"))?;
    media_event(
        "inspect_disc",
        "start",
        DiagnosticLevel::Info,
        "Inspecting Blu-ray source.",
        None,
        || serde_json::json!({ "tool": tool, "source_path": root, "arguments": ["-l"] }),
    );
    let output = Command::new(tool).arg(&root).arg("-l").output()?;
    if !output.status.success() {
        media_event(
            "inspect_disc",
            "failed",
            DiagnosticLevel::Error,
            "Blu-ray inspection tool failed.",
            Some(elapsed_ms(started)),
            || {
                serde_json::json!({
                    "status": output.status.code(),
                    "stdout": String::from_utf8_lossy(&output.stdout),
                    "stderr": String::from_utf8_lossy(&output.stderr)
                })
            },
        );
        return Err(BlurayError::ToolFailed {
            tool: "bd_list_titles",
            status: output.status.code().unwrap_or(-1),
            message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    let parsed = parse_title_listing(&String::from_utf8(output.stdout.clone())?, &root);
    media_event(
        "inspect_disc",
        if parsed.is_ok() {
            "completed"
        } else {
            "invalid_output"
        },
        if parsed.is_ok() {
            DiagnosticLevel::Info
        } else {
            DiagnosticLevel::Error
        },
        "Blu-ray source inspection completed.",
        Some(elapsed_ms(started)),
        || {
            serde_json::json!({
                "status": output.status.code(),
                "stdout": String::from_utf8_lossy(&output.stdout),
                "title_count": parsed.as_ref().map(|disc| disc.titles.len()).ok(),
                "error": parsed.as_ref().err().map(ToString::to_string)
            })
        },
    );
    parsed
}

/// Streams one Blu-ray PGS track into `FFmpeg` and writes an HDMV SUP file.
///
/// # Errors
///
/// Returns an error when the selected stream is invalid, required media tools
/// are unavailable, or either process fails.
#[allow(clippy::too_many_lines)]
pub fn demux_pgs_track(
    disc_root: impl AsRef<Path>,
    title: &BlurayTitleInfo,
    stream_index: u32,
    destination: impl AsRef<Path>,
) -> Result<(), BlurayError> {
    let started = std::time::Instant::now();
    if stream_index >= title.pgs_tracks {
        return Err(BlurayError::InvalidOutput(format!(
            "PGS stream index {stream_index} is outside title {}",
            title.index
        )));
    }
    let disc_root = normalize_disc_root(disc_root.as_ref())?;
    let destination = destination.as_ref();
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bd_splice = resolve_tool("bd_splice").ok_or(BlurayError::ToolNotFound("bd_splice"))?;
    let ffmpeg = resolve_tool("ffmpeg").ok_or(BlurayError::ToolNotFound("ffmpeg"))?;
    media_event(
        "demux_pgs_track",
        "start",
        DiagnosticLevel::Info,
        "Starting PGS demux.",
        None,
        || {
            serde_json::json!({
                "source_path": disc_root,
                "destination": destination,
                "title_index": title.index,
                "stream_index": stream_index,
                "bd_splice": bd_splice,
                "ffmpeg": ffmpeg
            })
        },
    );
    let mut splice = Command::new(bd_splice)
        .args(["-t", &title.index.to_string(), "-a", "1"])
        .arg(&disc_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let splice_stdout = splice
        .stdout
        .take()
        .ok_or_else(|| BlurayError::InvalidOutput("bd_splice stdout was unavailable".to_owned()))?;
    let ffmpeg_output = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "warning",
            "-nostats",
            "-y",
            "-probesize",
            "100M",
            "-analyzeduration",
            "30M",
            "-i",
            "pipe:0",
            "-map",
            &format!("0:s:{stream_index}"),
            "-c:s",
            "copy",
            "-f",
            "sup",
        ])
        .arg(destination)
        .stdin(Stdio::from(splice_stdout))
        .output()?;
    if !ffmpeg_output.status.success() {
        let _ = splice.kill();
    }
    let splice_output = splice.wait_with_output()?;
    if let Err(error) = ensure_tool_success("ffmpeg", &ffmpeg_output) {
        media_event(
            "demux_pgs_track",
            "failed",
            DiagnosticLevel::Error,
            "FFmpeg failed while demuxing PGS.",
            Some(elapsed_ms(started)),
            || {
                serde_json::json!({
                    "tool": "ffmpeg",
                    "status": ffmpeg_output.status.code(),
                    "stdout": String::from_utf8_lossy(&ffmpeg_output.stdout),
                    "stderr": String::from_utf8_lossy(&ffmpeg_output.stderr),
                    "error": error.to_string()
                })
            },
        );
        return Err(error);
    }
    if let Err(error) = ensure_tool_success("bd_splice", &splice_output) {
        media_event(
            "demux_pgs_track",
            "failed",
            DiagnosticLevel::Error,
            "bd_splice failed while demuxing PGS.",
            Some(elapsed_ms(started)),
            || {
                serde_json::json!({
                    "tool": "bd_splice",
                    "status": splice_output.status.code(),
                    "stderr": String::from_utf8_lossy(&splice_output.stderr),
                    "error": error.to_string()
                })
            },
        );
        return Err(error);
    }
    media_event(
        "demux_pgs_track",
        "completed",
        DiagnosticLevel::Info,
        "PGS demux completed.",
        Some(elapsed_ms(started)),
        || {
            serde_json::json!({
                "destination": destination,
                "output_bytes": std::fs::metadata(destination).map(|metadata| metadata.len()).ok(),
                "ffmpeg_status": ffmpeg_output.status.code(),
                "bd_splice_status": splice_output.status.code()
            })
        },
    );
    Ok(())
}

fn media_event(
    operation: &str,
    phase: &str,
    level: DiagnosticLevel,
    message: &str,
    duration_ms: Option<u64>,
    details: impl FnOnce() -> serde_json::Value,
) {
    if !rosettacue_diagnostics::enabled() {
        return;
    }
    rosettacue_diagnostics::emit(DiagnosticEvent {
        level,
        source: "bluray",
        category: "media",
        operation,
        phase,
        message,
        duration_ms,
        details: details(),
    });
}

fn elapsed_ms(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn ensure_tool_success(
    tool: &'static str,
    output: &std::process::Output,
) -> Result<(), BlurayError> {
    if output.status.success() {
        return Ok(());
    }
    Err(BlurayError::ToolFailed {
        tool,
        status: output.status.code().unwrap_or(-1),
        message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

fn normalize_disc_root(input: &Path) -> Result<PathBuf, BlurayError> {
    if !input.is_dir() {
        return Err(BlurayError::SourceNotFound(input.to_path_buf()));
    }
    let supplied = input.canonicalize()?;
    if supplied.join("BDMV/index.bdmv").is_file() {
        return Ok(supplied);
    }
    if supplied
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("BDMV"))
        && supplied.join("index.bdmv").is_file()
    {
        return supplied
            .parent()
            .map(Path::to_path_buf)
            .ok_or(BlurayError::NotBluray(supplied));
    }
    Err(BlurayError::NotBluray(supplied))
}

fn resolve_tool(name: &str) -> Option<PathBuf> {
    resolve_media_tool(name).map(|tool| tool.path)
}

/// Reports the external media tools available to both the desktop app and CLI.
#[must_use]
pub fn diagnose_media_tools() -> Vec<MediaToolDiagnostic> {
    [
        ("bd_list_titles", "source_analysis"),
        ("bd_splice", "pgs_extraction"),
        ("ffmpeg", "pgs_extraction"),
    ]
    .into_iter()
    .map(|(name, required_for)| match resolve_media_tool(name) {
        Some(tool) => MediaToolDiagnostic {
            name: name.to_owned(),
            required_for: required_for.to_owned(),
            available: true,
            path: Some(tool.path.to_string_lossy().into_owned()),
            origin: Some(tool.origin),
            version: read_tool_version(name, &tool.path),
            message: "ready".to_owned(),
        },
        None => MediaToolDiagnostic {
            name: name.to_owned(),
            required_for: required_for.to_owned(),
            available: false,
            path: None,
            origin: None,
            version: None,
            message: format!("{name} was not found; install it or set {MEDIA_TOOL_DIRECTORY_ENV}"),
        },
    })
    .collect()
}

fn resolve_media_tool(name: &str) -> Option<ResolvedMediaTool> {
    tool_candidates(name)
        .into_iter()
        .find(|(candidate, _)| is_executable_file(candidate))
        .map(|(path, origin)| ResolvedMediaTool {
            path: path.canonicalize().unwrap_or(path),
            origin,
        })
}

fn tool_candidates(name: &str) -> Vec<(PathBuf, MediaToolOrigin)> {
    let executable_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    let mut directories = Vec::<(PathBuf, MediaToolOrigin)>::new();
    if let Some(directory) = env::var_os(MEDIA_TOOL_DIRECTORY_ENV) {
        directories.push((PathBuf::from(directory), MediaToolOrigin::Configured));
    }
    if let Some(directory) = APPLICATION_TOOLS_DIRECTORY.get() {
        directories.push((directory.clone(), MediaToolOrigin::Bundled));
    }
    if let Ok(executable) = env::current_exe()
        && let Some(binary_directory) = executable.parent()
    {
        directories.push((binary_directory.join("tools"), MediaToolOrigin::Bundled));
        directories.push((binary_directory.to_path_buf(), MediaToolOrigin::Bundled));
        if let Some(contents) = binary_directory.parent() {
            directories.push((contents.join("Resources/tools"), MediaToolOrigin::Bundled));
        }
    }
    if let Some(paths) = env::var_os("PATH") {
        directories.extend(env::split_paths(&paths).map(|path| (path, MediaToolOrigin::Path)));
    }
    directories.extend(
        ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]
            .into_iter()
            .map(PathBuf::from)
            .map(|path| (path, MediaToolOrigin::System)),
    );

    let mut seen = HashSet::new();
    directories
        .into_iter()
        .map(|(directory, origin)| (directory.join(&executable_name), origin))
        .filter(|(candidate, _)| seen.insert(candidate.clone()))
        .collect()
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn read_tool_version(name: &str, path: &Path) -> Option<String> {
    let arguments: &[&str] = if name == "ffmpeg" {
        &["-version"]
    } else {
        &["--version"]
    };
    let output = Command::new(path).args(arguments).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr)
    } else {
        String::from_utf8_lossy(&output.stdout)
    };
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

fn parse_title_listing(output: &str, root: &Path) -> Result<BlurayDiscInfo, BlurayError> {
    let main_title_index = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("Main title:"))
        .map(str::trim)
        .ok_or_else(|| BlurayError::InvalidOutput("main title is missing".to_owned()))?
        .parse::<u32>()
        .map_err(|_| BlurayError::InvalidOutput("main title is not a number".to_owned()))?;

    let mut titles = Vec::new();
    for line in output.lines() {
        if let Some(captures) = TITLE_PATTERN.captures(line) {
            let hours = capture_u64(&captures, "hours")?;
            let minutes = capture_u64(&captures, "minutes")?;
            let seconds = capture_u64(&captures, "seconds")?;
            titles.push(BlurayTitleInfo {
                index: capture_u32(&captures, "index")?,
                playlist: format!("{:0>5}", capture_text(&captures, "playlist")?),
                duration_seconds: hours * 3_600 + minutes * 60 + seconds,
                chapters: capture_u32(&captures, "chapters")?,
                angles: capture_u32(&captures, "angles")?,
                clips: capture_u32(&captures, "clips")?,
                video_tracks: capture_u32(&captures, "video")?,
                audio_tracks: capture_u32(&captures, "audio")?,
                pgs_tracks: capture_u32(&captures, "pgs")?,
                pgs_languages: Vec::new(),
            });
        } else if let Some(languages) = line.trim_start().strip_prefix("PG :")
            && let Some(title) = titles.last_mut()
        {
            title.pgs_languages = languages.split_whitespace().map(str::to_owned).collect();
        }
    }

    if titles.is_empty() {
        return Err(BlurayError::InvalidOutput(
            "no playable titles were reported".to_owned(),
        ));
    }
    if !titles.iter().any(|title| title.index == main_title_index) {
        return Err(BlurayError::InvalidOutput(format!(
            "main title {main_title_index} was not listed"
        )));
    }

    let display_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Blu-ray")
        .to_owned();
    Ok(BlurayDiscInfo {
        root_path: root.to_string_lossy().into_owned(),
        display_name,
        main_title_index,
        titles,
    })
}

fn capture_text<'a>(captures: &'a Captures<'_>, name: &str) -> Result<&'a str, BlurayError> {
    captures
        .name(name)
        .map(|value| value.as_str())
        .ok_or_else(|| BlurayError::InvalidOutput(format!("missing {name}")))
}

fn capture_u32(captures: &Captures<'_>, name: &str) -> Result<u32, BlurayError> {
    capture_text(captures, name)?
        .parse()
        .map_err(|_| BlurayError::InvalidOutput(format!("invalid {name}")))
}

fn capture_u64(captures: &Captures<'_>, name: &str) -> Result<u64, BlurayError> {
    capture_text(captures, name)?
        .parse()
        .map_err(|_| BlurayError::InvalidOutput(format!("invalid {name}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = r"Main title: 2
index:   1 duration: 00:00:14 chapters:   1 angles:  1 clips:   1 (playlist: 00000.mpls) V:1 A:1  PG:0  IG:0
index:   2 duration: 02:01:13 chapters:  13 angles:  1 clips:   1 (playlist: 00001.mpls) V:1 A:3  PG:1  IG:1
	PG : jpn
";

    #[test]
    fn parses_libbluray_title_listing() {
        let disc =
            parse_title_listing(LISTING, Path::new("/Movies/Disc 1")).expect("parse title listing");
        assert_eq!(disc.main_title_index, 2);
        assert_eq!(disc.titles.len(), 2);
        let main = disc.main_title().expect("main title");
        assert_eq!(main.playlist, "00001");
        assert_eq!(main.duration_seconds, 7_273);
        assert_eq!(main.pgs_languages, ["jpn"]);
    }

    #[test]
    fn accepts_disc_root_and_bdmv_directory() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let bdmv = temporary.path().join("BDMV");
        std::fs::create_dir(&bdmv).expect("BDMV directory");
        std::fs::write(bdmv.join("index.bdmv"), []).expect("index.bdmv");
        let root = temporary.path().canonicalize().expect("canonical root");
        assert_eq!(
            normalize_disc_root(temporary.path()).expect("disc root"),
            root
        );
        assert_eq!(normalize_disc_root(&bdmv).expect("BDMV root"), root);
    }

    #[test]
    fn bundled_candidates_precede_path_candidates() {
        let candidates = tool_candidates("ffmpeg");
        let configured_index = candidates
            .iter()
            .position(|(_, origin)| *origin == MediaToolOrigin::Configured);
        let bundled_index = candidates
            .iter()
            .position(|(_, origin)| *origin == MediaToolOrigin::Bundled)
            .expect("bundled candidate");
        let path_index = candidates
            .iter()
            .position(|(_, origin)| *origin == MediaToolOrigin::Path);
        if let Some(configured_index) = configured_index {
            assert!(configured_index < bundled_index);
        }
        if let Some(path_index) = path_index {
            assert!(bundled_index < path_index);
        }
    }
}
