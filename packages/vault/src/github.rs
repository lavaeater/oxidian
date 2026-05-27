use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;

use crate::{FileContent, FileMeta, GithubConfig, SearchResult, VaultError};

const API: &str = "https://api.github.com";

fn request(method: reqwest::Method, url: &str, token: &str) -> reqwest::RequestBuilder {
    reqwest::Client::new()
        .request(method, url)
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", "Oxidian/0.1")
        .header("Accept", "application/vnd.github.v3+json")
}

fn get(url: &str, token: &str) -> reqwest::RequestBuilder {
    request(reqwest::Method::GET, url, token)
}

async fn check(resp: reqwest::Response) -> Result<reqwest::Response, VaultError> {
    if resp.status() == 401 {
        return Err(VaultError::Unauthorized);
    }
    if resp.status() == 404 {
        return Err(VaultError::NotFound(resp.url().path().to_string()));
    }
    resp.error_for_status().map_err(|e| VaultError::Http(e.to_string()))
}

// ── list_files ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TreeResponse {
    tree: Vec<TreeEntry>,
}

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    sha: String,
    size: Option<usize>,
}

pub async fn list_files(cfg: &GithubConfig) -> Result<Vec<FileMeta>, VaultError> {
    let url = format!(
        "{API}/repos/{}/{}/git/trees/{}?recursive=1",
        cfg.owner, cfg.repo, cfg.branch
    );
    let resp = get(&url, &cfg.token)
        .send()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;
    let tree: TreeResponse = check(resp)
        .await?
        .json()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;

    Ok(tree
        .tree
        .into_iter()
        .filter(|e| e.kind == "blob" && e.path.ends_with(".md"))
        .map(|e| FileMeta {
            path: e.path,
            sha: e.sha,
            size: e.size.unwrap_or(0),
        })
        .collect())
}

// ── read_file ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ContentsResponse {
    content: String,
    sha: String,
}

pub async fn read_file(cfg: &GithubConfig, path: &str) -> Result<FileContent, VaultError> {
    let url = format!(
        "{API}/repos/{}/{}/contents/{path}",
        cfg.owner, cfg.repo
    );
    let resp = get(&url, &cfg.token)
        .send()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;
    let body: ContentsResponse = check(resp)
        .await?
        .json()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;

    // GitHub base64-encodes content with newlines every 60 chars
    let raw = body.content.replace('\n', "");
    let bytes = STANDARD
        .decode(&raw)
        .map_err(|e| VaultError::Decode(e.to_string()))?;
    let content = String::from_utf8(bytes).map_err(|e| VaultError::Decode(e.to_string()))?;

    // Normalise line endings — GitHub can serve CRLF which breaks the tokenizer.
    let content = content.replace("\r\n", "\n").replace('\r', "\n");

    Ok(FileContent { content, sha: body.sha })
}

// ── write_file ────────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct WriteBody<'a> {
    message: &'a str,
    content: String,
    sha: &'a str,
    branch: &'a str,
}

#[derive(Deserialize)]
struct WriteResponse {
    content: WrittenFile,
}

#[derive(Deserialize)]
struct WrittenFile {
    sha: String,
}

/// Write `content` to `path`, creating a commit with `message`.
/// `sha` must be the current blob SHA (from `read_file` or a previous write).
/// Returns the new blob SHA to use for subsequent writes.
pub async fn write_file(
    cfg: &GithubConfig,
    path: &str,
    content: &str,
    sha: &str,
    message: &str,
) -> Result<String, VaultError> {
    let url = format!("{API}/repos/{}/{}/contents/{path}", cfg.owner, cfg.repo);
    let body = WriteBody {
        message,
        content: STANDARD.encode(content.as_bytes()),
        sha,
        branch: &cfg.branch,
    };
    let resp = request(reqwest::Method::PUT, &url, &cfg.token)
        .json(&body)
        .send()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;

    if resp.status() == reqwest::StatusCode::CONFLICT {
        return Err(VaultError::Conflict);
    }

    let written: WriteResponse = check(resp)
        .await?
        .json()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;

    Ok(written.content.sha)
}

// ── search_code ───────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SearchResponse {
    items: Vec<SearchItem>,
}

#[derive(Deserialize)]
struct SearchItem {
    path: String,
    sha: String,
    #[serde(default)]
    text_matches: Vec<TextMatch>,
}

#[derive(Deserialize)]
struct TextMatch {
    fragment: String,
}

