use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;

use crate::{FileContent, FileMeta, GithubConfig, VaultError};

const API: &str = "https://api.github.com";

fn get(url: &str, token: &str) -> reqwest::RequestBuilder {
    reqwest::Client::new()
        .get(url)
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", "Oxidian/0.1")
        .header("Accept", "application/vnd.github.v3+json")
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

    Ok(FileContent { content, sha: body.sha })
}
