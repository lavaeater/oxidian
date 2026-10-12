# Oxidian — Plugin Architecture

A plugin in Oxidian is a **registry entry**, not necessarily a JavaScript file.

That distinction is the whole design. The app's own optional features are
compiled-in Rust; community plugins will eventually be JS loaded from the vault.
Both need exactly the same three things from the app — a place in a list, an
on/off switch, and a settings form — so the *description* of a plugin is data,
and one code path serves both kinds.

§1 is built and in use. §2 is still a design sketch.

---

## 1. Built-in plugins (built)

### The descriptor

`packages/app/src/plugins/` defines what a plugin is:

```rust
pub struct PluginDef {
    pub id: &'static str,           // "bujo"
    pub name: &'static str,
    pub description: &'static str,
    pub settings: &'static [Field], // rendered generically
    pub assets: &'static [Asset],   // scaffolded on enable
}

pub enum Field {
    Text   { key, label, help, default },
    Path   { key, label, help, kind: PathKind, default },
    Bool   { key, label, help, default },
    Select { key, label, help, options, default },
}

pub struct Asset { pub path: &'static str, pub contents: &'static str }
```

The `Field` set is deliberately small. `views::plugins::PluginSettingsForm`
renders any `&[Field]` into a form and knows nothing about any particular
plugin — that is what a future external plugin inherits for free. A plugin whose
configuration doesn't fit these shapes ships its own panel instead; this is the
"trivial to configure" tier and keeping it narrow is what makes it general.

### Layout in the vault

Everything a plugin owns lives in the vault, version-controlled next to the
notes and synced to every device by the same git push:

```
.oxidian/
  plugins.json                 { "bujo": { "enabled": true } }
  plugins/bujo/
    settings.json              { "weekly_template": "…", … }
    templates/daily-log.md
    templates/weekly-log.md
    templates/monthly-log.md
```

Two decisions worth keeping:

- **Settings live in the vault, not `localStorage`.** Plugin configuration
  belongs to *the vault*, not to *this browser*, so it reaches the phone the way
  the notes it configures do.
- **Enablement is a flag, not folder existence.** "Installed but switched off"
  has to be expressible without deleting the user's templates.

Plugin folders sit under `.oxidian/plugins/<id>/` rather than `.oxidian/<id>/`
so a plugin id can never collide with `.oxidian/templates/`.

### Enable / disable semantics

| Action | What happens |
|---|---|
| **Enable** | Write `settings.json` from the declared defaults; write each `Asset` **only where the path is free**. Idempotent — re-enabling never overwrites a template you have edited. |
| **Disable** | Flip the flag in `plugins.json`. Nothing else is touched: settings and templates are the user's files and outlive the plugin. |
| **Re-enable** | Picks up the existing files unchanged. |

Scaffolding relies on `create_file` failing when the destination exists, which
*is* the never-clobber rule rather than an error to report.

### Migrating a setting into a plugin

A feature that predates the registry already has its settings somewhere. Two
hooks handle that without a migration step or a flag day:

- **`seed`** — on *first* enable only, values are pre-filled from wherever they
  used to live. `bujo::seed_from_config` carries over only paths the user
  actually chose; a setting still at its factory default says nothing, and
  letting it win would override the template the plugin is about to install.
- **Fallback** — until a plugin is named in `plugins.json` at all, readers fall
  back to the old config (`bujo::active`, `bujo::template_for`). Once it *is*
  named, the manifest is the only authority, so the toggle actually toggles.

The result: an existing vault keeps working untouched, and enabling the plugin
imports what it had.

### Templates

Plugins keep templates in their own folder, so the template scan
(`views::vault`) looks at `templates_dir` **and** any
`.oxidian/plugins/*/templates/*.md` — otherwise a plugin's own default template
would be installed into the vault and then never found by the resolver that
needs it.

### The line between core and plugin

Not everything optional should be a plugin, and nothing that is a plugin is
therefore slow or second-class — Bullet Journal is compiled-in Rust with full
access to the editor, the index, and the SHA-checked save path, and it is still
a plugin in every way the user can see.

