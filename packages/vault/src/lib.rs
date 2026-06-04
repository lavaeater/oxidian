pub mod github;
pub mod gitlab;

pub use github::{DeviceCodeResponse, PollOutcome, request_device_code, poll_device_token, get_username};

#[derive(thiserror::Error, Debug, Clone)]
pub enum VaultError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Decode error: {0}")]
    Decode(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Unauthorized — check your token")]
    Unauthorized,
    #[error("Conflict — file changed remotely, refresh to get the latest version")]
    Conflict,
}

/// Metadata for a file in the vault (returned by list_files).
#[derive(Debug, Clone, PartialEq)]
pub struct FileMeta {
    /// Repo-relative path, e.g. "notes/idea.md"
    pub path: String,
    /// Git blob SHA — required for write operations.
    pub sha: String,
    pub size: usize,
}

impl FileMeta {
    /// The bare filename without directory prefix.
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// Directory portion, empty string for root files.
    pub fn dir(&self) -> &str {
        match self.path.rfind('/') {
            Some(i) => &self.path[..i],
            None => "",
        }
    }
}

/// File content returned by read_file.
#[derive(Debug, Clone, Default)]
pub struct FileContent {
    pub content: String,
    /// Current blob SHA — needed to write the file back.
    pub sha: String,
}

/// A single result from a code search.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: String,
    pub sha: String,
    /// Best matching text fragment returned by the GitHub text-match API.
    pub fragment: String,
}

/// A `[[WikiLink]]` extracted from a file.
#[derive(Debug, Clone, PartialEq)]
pub struct WikiLink {
    /// The link target as written (e.g. `"My Note"` from `[[My Note]]`).
    pub target: String,
    /// Source file containing this link.
    pub source_path: String,
}

/// Which git hosting provider the vault is on.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum Provider {
    #[default]
    GitHub,
    GitLab,
}

impl Provider {
    pub fn label(&self) -> &'static str {
        match self { Provider::GitHub => "GitHub", Provider::GitLab => "GitLab" }
    }
}

/// Dispatch table — calls the right backend based on the config's provider.
pub mod dispatch {
    use super::*;

    pub async fn list_files(cfg: &GithubConfig) -> Result<Vec<FileMeta>, VaultError> {
        match cfg.provider {
            Provider::GitHub  => github::list_files(cfg).await,
            Provider::GitLab  => gitlab::list_files(cfg).await,
        }
    }
    pub async fn read_file(cfg: &GithubConfig, path: &str) -> Result<FileContent, VaultError> {
        match cfg.provider {
            Provider::GitHub  => github::read_file(cfg, path).await,
            Provider::GitLab  => gitlab::read_file(cfg, path).await,
        }
    }
    pub async fn write_file(cfg: &GithubConfig, path: &str, content: &str, sha: &str, msg: &str) -> Result<String, VaultError> {
        match cfg.provider {
            Provider::GitHub  => github::write_file(cfg, path, content, sha, msg).await,
            Provider::GitLab  => gitlab::write_file(cfg, path, content, sha, msg).await,
        }
    }
    pub async fn create_file(cfg: &GithubConfig, path: &str, content: &str, msg: &str) -> Result<String, VaultError> {
        match cfg.provider {
            Provider::GitHub  => github::create_file(cfg, path, content, msg).await,
            Provider::GitLab  => gitlab::create_file(cfg, path, content, msg).await,
        }
    }
    pub async fn delete_file(cfg: &GithubConfig, path: &str, sha: &str, msg: &str) -> Result<(), VaultError> {
        match cfg.provider {
            Provider::GitHub  => github::delete_file(cfg, path, sha, msg).await,
            Provider::GitLab  => gitlab::delete_file(cfg, path, sha, msg).await,
        }
    }
    pub async fn read_many(cfg: &GithubConfig, paths: &[String]) -> Vec<(String, String)> {
        match cfg.provider {
            Provider::GitHub  => github::read_many(cfg, paths).await,
            Provider::GitLab  => github::read_many(cfg, paths).await, // uses same sequential pattern
        }
    }

    /// Move a single file. Neither provider's Contents API has a native move, so
    /// this is create-at-new-path then delete-old. `create_file` fails if the
    /// destination already exists, so on a collision the original is left intact.
    pub async fn move_file(cfg: &GithubConfig, old_path: &str, old_sha: &str, new_path: &str) -> Result<(), VaultError> {
        if old_path == new_path { return Ok(()); }
        let msg = format!("Move {old_path} → {new_path}");
        let fc = read_file(cfg, old_path).await?;
        create_file(cfg, new_path, &fc.content, &msg).await?;
        delete_file(cfg, old_path, old_sha, &msg).await?;
        Ok(())
    }

    /// Move every file under `old_prefix/` to `new_prefix/…`, preserving the
    /// sub-path. `files` is the current vault listing (for paths + SHAs).
    /// Rejects moving a folder into itself or one of its own descendants.
    pub async fn move_dir(cfg: &GithubConfig, old_prefix: &str, new_prefix: &str, files: &[FileMeta]) -> Result<(), VaultError> {
        let old_prefix = old_prefix.trim_matches('/');
        let new_prefix = new_prefix.trim_matches('/');
        if old_prefix == new_prefix { return Ok(()); }
        if new_prefix == old_prefix || new_prefix.starts_with(&format!("{old_prefix}/")) {
            return Err(VaultError::Http("Cannot move a folder into itself".into()));
        }
        let strip = format!("{old_prefix}/");
        for file in files {
            if let Some(rel) = file.path.strip_prefix(&strip) {
                let dest = format!("{new_prefix}/{rel}");
                move_file(cfg, &file.path, &file.sha, &dest).await?;
            }
        }
        Ok(())
    }
}

/// Connection settings for a vault (works for GitHub and GitLab).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GithubConfig {
    pub token: String,
    pub owner: String,
    pub repo: String,
    #[serde(default = "default_branch")]
    pub branch: String,
    #[serde(default)]
    pub provider: Provider,
    #[serde(default = "default_templates_dir")]
    pub templates_dir: String,
    #[serde(default = "default_daily_note_template")]
    pub daily_note_template: String,
}

fn default_branch() -> String { "main".to_string() }
fn default_templates_dir() -> String { ".oxidian/templates".to_string() }
fn default_daily_note_template() -> String { ".oxidian/templates/daily-note.md".to_string() }
