pub mod github;

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

/// Connection settings for a GitHub-hosted vault.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GithubConfig {
    pub token: String,
    pub owner: String,
    pub repo: String,
    /// Branch to read from (defaults to "main").
    #[serde(default = "default_branch")]
    pub branch: String,
}

fn default_branch() -> String {
    "main".to_string()
}
