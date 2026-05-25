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
    pub async fn read_many(cfg: &GithubConfig, paths: &[String]) -> Vec<(String, String)> {
        match cfg.provider {
            Provider::GitHub  => github::read_many(cfg, paths).await,
            Provider::GitLab  => github::read_many(cfg, paths).await, // uses same sequential pattern
        }
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
}

fn default_branch() -> String {
    "main".to_string()
}
