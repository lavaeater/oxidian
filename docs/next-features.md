# Oxidian: next features — design notes

2026-10-20 · Tommie Nygren · living copy: https://claude.ai/code/artifact/5cde3fb0-11e5-44fb-91a0-41c695bf0038

## Summary

Build section-by-section review first: it extends code that already exists (the review modal, `migrate_tasks`, wikilinks) and needs nothing new on Android. Voice input comes second, then the desktop local-clone backend. Reminders come last, because they are the only idea that needs native Android code or a server that runs on its own.

| # | Idea | What it mainly needs | Rough size | Risk |
| --- | --- | --- | --- | --- |
| 1 | Section-by-section review | A section splitter in `index`, a "Notes" step in `MigrationReview`, a destination picker | Small–medium | Low: same write-dest-first pattern as task migration |
| 2 | Voice input | Audio capture in the webview, a self-hosted Whisper container, a settings field | Medium | Medium: microphone permission inside the Android WebView is unproven |
| 3 | Desktop on a local clone | A `Local` provider in `vault` behind the existing `dispatch` calls, plus git sync | Medium–large | Medium: sync and merge conflicts |
| 4 | Reminders | A reminder marker in task lines, plus either native Android notifications (JNI) or a small server that pushes | Large | High: background delivery is where Android fights back |

Nothing here needs an Oxidian backend except reminders, and even there the server is optional (see that section).

## 1. Section-by-section review

Add a second step to the review: after deciding about tasks, walk each source log's note sections and file each one away, leaving a wikilink behind. Use the same order as task migration: write the destination first, then replace the section in the source. A failure between the two writes then duplicates the text instead of losing it.

**What counts as a section**

- Split each log at its `##` headings. The `#` title and anything before the first `##` stay where they are.
- Skip the template's task section (`## Log` in the shipped template). Detect it from the template, not the name: a heading in the template body whose section holds the template's checkboxes.
- Skip sections that are empty or contain only tasks.
- Deeper headings (`###`) travel with their parent `##`. A per-section option could split at `###` later, if long logs need it.

**The step, per section**

- Show the heading and a preview of the first few lines.
- Offer three ways out: *Keep* (the default, so skipping is free), *Move to note…*, and *Move to new note…*.
- *Move to note…* opens a picker over existing notes, filtered to the destination folders. *New note* asks for a name and folder, with the section heading as the default name.

**The write, per moved section**

1. Append the section to the destination, under its original heading, followed by a back-reference line such as `From [[2026-10-19]]`.
2. Replace the section body in the source log with a link, and keep the heading: `## Meeting with Anna` followed by `→ [[Projects/Website#Meeting with Anna]]`. Keeping the heading means the day log still reads as an index of what happened.
3. Update the index for both files (`vault_index::update_page`), as `migrate_tasks` already does.

**Where things go**

- Add one bujo plugin setting, *Destination folders*, defaulting to `Projects/`, `Areas/`, `Resources/` (PARA). The picker shows those folders first and falls back to the whole vault.
- Nothing enforces PARA. The setting only orders the picker.

**Code touchpoints**

- `index`: a pure `sections(content) -> Vec<Section { heading, level, start_line, end_line }>` with unit tests, next to `extract`.
- `views/vault.rs`: `ReviewState` gains the parsed sections. `MigrationReview` gets a second page, and `apply_review` gets a `move_sections` call after `migrate_tasks`.
- The existing "Move to…" picker in `vault.rs` can be reused for the destination.
- On a phone, show one section per screen with the three buttons at the bottom; a list of twenty previews doesn't fit.

## 2. Reminders

Store a reminder as a marker on the task line, and choose how it gets delivered separately. The marker is cheap and useful on its own. Delivery is the expensive part: Oxidian has no backend, and a webview app that isn't running can't wake itself up.

**The data: one marker in the markdown**

- Use `⏰ 2026-10-21 09:00` on a task line, next to the existing `📅` due date. As far as I recall, Obsidian's Reminder plugin accepts this emoji format, which would keep the vault portable (unverified).
- `index::tasks` parses it into `Task.remind: Option<String>`, the same way it parses `due`.
- Completing, dropping or migrating a task clears its reminder. Migration carries the marker forward with the task.
- Times are local wall-clock time, with no time zone. The one exception is the server option below, which needs a configured zone.

**Delivery: three options, from cheapest to most native**

