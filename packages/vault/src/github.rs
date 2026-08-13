use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;

use crate::{FileContent, FileMeta, GithubConfig, VaultError};

const API: &str = "https://api.github.com";

fn request(method: reqwest::Method, url: &str, token: &str) -> reqwest::RequestBuilder {
    crate::http::client()
        .request(method, url)
        .header("Authorization", format!("Bearer {token}"))
        .header("User-Agent", "Oxidian/0.1")
        .header("Accept", "application/vnd.github.v3+json")
}

fn get(url: &str, token: &str) -> reqwest::RequestBuilder {
    request(reqwest::Method::GET, url, token)
}

/// Map the meaningful HTTP status codes to a `VaultError`; `None` means "not
/// special-cased" and the caller defers to `error_for_status`. (GitHub's
/// Contents API signals write conflicts with 409, handled at the `write_file`
/// call site, and 422 for create-on-existing — see `create_file`.)
fn status_error(status: u16, path: &str) -> Option<VaultError> {
    match status {
        401 => Some(VaultError::Unauthorized),
        404 => Some(VaultError::NotFound(path.to_string())),
        _ => None,
    }
}

fn check(resp: reqwest::Response) -> Result<reqwest::Response, VaultError> {
    if let Some(err) = status_error(resp.status().as_u16(), resp.url().path()) {
        return Err(err);
    }
    resp.error_for_status().map_err(|e| VaultError::Http(e.to_string()))
}

// ── list_files ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TreeResponse {
    tree: Vec<TreeEntry>,
    /// GitHub sets this when the recursive listing hit its limit (~100k entries
    /// or 7 MB) and silently dropped the rest. See `list_files`.
    #[serde(default)]
    truncated: bool,
}

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    sha: String,
    size: Option<usize>,
}

fn tree_url(cfg: &GithubConfig) -> String {
    format!(
        "{API}/repos/{}/{}/git/trees/{}?recursive=1",
        cfg.owner, cfg.repo, cfg.branch
    )
}

/// One directory level. `sha` is a tree SHA (or the branch name at the root);
/// entry paths are bare names, not full paths.
fn subtree_url(cfg: &GithubConfig, sha: &str) -> String {
    format!("{API}/repos/{}/{}/git/trees/{sha}", cfg.owner, cfg.repo)
}

/// Join a directory prefix and an entry name, keeping root-level paths bare.
fn join_path(prefix: &str, name: &str) -> String {
    if prefix.is_empty() { name.to_string() } else { format!("{prefix}/{name}") }
}

/// Keep markdown notes plus `.gitkeep` placeholders so empty folders (created
/// via "New folder" / Kanban columns) still appear in the tree; drop trees and
/// non-note blobs.
fn keep_blob(path: &str) -> bool {
    let ext = std::path::Path::new(path).extension();
    ext.is_some_and(|e| e.eq_ignore_ascii_case("md")) || path.ends_with(".gitkeep")
}

fn tree_to_files(tree: TreeResponse) -> Vec<FileMeta> {
    tree.tree
        .into_iter()
        .filter(|e| e.kind == "blob" && keep_blob(&e.path))
        .map(|e| FileMeta {
            path: e.path,
            sha: e.sha,
            size: e.size.unwrap_or(0),
        })
        .collect()
}

async fn fetch_tree(cfg: &GithubConfig, url: &str) -> Result<TreeResponse, VaultError> {
    let resp = get(url, &cfg.token)
        .send()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;
    check(resp)?
        .json()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))
}

/// The whole vault listing, normally in a single recursive tree request.
///
/// If GitHub reports the recursive listing as `truncated` (~100k entries or
/// 7 MB — reachable in a big vault with attachments), the response is silently
/// missing files, which would show up as notes vanishing from the tree and as
/// wrong query results once the index is built on top of this. In that case we
/// fall back to walking one directory at a time, which costs one request per
/// directory but is complete. The fallback is rare enough not to be worth
/// parallelising.
pub async fn list_files(cfg: &GithubConfig) -> Result<Vec<FileMeta>, VaultError> {
    let tree = fetch_tree(cfg, &tree_url(cfg)).await?;
    if !tree.truncated {
        return Ok(tree_to_files(tree));
    }
    walk_tree(cfg).await
}

