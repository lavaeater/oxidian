# Oxidian — Plugin Architecture

## Why JavaScript

Oxidian runs as WASM in a browser and in a WebView on Android and desktop. You
can't dynamically load compiled Rust at runtime, but **every platform always has
a JavaScript runtime**. Plugins are therefore JavaScript files executed inside
the same WebView context that renders the app.

This is the same model Obsidian uses (Electron / Node.js) and it means plugin
authors can write ordinary JS — no build toolchain required.

---

## Plugin layout in the vault

Plugins live inside the vault itself, version-controlled alongside notes.

```
.oxidian/
  plugins/
    periodic-notes/
      manifest.json
      main.js
    backlink-graph/
      manifest.json
      main.js
  templates/
    daily-note.md
```

### `manifest.json`

```json
{
  "id": "periodic-notes",
  "name": "Periodic Notes",
  "version": "1.0.0",
  "description": "Daily, weekly, and monthly notes from templates.",
  "author": "Jane Smith",
  "minOxidianVersion": "0.2.0"
}
```

---

## The host API (`window.oxidian`)

Oxidian injects this object before any plugin code runs.

```ts
interface OxidianAPI {
  vault: {
    listFiles(): Promise<FileMeta[]>;
    readFile(path: string): Promise<string>;
    writeFile(path: string, content: string): Promise<void>;
    createFile(path: string, content: string): Promise<void>;
    openFile(path: string): void;
  };

  ui: {
    // Registers a command that appears in the slash menu and command palette.
    registerCommand(id: string, name: string, callback: () => void): void;

    // Adds a persistent icon button to the sidebar header.
    addSidebarButton(icon: string, title: string, callback: () => void): void;

    // Shows a transient notice at the bottom of the screen.
    showNotice(message: string, durationMs?: number): void;
  };

  // Subscribe to app events.
  on(
    event: 'file-open' | 'file-save' | 'app-ready',
    callback: (payload?: unknown) => void
  ): void;
}
```

---

## Plugin lifecycle

1. On startup Oxidian reads `.oxidian/plugins/` from the vault.
2. For each enabled plugin it reads `manifest.json` then evaluates `main.js`
   via `document::eval` (Dioxus's JS bridge).
3. Plugin code runs synchronously during load, registering commands and
   listeners; async work happens inside callbacks.

```js
// .oxidian/plugins/periodic-notes/main.js
oxidian.ui.registerCommand('daily-note', 'Open daily note', async () => {
  const files = await oxidian.vault.listFiles();
  const today = new Date().toISOString().split('T')[0];
  const path  = `journal/${today}.md`;

  if (!files.some(f => f.path === path)) {
    await oxidian.vault.createFile(path, `# ${today}\n\n`);
  }
  oxidian.vault.openFile(path);
});

oxidian.ui.addSidebarButton('📅', 'Daily note', () => {
  oxidian.ui.registerCommand.run('daily-note');
});
```

---

## Security model

- **No sandbox** — same trust model as Obsidian. Plugins run in the WebView
  context with full access to the `window.oxidian` API.
- **Explicit install** — the user must add the plugin directory to the vault.
  Oxidian never auto-executes plugin code without user action.
- **API boundary** — plugins can only do what `window.oxidian` exposes. They
  cannot make direct vault API calls or access the GitHub token; those go
  through the Rust host which enforces rate limits and auth.
- **Future: permissions manifest** — `manifest.json` will declare which API
  surfaces the plugin uses. Oxidian will show these on install and can refuse
  to load plugins that claim permissions the user didn't grant.

---

## Installation flows

### From the vault (manual)
Drop a `manifest.json` + `main.js` into `.oxidian/plugins/<id>/` and commit.
Oxidian picks them up on next load.

### From a GitHub repo (community plugins)
A future Plugin Manager UI will let the user enter `owner/repo`, fetch
`manifest.json` + `main.js` from the repo's latest release, write them into
the vault, and commit.

### Official plugin registry
A community-maintained index at `oxidian-community/plugins` (a GitHub repo)
lists vetted plugins. The Plugin Manager queries this index and lets the user
browse and install with one click — same model as Obsidian's community plugins.

---

## Implementing the host side (Rust)

The API shim is injected once, before plugins load:

```rust
const OXIDIAN_API_SHIM: &str = r#"
window.oxidian = {
  vault: {
    listFiles: () => new Promise(r => dioxus.send({ cmd:'vault.listFiles', resolve: r })),
    readFile:  p  => new Promise(r => dioxus.send({ cmd:'vault.readFile',  path: p, resolve: r })),
    // …
  },
  ui: {
    registerCommand(id, name, cb) { /* store in registry */ },
    addSidebarButton(icon, title, cb) { /* store; Dioxus re-renders sidebar */ },
    showNotice(msg, ms = 3000) { /* inject a toast DOM node */ },
  },
  on(event, cb) { /* event bus */ },
};
"#;
```

`dioxus.send` delivers a message to the Rust side which handles the command,
then calls back into JS with the result. Dioxus 0.7's `document::eval` already
supports this bidirectional pattern.

---

## Relationship to built-in features

Core features (daily notes, templates, graph view) live in Rust and are **not**
plugins — they're faster, type-safe, and ship with the app. The plugin system
is for community extensibility, not for gutting the core.

The dividing line: if a feature requires tight UI integration, real-time
reactivity, or access to internal state (signals, file SHA tracking), it belongs
in core. If it can be expressed as "read files, write files, open a file, show
a notice" — it's a fine plugin.

---

## What's not a plugin (yet)

| Feature | Why it stays in core |
|---------|----------------------|
| Daily notes | Needs template system + sidebar button |
| Templates | Core data model for content creation |
| Graph view | Tight integration with wikilink index |
| Auto-save | Needs access to internal SHA state |
| Search | Needs access to full file list + GitHub API |
