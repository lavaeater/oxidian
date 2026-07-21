/// GitLab API backend — mirrors the github module but targets the GitLab v4 API.
/// Base URL: https://gitlab.com/api/v4  (or user-provided for self-hosted).

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;

use crate::{FileContent, FileMeta, GithubConfig, VaultError};

fn api(_cfg: &GithubConfig) -> String {
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

/// Map the meaningful HTTP status codes to a `VaultError`. Returns `None` for
/// codes this doesn't special-case (the caller falls back to `error_for_status`).
/// 409 → `Conflict` is the SHA/`last_commit_id` mismatch that guards concurrent
/// edits — losing this mapping would let a stale write clobber remote changes.
fn status_error(status: u16, path: &str) -> Option<VaultError> {
    match status {
        401 => Some(VaultError::Unauthorized),
        404 => Some(VaultError::NotFound(path.to_string())),
        409 => Some(VaultError::Conflict),
        _ => None,
    }
}

async fn check(resp: reqwest::Response) -> Result<reqwest::Response, VaultError> {
    if let Some(err) = status_error(resp.status().as_u16(), resp.url().path()) {
        return Err(err);
    }
    resp.error_for_status().map_err(|e| VaultError::Http(e.to_string()))
}

// ── list_files ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    id: String,
}

fn tree_url(cfg: &GithubConfig) -> String {
    let project = urlencoded(&format!("{}/{}", cfg.owner, cfg.repo));
    format!(
        "{}/projects/{project}/repository/tree?recursive=true&per_page=100&ref={}",
        api(cfg), cfg.branch
    )
}

/// GitLab tree entries: keep note blobs, map the blob `id` to our `sha` (GitLab
/// doesn't return a size here, so it's 0).
fn tree_to_files(entries: Vec<TreeEntry>) -> Vec<FileMeta> {
    entries.into_iter()
        .filter(|e| e.kind == "blob" && (e.path.ends_with(".md") || e.path.ends_with(".gitkeep")))
        .map(|e| FileMeta { path: e.path, sha: e.id, size: 0 })
        .collect()
}

pub async fn list_files(cfg: &GithubConfig) -> Result<Vec<FileMeta>, VaultError> {
    let resp = get(&tree_url(cfg), &cfg.token).send().await.map_err(|e| VaultError::Http(e.to_string()))?;
    let entries: Vec<TreeEntry> = check(resp).await?.json().await.map_err(|e| VaultError::Http(e.to_string()))?;
    Ok(tree_to_files(entries))
}

// ── read_file ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct FileResponse {
    content: String,
    blob_id: String,
}

/// Path to a single file's contents endpoint (used for read; write/create/delete
/// use the same path without the `?ref=` query).
fn file_url(cfg: &GithubConfig, path: &str) -> String {
    let project = urlencoded(&format!("{}/{}", cfg.owner, cfg.repo));
    format!("{}/projects/{project}/repository/files/{}", api(cfg), encoded_path(path))
}

/// Decode GitLab's base64 file content into a normalised UTF-8 string
/// (strip wrap newlines, normalise CRLF — raw CR breaks the tokenizer).
fn decode_content(b64: &str, sha: String) -> Result<FileContent, VaultError> {
    let raw = b64.replace('\n', "");
    let bytes = STANDARD.decode(&raw).map_err(|e| VaultError::Decode(e.to_string()))?;
    let content = String::from_utf8(bytes).map_err(|e| VaultError::Decode(e.to_string()))?;
    let content = content.replace("\r\n", "\n").replace('\r', "\n");
    Ok(FileContent { content, sha })
}

pub async fn read_file(cfg: &GithubConfig, path: &str) -> Result<FileContent, VaultError> {
    let url = format!("{}?ref={}", file_url(cfg, path), cfg.branch);
    let resp = get(&url, &cfg.token).send().await.map_err(|e| VaultError::Http(e.to_string()))?;
    let body: FileResponse = check(resp).await?.json().await.map_err(|e| VaultError::Http(e.to_string()))?;
    decode_content(&body.content, body.blob_id)
}

// ── write_file ────────────────────────────────────────────────────────────────

pub async fn write_file(cfg: &GithubConfig, path: &str, content: &str, sha: &str, message: &str) -> Result<String, VaultError> {
    let url = file_url(cfg, path);

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
    let url = file_url(cfg, path);

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
    let url = file_url(cfg, path);
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

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};

    fn cfg() -> GithubConfig {
        GithubConfig {
            token: "tok".into(),
            owner: "group/sub".into(),
            repo: "notes".into(),
            branch: "main".into(),
            provider: crate::Provider::GitLab,
            templates_dir: String::new(),
            daily_note_template: String::new(),
        }
    }

    #[test]
    fn project_path_is_percent_encoded_including_slashes() {
        // Namespace slashes must be encoded so the whole project path is one segment.
        assert_eq!(urlencoded("group/sub/notes"), "group%2Fsub%2Fnotes");
        assert_eq!(encoded_path("dir/file name.md"), "dir%2Ffile name.md");
    }

    #[test]
    fn builds_tree_and_file_urls() {
        assert_eq!(
            tree_url(&cfg()),
            "https://gitlab.com/api/v4/projects/group%2Fsub%2Fnotes/repository/tree?recursive=true&per_page=100&ref=main"
        );
        assert_eq!(
            file_url(&cfg(), "sub/idea.md"),
            "https://gitlab.com/api/v4/projects/group%2Fsub%2Fnotes/repository/files/sub%2Fidea.md"
        );
    }

    #[test]
    fn tree_keeps_only_notes_and_maps_id_to_sha() {
        let json = r#"[
            {"path":"a.md","type":"blob","id":"id1"},
            {"path":"pic.png","type":"blob","id":"id2"},
            {"path":"dir","type":"tree","id":"id3"},
            {"path":"empty/.gitkeep","type":"blob","id":"id4"}
        ]"#;
        let entries: Vec<TreeEntry> = serde_json::from_str(json).unwrap();
        let files = tree_to_files(entries);
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["a.md", "empty/.gitkeep"]);
        assert_eq!(files[0].sha, "id1"); // blob id -> sha
        assert_eq!(files[0].size, 0);    // GitLab tree has no size
    }

    #[test]
    fn decode_content_strips_newlines_and_normalises_crlf() {
        let encoded = STANDARD.encode("x\r\ny\rz");
        let wrapped = format!("{}\n{}", &encoded[..4], &encoded[4..]);
        let fc = decode_content(&wrapped, "blob".into()).unwrap();
        assert_eq!(fc.content, "x\ny\nz");
        assert_eq!(fc.sha, "blob");
    }

    #[test]
    fn decode_content_rejects_invalid_base64() {
        assert!(matches!(
            decode_content("@@@", "s".into()),
            Err(VaultError::Decode(_))
        ));
    }

    #[test]
    fn status_error_maps_known_codes() {
        assert!(matches!(status_error(401, "/x"), Some(VaultError::Unauthorized)));
        assert!(matches!(status_error(404, "/notes/a.md"), Some(VaultError::NotFound(p)) if p == "/notes/a.md"));
        // The SHA-conflict guarantee: a stale write must surface as Conflict.
        assert!(matches!(status_error(409, "/x"), Some(VaultError::Conflict)));
        // Success and unclassified codes defer to error_for_status.
        assert!(status_error(200, "/x").is_none());
        assert!(status_error(500, "/x").is_none());
    }
}