/// Breadth-first directory walk, used only when the recursive listing truncates.
async fn walk_tree(cfg: &GithubConfig) -> Result<Vec<FileMeta>, VaultError> {
    let mut files = Vec::new();
    // (tree sha, path prefix); the branch name resolves as the root tree.
    let mut queue = vec![(cfg.branch.clone(), String::new())];
    while let Some((sha, prefix)) = queue.pop() {
        let tree = fetch_tree(cfg, &subtree_url(cfg, &sha)).await?;
        for entry in tree.tree {
            let path = join_path(&prefix, &entry.path);
            match entry.kind.as_str() {
                "tree" => queue.push((entry.sha, path)),
                "blob" if keep_blob(&path) => files.push(FileMeta {
                    path,
                    sha: entry.sha,
                    size: entry.size.unwrap_or(0),
                }),
                _ => {}
            }
        }
    }
    Ok(files)
}

// ── read_file ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ContentsResponse {
    content: String,
    sha: String,
}

fn contents_url(cfg: &GithubConfig, path: &str) -> String {
    format!("{API}/repos/{}/{}/contents/{path}", cfg.owner, cfg.repo)
}

/// Decode GitHub's base64 blob content into a normalised UTF-8 string.
/// GitHub base64-encodes with a newline every 60 chars and can serve CRLF, both
/// of which we strip/normalise (raw CR breaks the tokenizer).
fn decode_content(b64: &str, sha: String) -> Result<FileContent, VaultError> {
    let raw = b64.replace('\n', "");
    let bytes = STANDARD
        .decode(&raw)
        .map_err(|e| VaultError::Decode(e.to_string()))?;
    let content = String::from_utf8(bytes).map_err(|e| VaultError::Decode(e.to_string()))?;
    let content = content.replace("\r\n", "\n").replace('\r', "\n");
    Ok(FileContent { content, sha })
}

pub async fn read_file(cfg: &GithubConfig, path: &str) -> Result<FileContent, VaultError> {
    let resp = get(&contents_url(cfg, path), &cfg.token)
        .send()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;
    let body: ContentsResponse = check(resp)?
        .json()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;
    decode_content(&body.content, body.sha)
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
    let url = contents_url(cfg, path);
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

    let written: WriteResponse = check(resp)?
        .json()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;

    Ok(written.content.sha)
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
    let url = contents_url(cfg, path);
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

    let written: WriteResponse = check(resp)?
        .json()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;

    Ok(written.content.sha)
}

// ── delete_file ───────────────────────────────────────────────────────────────

pub async fn delete_file(
    cfg: &GithubConfig,
    path: &str,
    sha: &str,
    message: &str,
) -> Result<(), VaultError> {
    let url = contents_url(cfg, path);
    let body = serde_json::json!({
        "message": message,
        "sha": sha,
        "branch": cfg.branch,
    });
    let resp = request(reqwest::Method::DELETE, &url, &cfg.token)
        .json(&body)
        .send()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;
    check(resp)?;
    Ok(())
}

// ── OAuth Device Flow ─────────────────────────────────────────────────────────

pub const GITHUB_CLIENT_ID: &str = "Ov23li0fTUa8YSbUsWwI";

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    /// Pre-filled URL including the `user_code` as a query param — open this
    /// directly so the user just has to click Authorize, no typing needed.
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub expires_in: u32,
    pub interval: u32,
}

#[derive(Debug, PartialEq)]
pub enum PollOutcome {
    Token(String),
    Pending,
    SlowDown(u32),
    Expired,
    Denied,
}

/// Map the device-flow poll response fields to an outcome. A present token wins;
/// otherwise the `error` code selects the state (unknown/absent = still pending).
fn classify_poll(access_token: Option<String>, error: Option<&str>, interval: Option<u32>) -> PollOutcome {
    if let Some(token) = access_token {
        return PollOutcome::Token(token);
    }
    match error {
        Some("slow_down")     => PollOutcome::SlowDown(interval.unwrap_or(10)),
        Some("expired_token") => PollOutcome::Expired,
        Some("access_denied") => PollOutcome::Denied,
        _                     => PollOutcome::Pending,
    }
}

pub async fn request_device_code() -> Result<DeviceCodeResponse, VaultError> {
    let resp = crate::http::client()
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
    let resp = crate::http::client()
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
    Ok(classify_poll(body.access_token, body.error.as_deref(), body.interval))
}