What makes something a plugin here is that **the user can turn it off**. Search,
the file tree, auto-save, and wikilinks are not plugins because a vault without
them is not Oxidian. Bullet Journalling is, because plenty of people want a
notes app and no journal.

---

## 2. External JavaScript plugins (design sketch — not built)

Oxidian runs as WASM in a browser and in a WebView on Android and desktop. You
can't dynamically load compiled Rust at runtime, but **every platform always has
a JavaScript runtime**. Community plugins would therefore be JavaScript files
executed inside the same WebView context that renders the app — the same model
Obsidian uses, and no build toolchain required of the author.

The registry above already carries them: an external plugin is a `PluginDef`
built at *runtime* from a manifest instead of at compile time, appended to
`plugins::builtins()` rather than replacing it. Its settings render through the
same form; its folder, enablement, scaffolding, and disable semantics are
identical. What is missing is only the loader and the host API.

### `manifest.json`

```json
{
  "id": "backlink-graph",
  "name": "Backlink Graph",
  "version": "1.0.0",
  "description": "An alternative graph view.",
  "author": "Jane Smith",
  "minOxidianVersion": "0.2.0",
  "settings": [
    { "kind": "bool", "key": "cluster", "label": "Cluster by folder", "default": true }
  ]
}
```

The `settings` array is the JSON spelling of `&[Field]` — the one piece of the
descriptor that has to cross the language boundary.

### The host API (`window.oxidian`)

Injected once, before any plugin code runs:

```ts
interface OxidianAPI {
  vault: {
    listFiles(): Promise<FileMeta[]>;
    readFile(path: string): Promise<string>;
    writeFile(path: string, content: string): Promise<void>;
    createFile(path: string, content: string): Promise<void>;
    openFile(path: string): void;
  };

  // This plugin's own settings, as configured through the generic form.
  settings: { get(key: string): unknown };

  ui: {
    registerCommand(id: string, name: string, callback: () => void): void;
    addSidebarButton(icon: string, title: string, callback: () => void): void;
    showNotice(message: string, durationMs?: number): void;
  };

  on(event: 'file-open' | 'file-save' | 'app-ready', cb: (payload?: unknown) => void): void;
}
```

`dioxus.send` delivers a message to the Rust side, which handles the command and
calls back into JS with the result — the bidirectional `document::eval` pattern
already used for the editor bridge (`packages/app/assets/oxidian.js`).

### Lifecycle

1. On startup, read `.oxidian/plugins.json` (already done) and, for each entry
   with no built-in of that id, read `.oxidian/plugins/<id>/manifest.json`.
2. Build a runtime `PluginDef` from the manifest so the plugin appears in the
   panel whether or not its code loads.
3. For each **enabled** plugin, evaluate `main.js` via `document::eval`.
4. Plugin code runs synchronously during load, registering commands and
   listeners; async work happens inside callbacks.

### Security model

- **No sandbox** — same trust model as Obsidian. Plugin code runs in the WebView
  with whatever `window.oxidian` exposes.
- **Explicit install and explicit enable** — code is evaluated only for a plugin
  the user switched on. A disabled plugin's `main.js` is never run, which makes
  the toggle a real containment tool and not just a UI state.
- **API boundary** — plugins cannot reach the host token or call the Git API
  directly; those stay behind the Rust host, which enforces auth and rate limits.
- **Future: permissions manifest** — declare which API surfaces the plugin uses,
  show them at install, refuse what the user didn't grant.

### Installation flows

- **From the vault** — drop `manifest.json` + `main.js` into
  `.oxidian/plugins/<id>/` and commit.
- **From a GitHub repo** — a Plugin Manager UI fetches both files from a repo's
  latest release, writes them into the vault, and commits.
- **From a registry** — a community-maintained index (`oxidian-community/plugins`)
  the manager can browse, same model as Obsidian's community plugins.

### What has to be true before this is worth building

The loader is the small part. The hard parts are the host API surface (every
method is a compatibility promise), the permissions story, and the fact that an
un-sandboxed plugin with vault write access can destroy a notebook. None of that
is urgent while the only plugins are the ones shipped in the binary — which is
exactly why §1 was built first and shipped with a real customer.
