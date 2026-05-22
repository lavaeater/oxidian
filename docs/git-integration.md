# Git Integration — Design Plan

## The goal

Oxidian should work across web, desktop, and mobile without requiring the user to run a dedicated backend server. A user
who only wants the web app should be able to host a single static WASM binary and connect it to their GitHub/GitLab
account directly — no Heroku, no Railway, no VPS.

---

## Can we go backend-free? Yes, mostly.

The key insight: modern hosted git platforms (GitHub, GitLab, Gitea) expose REST APIs that support CORS, meaning browser
WASM code can call them directly with a token. There is no need for a proxy or server to relay API requests.

| Scenario                           | Backend needed? | Notes                                                  |
|------------------------------------|-----------------|--------------------------------------------------------|
| GitHub API + Personal Access Token | **No**          | Direct CORS API calls from WASM                        |
| GitHub OAuth (web flow)            | Yes, tiny       | Client secret must stay server-side                    |
| GitHub OAuth Device Flow           | **No**          | User enters a code at github.com; app polls for token  |
| GitLab API + PAT                   | **No**          | Same as GitHub                                         |
| Self-hosted Gitea/Forgejo + PAT    | **No**          | Same API shape, user provides base URL                 |
| Local git repo on disk (desktop)   | **No**          | libgit2 via `git2` crate, no network at all            |
| Local git repo in browser (web)    | Partial         | OPFS + isomorphic-git possible, but remote CORS varies |

**Conclusion**: For the initial scope (GitHub + GitLab + local desktop) we can be fully backend-free. We should design
for it now and not paint ourselves into a corner.

The one exception is GitHub's standard OAuth web flow. The workaround is **Device Flow** (RFC 8628), which GitHub
supports: the app shows a code, the user visits github.com/login/device and enters it, the app polls until the token
arrives. Zero server required.

---

## The storage backend abstraction

Everything git-related lives behind a single async Rust trait. Platform implementations swap in behind it; the UI never
calls a git API directly.

```rust
/// One vault = one git repository (or directory).
#[async_trait]
pub trait VaultBackend: Send + Sync {
    // ── File operations ────────────────────────────────────────────────────
    async fn list_files(&self) -> Result<Vec<FileMeta>>;
    async fn read_file(&self, path: &str) -> Result<String>;
    async fn write_file(&self, path: &str, content: &str, message: &str) -> Result<CommitMeta>;
    async fn delete_file(&self, path: &str, message: &str) -> Result<CommitMeta>;
    async fn rename_file(&self, from: &str, to: &str, message: &str) -> Result<CommitMeta>;

    // ── Branch / history ──────────────────────────────────────────────────
    async fn list_branches(&self) -> Result<Vec<String>>;
    async fn current_branch(&self) -> Result<String>;
    async fn switch_branch(&self, branch: &str) -> Result<()>;
    async fn recent_commits(&self, path: Option<&str>, limit: usize) -> Result<Vec<CommitMeta>>;
}

pub struct FileMeta {
    pub path: String,
    pub size: usize,
    pub sha: String,   // blob SHA — required for update operations on GitHub/GitLab
}

pub struct CommitMeta {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub timestamp: i64,
}
```

**Implementations planned:**

| Struct             | Backend             | Platforms           |
|--------------------|---------------------|---------------------|
| `GitHubApiBackend` | GitHub REST API v3  | all (WASM + native) |
| `GitLabApiBackend` | GitLab REST API v4  | all                 |
| `GiteaApiBackend`  | Gitea / Forgejo API | all                 |
| `LocalGitBackend`  | `git2` (libgit2)    | desktop + mobile    |
| `MemoryBackend`    | in-memory map       | tests               |

`LocalGitBackend` is only compiled on platforms with filesystem access (desktop/mobile feature flags). On web, only the
API backends are available.

---

## Platform breakdown

### Web (WASM)

**Auth**: Personal Access Token (simplest) or GitHub Device Flow (no server). Token stored in `localStorage` —
acceptable for a self-hosted tool, noted clearly in docs.

**File reads/writes**: Direct calls to the platform REST API over HTTPS. GitHub's API fully supports CORS, so WASM can
call it without any proxy. One write = one commit. For batching multiple file changes into a single commit, the Git Data
API is used: create blob → create/update tree → create commit → update ref. More complex but gives clean history.

**Local repo on web**: Not feasible in the initial version. The browser can store a working copy in OPFS (Origin Private
File System) and use isomorphic-git for local operations, but cloning from arbitrary remotes requires CORS headers on
the remote — which GitHub provides but self-hosted instances may not. Treat this as a future enhancement; for now, web =
API only.

### Desktop (native)

**Auth**: PAT or OAuth for remote repos; none for local repos.

**Local repo**: `git2-rs` bindings to libgit2. Full local git — stage, commit, push, pull, branch — with no network
required if the repo is on disk. For SSH, keys come from the system agent. For HTTPS, `git2` handles credential helpers.

**Remote sync**: Push/pull are explicit user actions. UI shows ahead/behind count. Auto-save commits locally; user
pushes when ready.