/// Full-text search across the repo using GitHub's Code Search API.
/// Returns up to 30 results with matching text fragments.
pub async fn search_code(cfg: &GithubConfig, query: &str) -> Result<Vec<SearchResult>, VaultError> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }
    let q = format!("{} repo:{}/{}", query.trim(), cfg.owner, cfg.repo);
    let url = format!("{API}/search/code?q={}&per_page=30", urlencoded(&q));

    let resp = reqwest::Client::new()
        .get(&url)
        .header("Authorization", format!("Bearer {}", cfg.token))
        .header("User-Agent", "Oxidian/0.1")
        // text-match media type returns matching fragments
        .header("Accept", "application/vnd.github.text-match+json")
        .send()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;

    let body: SearchResponse = check(resp)
        .await?
        .json()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;

    Ok(body
        .items
        .into_iter()
        .filter(|i| i.path.ends_with(".md"))
        .map(|i| SearchResult {
            path: i.path,
            sha: i.sha,
            fragment: i
                .text_matches
                .into_iter()
                .next()
                .map(|m| m.fragment)
                .unwrap_or_default(),
        })
        .collect())
}

// ── read_many ─────────────────────────────────────────────────────────────────

/// Fetch content of multiple files sequentially.
/// Returns `(path, content)` pairs for successfully fetched files.
pub async fn read_many(cfg: &GithubConfig, paths: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for path in paths {
        if let Ok(fc) = read_file(cfg, path).await {
            out.push((path.clone(), fc.content));
        }
    }
    out
}

// ── create_file ───────────────────────────────────────────────────────────────

/// Create a new file (path must not already exist).
/// Returns the blob SHA of the newly created file.
pub async fn create_file(
    cfg: &GithubConfig,
    path: &str,
    content: &str,
    message: &str,
) -> Result<String, VaultError> {
    let url = format!("{API}/repos/{}/{}/contents/{path}", cfg.owner, cfg.repo);
    // No "sha" field = create, not update
    let body = serde_json::json!({
        "message": message,
        "content": STANDARD.encode(content.as_bytes()),
        "branch": cfg.branch,
    });
    let resp = request(reqwest::Method::PUT, &url, &cfg.token)
        .json(&body)
        .send()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;

    if resp.status() == reqwest::StatusCode::UNPROCESSABLE_ENTITY {
        return Err(VaultError::Http("File already exists".into()));
    }

    let written: WriteResponse = check(resp)
        .await?
        .json()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;

    Ok(written.content.sha)
}

// ── OAuth Device Flow ─────────────────────────────────────────────────────────

pub const GITHUB_CLIENT_ID: &str = "Ov23li0fTUa8YSbUsWwI";

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    /// Pre-filled URL including the user_code as a query param — open this
    /// directly so the user just has to click Authorize, no typing needed.
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub expires_in: u32,
    pub interval: u32,
}

#[derive(Debug)]
pub enum PollOutcome {
    Token(String),
    Pending,
    SlowDown(u32),
    Expired,
    Denied,
}

pub async fn request_device_code() -> Result<DeviceCodeResponse, VaultError> {
    let resp = reqwest::Client::new()
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .form(&[("client_id", GITHUB_CLIENT_ID), ("scope", "repo")])
        .send()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;
    resp.json::<DeviceCodeResponse>()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))
}

pub async fn poll_device_token(device_code: &str) -> Result<PollOutcome, VaultError> {
    #[derive(serde::Deserialize)]
    struct PollResp {
        access_token: Option<String>,
        error: Option<String>,
        interval: Option<u32>,
    }
    let resp = reqwest::Client::new()
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", GITHUB_CLIENT_ID),
            ("device_code", device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .send()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;
    let body: PollResp = resp.json().await.map_err(|e| VaultError::Http(e.to_string()))?;
    if let Some(token) = body.access_token {
        return Ok(PollOutcome::Token(token));
    }
    Ok(match body.error.as_deref() {
        Some("slow_down")      => PollOutcome::SlowDown(body.interval.unwrap_or(10)),
        Some("expired_token")  => PollOutcome::Expired,
        Some("access_denied")  => PollOutcome::Denied,
        _                      => PollOutcome::Pending,
    })
}

pub async fn get_username(token: &str) -> Result<String, VaultError> {
    #[derive(serde::Deserialize)]
    struct User { login: String }
    let resp = get("https://api.github.com/user", token)
        .send()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;
    let user: User = check(resp).await?.json().await.map_err(|e| VaultError::Http(e.to_string()))?;
    Ok(user.login)
}

fn urlencoded(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            ' ' => "+".chars().collect::<Vec<_>>(),
            c if c.is_alphanumeric() || "-_.~".contains(c) => vec![c],
            c => format!("%{:02X}", c as u32).chars().collect(),
        })
        .collect()
}