pub async fn get_username(token: &str) -> Result<String, VaultError> {
    #[derive(serde::Deserialize)]
    struct User { login: String }
    let resp = get("https://api.github.com/user", token)
        .send()
        .await
        .map_err(|e| VaultError::Http(e.to_string()))?;
    let user: User = check(resp)?.json().await.map_err(|e| VaultError::Http(e.to_string()))?;
    Ok(user.login)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};

    fn cfg() -> GithubConfig {
        GithubConfig {
            token: "tok".into(),
            owner: "me".into(),
            repo: "notes".into(),
            branch: "main".into(),
            provider: crate::Provider::GitHub,
            templates_dir: String::new(),
            daily_note_template: String::new(),
            weekly_note_template: String::new(),
            monthly_note_template: String::new(),
        }
    }

    #[test]
    fn builds_expected_urls() {
        assert_eq!(
            tree_url(&cfg()),
            "https://api.github.com/repos/me/notes/git/trees/main?recursive=1"
        );
        assert_eq!(
            contents_url(&cfg(), "sub/idea.md"),
            "https://api.github.com/repos/me/notes/contents/sub/idea.md"
        );
    }

    #[test]
    fn tree_keeps_only_md_and_gitkeep_blobs() {
        let json = r#"{"tree":[
            {"path":"a.md","type":"blob","sha":"s1","size":10},
            {"path":"img.png","type":"blob","sha":"s2","size":20},
            {"path":"dir","type":"tree","sha":"s3","size":null},
            {"path":"empty/.gitkeep","type":"blob","sha":"s4","size":null}
        ]}"#;
        let tree: TreeResponse = serde_json::from_str(json).unwrap();
        let files = tree_to_files(tree);
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["a.md", "empty/.gitkeep"]);
        // Missing size defaults to 0.
        assert_eq!(files[1].size, 0);
        assert_eq!(files[0].sha, "s1");
    }

    #[test]
    fn truncated_flag_is_parsed_and_defaults_to_false() {
        let plain: TreeResponse = serde_json::from_str(r#"{"tree":[]}"#).unwrap();
        assert!(!plain.truncated, "absent flag must not be treated as truncated");
        let cut: TreeResponse =
            serde_json::from_str(r#"{"tree":[],"truncated":true}"#).unwrap();
        assert!(cut.truncated, "a truncated listing must be detected, not silently used");
    }

    #[test]
    fn subtree_url_and_path_joining_for_the_walk_fallback() {
        assert_eq!(
            subtree_url(&cfg(), "abc123"),
            "https://api.github.com/repos/me/notes/git/trees/abc123"
        );
        // Root entries stay bare; nested ones get the prefix.
        assert_eq!(join_path("", "a.md"), "a.md");
        assert_eq!(join_path("notes", "a.md"), "notes/a.md");
        assert_eq!(join_path("notes/sub", "a.md"), "notes/sub/a.md");
    }

    #[test]
    fn keep_blob_matches_the_recursive_filter() {
        assert!(keep_blob("a.md"));
        assert!(keep_blob("empty/.gitkeep"));
        assert!(!keep_blob("img.png"));
    }

    #[test]
    fn decode_content_strips_newlines_and_normalises_crlf() {
        // GitHub wraps base64 at 60 cols and may embed CRLF in the payload.
        let encoded = STANDARD.encode("line1\r\nline2\rline3");
        let wrapped = format!("{}\n{}", &encoded[..4], &encoded[4..]);
        let fc = decode_content(&wrapped, "sha1".into()).unwrap();
        assert_eq!(fc.content, "line1\nline2\nline3");
        assert_eq!(fc.sha, "sha1");
    }

    #[test]
    fn decode_content_rejects_invalid_base64() {
        assert!(matches!(
            decode_content("!!not base64!!", "s".into()),
            Err(VaultError::Decode(_))
        ));
    }

    #[test]
    fn classify_poll_prefers_token() {
        assert_eq!(
            classify_poll(Some("gho_x".into()), Some("slow_down"), Some(5)),
            PollOutcome::Token("gho_x".into())
        );
    }

    #[test]
    fn classify_poll_maps_error_codes() {
        assert_eq!(classify_poll(None, Some("slow_down"), Some(7)), PollOutcome::SlowDown(7));
        assert_eq!(classify_poll(None, Some("slow_down"), None), PollOutcome::SlowDown(10));
        assert_eq!(classify_poll(None, Some("expired_token"), None), PollOutcome::Expired);
        assert_eq!(classify_poll(None, Some("access_denied"), None), PollOutcome::Denied);
        // Unknown/absent error while still waiting.
        assert_eq!(classify_poll(None, Some("authorization_pending"), None), PollOutcome::Pending);
        assert_eq!(classify_poll(None, None, None), PollOutcome::Pending);
    }

    #[test]
    fn status_error_maps_auth_and_not_found() {
        assert!(matches!(status_error(401, "/x"), Some(VaultError::Unauthorized)));
        assert!(matches!(status_error(404, "/notes/a.md"), Some(VaultError::NotFound(p)) if p == "/notes/a.md"));
        // 409/422 are handled at their call sites, not here; success defers.
        assert!(status_error(409, "/x").is_none());
        assert!(status_error(200, "/x").is_none());
    }
}