### Mobile (iOS + Android)

**Auth**: PAT stored in the platform keychain. Device Flow works well (just opens a browser tab for the user-code step).

**Storage**: API-only for remote vaults on first release. For local use, Dioxus mobile has access to the app's sandboxed
document directory; `git2-rs` can operate there for a proper local-first workflow.

**No push/pull UI on first cut** — read/write via API, pull-to-refresh fetches the latest tree.

---

## Authentication strategies

### Personal Access Token (PAT) — all platforms, no server

The user generates a token on github.com (or gitlab.com etc.) with `repo` scope and pastes it into Oxidian settings. The
app stores it and sends it as `Authorization: Bearer <token>` on every request.

No server. No OAuth dance. Works on all platforms. This is the right starting point.

### OAuth Device Flow — web + mobile, no server

```
1. App POSTs to /login/device/code → receives device_code + user_code
2. App shows: "Visit github.com/login/device and enter: ABCD-1234"
3. App polls /login/oauth/access_token every 5 seconds
4. User completes the web step → next poll returns the access token
```

GitHub, GitLab (via PKCE variant), and Gitea all support this. The client ID is not a secret and can be embedded in the
binary. No server involvement at any point.

### OAuth Web Flow — optional, requires a tiny server

If a hosted version is ever offered (so users don't have to self-host), a stateless serverless function (~50 lines,
Cloudflare Workers) handles the `code → token` exchange. The function holds the client secret; it stores nothing. This
is a later concern and should not drive the initial architecture.

---

## File identity and conflict handling

When the app reads a file via the API it receives a blob SHA. When writing back, the SHA is sent as a precondition
(GitHub: the `"sha"` field in the PUT body). If the file changed remotely between read and write, the API returns 409
Conflict. The app should:

1. Re-fetch the latest remote version.
2. Offer a simple conflict resolution UI: keep yours / keep theirs / show diff.

For the local backend, `git2` handles this natively via merge.

**Remote polling**: The app should periodically check whether the remote SHA for open files has changed (e.g. every 60
seconds, or on window focus). This is a lightweight HEAD request and catches the case where another device pushed a
change.

---

## Implementation phases

### Phase 1 — Read-only vault browser (web + desktop)

- Define `VaultBackend` trait in `packages/api` (or a new `packages/vault` crate)
- Implement `GitHubApiBackend` read-only: `list_files`, `read_file`
- Auth: PAT input in settings panel, stored in localStorage / keychain
- File tree sidebar component
- Clicking a file opens it in `MarkdownArea`

Deliverable: browse and read any GitHub-hosted vault.

### Phase 2 — Save (single-file commit)

- `write_file` on `GitHubApiBackend` using the Contents API (PUT with blob SHA)
- Auto-save after N seconds of inactivity (configurable) or explicit Ctrl+S
- Show "saved · 2s ago" / "unsaved changes" status indicator

Deliverable: full read/write editing loop, one commit per save.

### Phase 3 — Local backend (desktop)

- New `packages/vault` crate with `LocalGitBackend` via `git2-rs`
- Open a local folder as vault (directory picker)
- Stage + commit on every save
- Push / pull buttons with status badge (N commits ahead, M behind)
- Entirely offline-capable

Deliverable: desktop app works with zero network, syncs on demand.

### Phase 4 — Multi-file commits + branch management

- Batch writes into one commit via the low-level Git Data API (blob → tree → commit → ref update)
- Create / switch branches from the UI
- File history panel (list commits touching a file)
- Rename and delete files

### Phase 5 — GitLab / Gitea / self-hosted adapters

- Abstract credential storage behind a `Credentials` type (token + base URL + provider kind)
- Implement `GitLabApiBackend` and `GiteaApiBackend` (API shapes are similar to GitHub)
- Settings UI: choose provider, enter base URL, authenticate

---

## What NOT to build

- **No own git server.** Oxidian is a client for existing git hosts.
- **No always-on backend.** The OAuth exchange function is the only server-side piece, and it is optional (Device Flow
  avoids it entirely).
- **No custom sync protocol.** Git is the sync protocol. We ride on top of it.
- **No full git UI.** This is not a git GUI. Commits, push, and pull are the surface area. Rebasing, merging, and
  cherry-picking are out of scope.

---

## Open questions

**Conflict UX**: No design yet for surfacing remote conflicts in the editor. Needs a non-disruptive approach (toast
notification? sidebar indicator?).

**Binary assets**: Images in notes are binary. The GitHub API returns them base64-encoded in the contents response. The
image renderer needs to handle `data:` URIs generated from these.

**Large repos**: The `?recursive=1` tree endpoint returns every file in one shot, which can be thousands of entries. The
file tree needs a virtual list and lazy expansion.

**WikiLink resolution**: Resolving `[[Note Name]]` to a file path requires the full file tree in memory. Cache it on
load; invalidate on save or remote poll.

**Mobile local repos**: iOS exposes a user-accessible Documents directory; Android has SAF. Accessing either from Dioxus
mobile may need native bridge code. Defer to a later phase.
