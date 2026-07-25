use std::{
    env,
    error::Error,
    path::{Path as FilePath, PathBuf},
    process::ExitCode,
    sync::Arc,
};

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post, put},
};
use serde::Serialize;
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt, SeekFrom},
    net::TcpListener,
    sync::RwLock,
};
use tokio_util::io::ReaderStream;
use trailmix::{AnalysisConfig, AudioBuffer};
use trailmix_manifest::{AnnotationManifest, TrackAnnotation};

const INDEX_HTML: &str = include_str!("../static/index.html");
const APP_JS: &str = include_str!("../static/app.js");
const STYLE_CSS: &str = include_str!("../static/style.css");

#[derive(Clone)]
struct AppState {
    manifest_path: PathBuf,
    base_directory: PathBuf,
    manifest: Arc<RwLock<AnnotationManifest>>,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

#[derive(Debug)]
struct AppError {
    status: StatusCode,
    message: String,
}

impl AppError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("test-kitchen: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn Error>> {
    let (manifest_path, should_open_browser) = parse_arguments()?;
    let manifest = if manifest_path.exists() {
        trailmix_manifest::load(&manifest_path)?
    } else {
        let manifest = AnnotationManifest::default();
        trailmix_manifest::save(&manifest_path, &manifest)?;
        manifest
    };
    let base_directory = manifest_path
        .parent()
        .unwrap_or_else(|| FilePath::new("."))
        .to_path_buf();
    let state = AppState {
        manifest_path,
        base_directory,
        manifest: Arc::new(RwLock::new(manifest)),
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/app.js", get(javascript))
        .route("/style.css", get(stylesheet))
        .route("/api/manifest", get(get_manifest))
        .route("/api/tracks", post(add_track))
        .route("/api/tracks/{id}", put(update_track).delete(delete_track))
        .route("/api/tracks/{id}/analyze", post(analyze_track))
        .route("/audio/{id}", get(serve_audio))
        .with_state(state);
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let address = listener.local_addr()?;
    let url = format!("http://{address}");
    println!("Test Kitchen is running at {url}");
    if let Some(Err(error)) = should_open_browser.then(|| open::that(&url)) {
        eprintln!("test-kitchen: could not open browser: {error}");
    }
    axum::serve(listener, app).await?;
    Ok(())
}

fn parse_arguments() -> Result<(PathBuf, bool), Box<dyn Error>> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let Some(first) = arguments.next() else {
        return Err("usage: test-kitchen [--no-open] <manifest.json>".into());
    };
    let (path, should_open_browser) = if first == "--no-open" {
        (
            arguments
                .next()
                .ok_or("usage: test-kitchen [--no-open] <manifest.json>")?,
            false,
        )
    } else {
        (first, true)
    };
    if arguments.next().is_some() {
        return Err("usage: test-kitchen [--no-open] <manifest.json>".into());
    }
    Ok((PathBuf::from(path), should_open_browser))
}

async fn index() -> impl IntoResponse {
    static_response("text/html; charset=utf-8", INDEX_HTML)
}

async fn javascript() -> impl IntoResponse {
    static_response("text/javascript; charset=utf-8", APP_JS)
}

async fn stylesheet() -> impl IntoResponse {
    static_response("text/css; charset=utf-8", STYLE_CSS)
}

fn static_response(content_type: &'static str, content: &'static str) -> Response {
    let mut response = Html(content).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; media-src 'self'; script-src 'self'; style-src 'self'; \
             connect-src 'self'; img-src 'self' data:; object-src 'none'; base-uri 'none'",
        ),
    );
    response
}

async fn get_manifest(State(state): State<AppState>) -> Json<AnnotationManifest> {
    Json(state.manifest.read().await.clone())
}

async fn add_track(
    State(state): State<AppState>,
    Json(track): Json<TrackAnnotation>,
) -> Result<Json<TrackAnnotation>, AppError> {
    let mut manifest = state.manifest.write().await;
    if manifest
        .tracks
        .iter()
        .any(|existing| existing.id == track.id)
    {
        return Err(AppError::bad_request(format!(
            "track ID {} already exists",
            track.id
        )));
    }
    let mut candidate = manifest.clone();
    candidate.tracks.push(track.clone());
    save_candidate(&state, &candidate)?;
    *manifest = candidate;
    Ok(Json(track))
}

async fn update_track(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(track): Json<TrackAnnotation>,
) -> Result<Json<TrackAnnotation>, AppError> {
    if id != track.id {
        return Err(AppError::bad_request(
            "route ID and annotation ID must match",
        ));
    }
    let mut manifest = state.manifest.write().await;
    let mut candidate = manifest.clone();
    let existing = candidate
        .tracks
        .iter_mut()
        .find(|existing| existing.id == id)
        .ok_or_else(|| AppError::not_found(format!("unknown track {id}")))?;
    *existing = track.clone();
    save_candidate(&state, &candidate)?;
    *manifest = candidate;
    Ok(Json(track))
}