| Option | How it works | Fires when Oxidian is closed | Effort |
| --- | --- | --- | --- |
| A. In-app agenda | On open and on focus, show due reminders as a banner: "3 reminders due". Inside the app only. | No | Small |
| B. Self-hosted push server | A small Docker container pulls the vault every minute, finds due `⏰` lines, and sends them to [ntfy](https://ntfy.sh). The ntfy Android app shows the notification. | Yes, on the phone and the desktop alike | Medium, mostly outside Oxidian |
| C. Native Android alarms | The mobile shell schedules an exact alarm per reminder through JNI (`AlarmManager`), then posts a notification. | Yes, Android only | Large: Kotlin/JNI glue, permissions, rescheduling after reboot |

**Recommendation:** start with A, add B if you want real notifications. B suits you because you're happy to run Docker. It keeps all Android-specific code out of Oxidian, and it fires even if the app was never opened that day. Its cost is about a minute of lag, plus one more service to keep running.

**Notes on option C, if it's ever wanted**

- Since Android 12, exact alarms need the `SCHEDULE_EXACT_ALARM` permission, and Android 13 adds the `POST_NOTIFICATIONS` runtime prompt.
- Alarms are lost on reboot unless a `BOOT_COMPLETED` receiver re-registers them. That receiver needs the reminder list stored on the device, because it can't read the vault without the app.
- Dioxus mobile currently gives the app only a webview. Every one of these steps needs custom Android code that `dx` bundles in, which is the same problem the `android-icons` skill works around.

## 3. Voice input

Record audio in the webview, send it to a self-hosted Whisper server, and insert the text. That one path works the same way on web and Android, because both are webviews running the same `app` code. Browser speech recognition is not an option: Android's WebView doesn't provide the Web Speech API's recogniser, and Chrome's version sends the audio to Google anyway.

**The flow**

1. A mic button in the toolbar, plus a "Voice note" command in the palette, starts recording with `MediaRecorder` in `oxidian.js`, bound through `use_js!` like the other browser glue.
2. Stopping the recording uploads the audio blob straight from the browser to the transcription server. Oxidian has no server of its own, so nothing sits in between.
3. The transcript is inserted at the cursor, or appended to today's log as `- 🎙 <text>` when started from the palette.
4. Optionally, the audio file can be kept as an attachment. This is off by default: a git vault grows fast with binary files.

**The server: a Docker container**

- Run an OpenAI-compatible Whisper server built on faster-whisper; several open-source images expose `POST /v1/audio/transcriptions`. The client then has a single small contract to code against, and you can swap the engine later.
- Model: `large-v3-turbo` handles Swedish and English well and runs acceptably on a CPU for short notes. Your RX 7600 could accelerate it through ROCm, but CPU is the simpler first step.
- The phone has to reach the server. Use Tailscale, or a reverse proxy with TLS. The web build is served over HTTPS, so the endpoint must be HTTPS as well, and it must send CORS headers for the Oxidian origin.
- Authentication: a bearer token in the request.

**Settings**

- Transcription endpoint URL and token live in `localStorage` per device, like the git token. They are not stored in the vault, because the token is a secret and the vault is a git repo.
- Language: auto-detect or a fixed language; fixing it is faster and more accurate.

**Risks to check first**

- **Microphone access in the Android WebView.** The app needs `RECORD_AUDIO` in the manifest, and the webview host must grant the page's permission request. I haven't checked whether wry on Android does that today. Build a 30-minute spike of `getUserMedia` on the phone before anything else.
- Long recordings: keep a limit of a few minutes per note, or chunk uploads. That avoids timeouts behind a proxy.

**Later:** an optional clean-up step, where an LLM turns a rambling transcript into bullets and `- [ ]` tasks, fits the BuJo rapid-logging style well.

## 4. Desktop client on a local clone

Add a third provider, `Local { path }`, to `vault` beside GitHub and GitLab, and keep every caller unchanged. All I/O already goes through `vault::dispatch` (`list_files`, `read_file`, `write_file`, `create_file`, `delete_file`, `move_file`, `move_dir`), so the app above it shouldn't notice the difference.

**Mapping the calls**

- `list_files` walks the working tree and skips `.git`.
- `read_file` returns the content plus the git blob SHA, computed as `sha1("blob <len>\0" + bytes)`. That is the same SHA the GitHub API returns, so the existing conflict checks keep working unchanged.
- `write_file` refuses when the file on disk no longer hashes to the SHA the caller read. That detects an edit made outside Oxidian, for example in Obsidian or a text editor, the same way a stale SHA does on GitHub.
- `move_file` and `move_dir` become plain renames.

**Git: commit locally, sync in the background**

- Each write is a local commit, with the same commit messages the app already generates. Commits are cheap, and the history stays as detailed as it is today.
- A sync loop runs fetch, then rebase, then push: on start, every few minutes, and on demand from a button. Use the `git` CLI first, since it's simplest and respects your SSH keys and credential helpers. Move to `gix` only if shelling out becomes a problem.
- A rebase conflict stops the sync. The app then shows which files conflict and leaves the repo for you to resolve. Auto-merging markdown is not worth the risk.
- A file watcher (the `notify` crate) refreshes the file list and index when files change on disk. That makes it safe to keep Obsidian open on the same folder.

**Where it fits**

- `vault` stays free of Dioxus; the local backend is plain `std::fs` and `std::process`. It compiles only for non-wasm targets, behind a `local` feature.
- Settings gets a third provider choice with a folder picker. That choice only appears on desktop builds.
- Android could later use the same backend on app storage, but bundling git for Android is a separate project. The REST providers already work there.

**Payoff:** offline editing, instant reads with no API rate limits, and coexistence with Obsidian and other tools on the same files.

## Open questions

- [ ] Section review: should a moved section keep its heading in the day log with a link under it, or collapse to a single link line?
- [ ] Section review: do you already use PARA folders (`Projects/`, `Areas/`, `Resources/`) in the vault, or different names?
- [ ] Reminders: is a minute of lag from a self-hosted ntfy server acceptable, or do you want alarms that fire exactly on time?
- [ ] Voice: where will the Whisper container run (this desktop, a home server, a VPS), and can the phone reach it over Tailscale?
- [ ] Voice: keep the audio files in the vault, or transcript only?
- [ ] Desktop: is offline use or Obsidian coexistence the main reason for the local clone? That decides whether the file watcher comes in the first version.
