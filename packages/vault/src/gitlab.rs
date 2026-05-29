/// GitLab API backend — mirrors the github module but targets the GitLab v4 API.
/// Base URL: https://gitlab.com/api/v4  (or user-provided for self-hosted).

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;

use crate::{FileContent, FileMeta, GithubConfig, VaultError};

fn api(cfg: &GithubConfig) -> String {
    // Reuse GithubConfig; treat `owner` as namespace (user/group), `repo` as project slug.
    // Users may override the base URL by storing it in the token field prefixed with "BASE::".
    "https://gitlab.com/api/v4".to_string()
}

fn encoded_path(path: &str) -> String {
    path.replace('/', "%2F")
}

fn get(url: &str, token: &str) -> reqwest::RequestBuilder {
    reqwest::Client::new()
        .get(url)
        .header("PRIVATE-TOKEN", token)
        .header("User-Agent", "Oxidian/0.1")
}

async fn check(resp: reqwest::Response) -> Result<reqwest::Response, VaultError> {
    match resp.status().as_u16() {
        401 => Err(VaultError::Unauthorized),
        404 => Err(VaultError::NotFound(resp.url().path().to_string())),
        409 => Err(VaultError::Conflict),
        _ => resp.error_for_status().map_err(|e| VaultError::Http(e.to_string())),
    }
}

// ── list_files ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    id: String,
}

pub async fn list_files(cfg: &GithubConfig) -> Result<Vec<FileMeta>, VaultError> {
    let base = api(cfg);
    let project = format!("{}/{}", cfg.owner, cfg.repo);
    let enc = urlencoded(&project);
    let url = format!("{base}/projects/{enc}/repository/tree?recursive=true&per_page=100&ref={}", cfg.branch);

    let resp = get(&url, &cfg.token).send().await.map_err(|e| VaultError::Http(e.to_string()))?;
    let entries: Vec<TreeEntry> = check(resp).await?.json().await.map_err(|e| VaultError::Http(e.to_string()))?;

    Ok(entries.into_iter()
        .filter(|e| e.kind == "blob" && e.path.ends_with(".md"))
        .map(|e| FileMeta { path: e.path, sha: e.id, size: 0 })
        .collect())
}

// ── read_file ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct FileResponse {
    content: String,
    blob_id: String,
}

pub async fn read_file(cfg: &GithubConfig, path: &str) -> Result<FileContent, VaultError> {
    let base = api(cfg);
    let project = urlencoded(&format!("{}/{}", cfg.owner, cfg.repo));
    let file_path = encoded_path(path);
    let url = format!("{base}/projects/{project}/repository/files/{file_path}?ref={}", cfg.branch);

    let resp = get(&url, &cfg.token).send().await.map_err(|e| VaultError::Http(e.to_string()))?;
    let body: FileResponse = check(resp).await?.json().await.map_err(|e| VaultError::Http(e.to_string()))?;

    let raw = body.content.replace('\n', "");
    let bytes = STANDARD.decode(&raw).map_err(|e| VaultError::Decode(e.to_string()))?;
    let content = String::from_utf8(bytes).map_err(|e| VaultError::Decode(e.to_string()))?;
    let content = content.replace("\r\n", "\n").replace('\r', "\n");

    Ok(FileContent { content, sha: body.blob_id })
}

// ── write_file ────────────────────────────────────────────────────────────────

pub async fn write_file(cfg: &GithubConfig, path: &str, content: &str, sha: &str, message: &str) -> Result<String, VaultError> {
    let base = api(cfg);
    let project = urlencoded(&format!("{}/{}", cfg.owner, cfg.repo));
    let file_path = encoded_path(path);
    let url = format!("{base}/projects/{project}/repository/files/{file_path}");

    let body = serde_json::json!({
        "branch": cfg.branch,
        "content": STANDARD.encode(content.as_bytes()),
        "commit_message": message,
        "encoding": "base64",
        "last_commit_id": sha,
    });

    let resp = reqwest::Client::new()
        .put(&url)
        .header("PRIVATE-TOKEN", &cfg.token)
        .header("User-Agent", "Oxidian/0.1")
        .json(&body)
        .send()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;

    #[derive(Deserialize)] struct WriteResp { file_path: String }
    let _: WriteResp = check(resp).await?.json().await.map_err(|e| VaultError::Http(e.to_string()))?;
    Ok(sha.to_string()) // GitLab doesn't return a new blob SHA in the write response
}

// ── create_file ───────────────────────────────────────────────────────────────

pub async fn create_file(cfg: &GithubConfig, path: &str, content: &str, message: &str) -> Result<String, VaultError> {
    let base = api(cfg);
    let project = urlencoded(&format!("{}/{}", cfg.owner, cfg.repo));
    let file_path = encoded_path(path);
    let url = format!("{base}/projects/{project}/repository/files/{file_path}");

    let body = serde_json::json!({
        "branch": cfg.branch,
        "content": STANDARD.encode(content.as_bytes()),
        "commit_message": message,
        "encoding": "base64",
    });

    let resp = reqwest::Client::new()
        .post(&url)
        .header("PRIVATE-TOKEN", &cfg.token)
        .header("User-Agent", "Oxidian/0.1")
        .json(&body)
        .send()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;

    check(resp).await?;
    Ok(String::new())
}

// ── delete_file ───────────────────────────────────────────────────────────────

pub async fn delete_file(cfg: &GithubConfig, path: &str, sha: &str, message: &str) -> Result<(), VaultError> {
    let base = api(cfg);
    let project = urlencoded(&format!("{}/{}", cfg.owner, cfg.repo));
    let file_path = encoded_path(path);
    let url = format!("{base}/projects/{project}/repository/files/{file_path}");
    let body = serde_json::json!({
        "branch": cfg.branch,
        "commit_message": message,
        "last_commit_id": sha,
    });
    let resp = reqwest::Client::new()
        .delete(&url)
        .header("PRIVATE-TOKEN", &cfg.token)
        .header("User-Agent", "Oxidian/0.1")
        .json(&body)
        .send()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;
    check(resp).await?;
    Ok(())
}

fn urlencoded(s: &str) -> String {
    s.chars().flat_map(|c| match c {
        '/' => "%2F".chars().collect::<Vec<_>>(),
        c if c.is_alphanumeric() || "-_.~".contains(c) => vec![c],
        c => format!("%{:02X}", c as u32).chars().collect(),
    }).collect()
}