async fn delete_track(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let mut manifest = state.manifest.write().await;
    let mut candidate = manifest.clone();
    let original_length = candidate.tracks.len();
    candidate.tracks.retain(|track| track.id != id);
    if candidate.tracks.len() == original_length {
        return Err(AppError::not_found(format!("unknown track {id}")));
    }
    save_candidate(&state, &candidate)?;
    *manifest = candidate;
    Ok(StatusCode::NO_CONTENT)
}

fn save_candidate(state: &AppState, manifest: &AnnotationManifest) -> Result<(), AppError> {
    trailmix_manifest::save(&state.manifest_path, manifest)
        .map_err(|error| AppError::bad_request(error.to_string()))
}

async fn analyze_track(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<trailmix::Analysis>, AppError> {
    let path = track_path(&state, &id).await?;
    let analysis = tokio::task::spawn_blocking(move || {
        let decoded = trailmix_codecs::decode_file(path)?;
        Ok::<_, trailmix_codecs::DecodeError>(trailmix::analyze(
            AudioBuffer {
                samples: &decoded.samples,
                sample_rate: decoded.sample_rate,
            },
            AnalysisConfig::default(),
        ))
    })
    .await
    .map_err(|error| AppError::internal(error.to_string()))?
    .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(analysis))
}

async fn serve_audio(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let path = track_path(&state, &id).await?;
    let mut file = File::open(&path)
        .await
        .map_err(|error| AppError::not_found(error.to_string()))?;
    let length = file
        .metadata()
        .await
        .map_err(|error| AppError::internal(error.to_string()))?
        .len();
    if length == 0 {
        return Err(AppError::bad_request("audio file is empty"));
    }

    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(|value| parse_byte_range(value, length))
        .transpose()?;
    let (start, end, status) = range.map_or((0, length - 1, StatusCode::OK), |(start, end)| {
        (start, end, StatusCode::PARTIAL_CONTENT)
    });
    file.seek(SeekFrom::Start(start))
        .await
        .map_err(|error| AppError::internal(error.to_string()))?;
    let response_length = end - start + 1;
    let stream = ReaderStream::new(file.take(response_length));
    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = status;
    let response_headers = response.headers_mut();
    response_headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    response_headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(audio_content_type(&path)),
    );
    response_headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&response_length.to_string())
            .map_err(|error| AppError::internal(error.to_string()))?,
    );
    if status == StatusCode::PARTIAL_CONTENT {
        response_headers.insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {start}-{end}/{length}"))
                .map_err(|error| AppError::internal(error.to_string()))?,
        );
    }
    Ok(response)
}

async fn track_path(state: &AppState, id: &str) -> Result<PathBuf, AppError> {
    let manifest = state.manifest.read().await;
    let track = manifest
        .tracks
        .iter()
        .find(|track| track.id == id)
        .ok_or_else(|| AppError::not_found(format!("unknown track {id}")))?;
    Ok(if track.path.is_absolute() {
        track.path.clone()
    } else {
        state.base_directory.join(&track.path)
    })
}

fn parse_byte_range(value: &str, length: u64) -> Result<(u64, u64), AppError> {
    let range = value
        .strip_prefix("bytes=")
        .ok_or_else(|| AppError::bad_request("unsupported Range header"))?;
    if range.contains(',') {
        return Err(AppError::bad_request(
            "multiple byte ranges are not supported",
        ));
    }
    let (start_text, end_text) = range
        .split_once('-')
        .ok_or_else(|| AppError::bad_request("invalid byte range"))?;
    let (start, end) = if start_text.is_empty() {
        let suffix_length = end_text
            .parse::<u64>()
            .map_err(|_| AppError::bad_request("invalid byte range"))?;
        if suffix_length == 0 {
            return Err(AppError::bad_request("invalid byte range"));
        }
        (length.saturating_sub(suffix_length), length - 1)
    } else {
        let start = start_text
            .parse::<u64>()
            .map_err(|_| AppError::bad_request("invalid byte range"))?;
        let end = if end_text.is_empty() {
            length - 1
        } else {
            end_text
                .parse::<u64>()
                .map_err(|_| AppError::bad_request("invalid byte range"))?
                .min(length - 1)
        };
        (start, end)
    };
    if start >= length || end < start {
        return Err(AppError::bad_request("byte range is outside the file"));
    }
    Ok((start, end))
}

fn audio_content_type(path: &FilePath) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mp3") => "audio/mpeg",
        Some("m4a" | "mp4") => "audio/mp4",
        Some("wav") => "audio/wav",
        Some("aif" | "aiff") => "audio/aiff",
        Some("flac") => "audio/flac",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_open_and_suffix_ranges() {
        assert_eq!(parse_byte_range("bytes=10-", 100).expect("range"), (10, 99));
        assert_eq!(parse_byte_range("bytes=-20", 100).expect("range"), (80, 99));
    }

    #[test]
    fn rejects_out_of_bounds_ranges() {
        assert!(parse_byte_range("bytes=100-200", 100).is_err());
        assert!(parse_byte_range("items=1-2", 100).is_err());
    }
}
