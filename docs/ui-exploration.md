# UI-exploration

## Necessary Tweaks

### 1. New file in current folder ✅

We always keep track of what the "current folder" is. This means that it is marked in the tree view and if possible kept in view. This also goes for current file. When creating a new note / file, this current folder is to be used as the target folder. So if the user clicks a folder - that is the new current folder. Creating a note puts it in that folder by default, unless the user enters / in the beginning of the note file name.

**Done:** `NewFileModal` now receives `current_dir` (the `selected_dir` signal). The modal shows a hint "Creating inside X/" and a live preview of the resolved path. Names are resolved via `resolve_folder_path` — same logic as `NewFolderModal`: no leading `/` → relative to current folder, leading `/` → vault root. A live "Will create: …" preview line shows the exact path before the user hits Create.

### 2. Interaction Exploration ✅

The directory tree view is a bit clunky and doesn't feel that modern. I would like for you, Claude, to suggest three other types of navigations for the files and folders - and I would prefer if they were selectable at runtime for testing / exploration / customization in the future.

**Done:** A three-button picker (🌲 / ≡ / ⫼) sits at the top of the Files panel. The three variants are:

- **🌲 Tree** (existing): collapsible folder tree with depth-indented nodes.
- **≡ Flat list**: all files shown as a flat, sorted list with folder name headers. A filter box at the top narrows by path substring. Clicking a folder header sets it as the current dir.
- **⫼ Column view** (Miller columns): two-pane layout with a breadcrumb bar at the top. Left column shows the current level; clicking a folder in the left pane opens it in the right column. Clicking a folder in the RIGHT pane drills down — that folder becomes the new left column. The breadcrumb lets you jump back to any ancestor or vault root.

**Nav plugin architecture:** `NAV_PLUGINS` is a static registry of `NavPlugin { id, icon, label }` entries. The picker iterates it. Adding a nav view requires: (1) pushing to `NAV_PLUGINS`, (2) adding one `match` arm in `nav_dispatch`. For runtime-loaded (third-party) plugins the next step is a `NavPluginRegistry` Dioxus Context holding `Vec<Box<dyn NavPluginDyn>>` — documented in code as future work.

### 3. Autocomplete For Paths

When creating a file or folder, if the user enters / in the start OR starts typing the name of a subfolder in the _current folder_ we should have auto-complete for that in the dialog, if possible.

