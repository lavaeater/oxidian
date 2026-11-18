# UI Polish

The UI is serviceable, but not "perfect". But what is perfection?

Perfection is probably sensible defaults, smooth editing in our case, and then perhaps easy customization.

## Small issues

### Caret Stuck in Checkbox Lists

If the caret is in a checkbox list between the dash "-" and the left square bracket "[" it cannot scroll up or down. This makes flowing through a document slow.

> **Status: fixed on web (option 1, CSS-only); Android still to verify.** Covered by
> `e2e/tests/task-checkbox-ux.spec.js`. Only ArrowDown was actually stuck; ArrowUp worked.
>
> **Analysis** — *Effort: S (½–1 day). Risk: low–medium (needs testing on web + Android).*
>
> The `[ ]` is rendered as `<span class="md-task-checkbox">` styled `display: inline-flex; user-select: none`
> (`packages/ui/src/cm/markdown_area/style.css:254`). That makes it an atomic, non-selectable inline box. The
> position right before it (after `- `) is a boundary where the browser's native ArrowUp/ArrowDown caret movement
> can't compute a horizontal "goal x" and just gives up. On the active line the box is unstyled visually
> (`.md-line--active .md-task-checkbox`, line 334) but it keeps `inline-flex` and `user-select: none`, so the
> problem survives into edit mode.
>
> Fix options, in order of preference:
> 1. CSS-only: on `.md-line--active .md-task-checkbox` set `display: inline; user-select: text;` so that while
>    the line is being edited it's plain text the caret can move through. Probably enough on its own.
> 2. If (1) isn't enough on the Android WebView: handle ArrowUp/ArrowDown in `setup_keyboard`
>    (`packages/ui/assets/markdown_area.js:152`) when the caret is inside a task-line prefix: move to the same
>    column on the previous/next line via `lineTextAndCursor` + `_pendingCursor`, the same way Enter already
>    works in the text model.

### Conserve Vertical space

Toolbars always tak up space. 
In our app we have tabs at the top that indicate which document we are working on. 
Below that we then have the "status" bar with file name / path, a button for the command palette, add bookmark, word count
move to other pane and an export to html button.
And below that we have the properties pane - collapsible, but defaults to open.
Lastly we have the formatting toolbar with some common formatting tasks - however, none of the buttons seem to work.

Solution: I want us to add a top right button that reveals a menu with a checkbox list of what bars to actually show.
Bars should also be placeable at the top OR bottom - the status bar for instance would work better at the bottom by
default. It should also have a menu option called Super Collapse that simply hides all the bars except the top one for 
the tabs and the button itself.

Formatting toolbar: why aren't the buttons working?

> **Status (toolbar): fixed.** Both bugs were confirmed and fixed. The editor now exposes `_mdRange` / `_mdApply`
> (`markdown_area.js`), reached through `js::editor_range` / `js::editor_apply`. The edits themselves are pure
> Rust in `views/toolbar.rs`, with UTF-16↔byte conversion and 13 unit tests. Heading buttons now replace the
> level instead of stacking (`## x` → `# x`). Bullet ↔ task conversion and multi-line selections work. An edit
> that races a keystroke is dropped rather than clobbering it. E2E: `e2e/tests/formatting-toolbar.spec.js`.
>
> **Status (bars): done.** `state::UiPrefs` is persisted under `oxidian_ui`, and `views/layout_menu.rs` is opened
> from a new button in the tab strip. Defaults: status bar at the bottom; properties and formatting at the top;
> properties folded and remembered. Super Collapse hides every bar except the tab strip. On a phone, the formatting
> toolbar is now one horizontally scrolling row instead of two. E2E: `e2e/tests/layout-menu.spec.js`.
> **Still to verify on Android:** where the bottom status bar sits when the soft keyboard is open.
>
> **Analysis: why the formatting toolbar does nothing** (*Effort: S–M, ~1 day. Do this first within this item.*)
>
> There are two separate bugs in `packages/app/src/views/toolbar.rs`:
>
> 1. **The edit never reaches the DOM.** The buttons `prevent_default` on mousedown so the editor keeps focus,
>    then do `content.with_mut(...)`. But `MarkdownArea` deliberately *doesn't* repaint while it's focused
>    (`component.rs:595`, `if !is_focused()`; that's the contenteditable rule in our memory notes). So the
>    signal changes, the screen doesn't, and the next keystroke's `read_state` reads the old DOM text back and
>    **overwrites the toolbar's change**. The button really is a no-op.
> 2. **Offsets are wrong anyway.** `get_selection()` in `oxidian.js:269` adds up raw text-node lengths. It
>    doesn't count the `\n` between `.md-line` divs, and it counts text inside rendered blocks
>    (`[data-md-render]`). The editor's model uses `lineTextAndCursor` / `visibleText` for exactly this reason. So
>    even after a repaint, the prefix/wrap lands one character too early for every line above the caret.
>
> Fix: route toolbar edits through the editor's own "pending edit" path, the same one Enter and Ctrl-Enter use.
> Add an exported `apply_edit(id, fn)`-style helper to `markdown_area.js` that reads `lineTextAndCursor`, computes
> the new text and caret, sets `_pendingText` / `_pendingCursor` and `lineChange`, and dispatches `input`.
> The Rust side then re-renders and restores the caret correctly. `wrap` / `toggle_line_prefix` can stay as pure
> Rust (and get unit tests) if the helper takes `(text, sel_start, sel_end)` → `(new_text, new_cursor)`.
> Alternatively, expose that from `MarkdownArea` as a prop or handle so the toolbar never touches `content` directly.
>
> **Analysis: configurable bars / Super Collapse** (*Effort: M, 2–3 days.*)
>
> Today the stack per pane (`vault.rs` ~2525–2636) is: tab strip → `editor-titlebar` (status) →
> `PropertiesPanel` → `FormattingToolbar` → editor. All four are hard-coded in order. Work involved:
> - A `BarLayout` struct in `state` (per bar: `visible: bool`, `position: Top|Bottom`, plus `super_collapse: bool`),
>   persisted to localStorage like bookmarks. It should be a global setting, not per note or per vault.
> - A top-right "⋯ / layout" button in `tab-strip-actions` with a small popover of checkboxes and top/bottom
>   toggles, plus a "Super Collapse" item. The tab strip only renders when tabs exist (`if !tabs.read().is_empty()`),
>   so the button needs a home when no tabs are open, or the strip should always render when a file is open.
> - Render the pane as `top bars → editor → bottom bars`, filtering by the layout. Mostly moving existing `rsx!`
>   blocks into small components (`StatusBar`, `PropsBar`, `FormatBar`), which also shrinks the 4.8k-line `vault.rs`.
> - Default: status at the bottom, properties collapsed, formatting on. **Mobile caveat:** a bottom bar sits under
>   the soft keyboard on Android, so either pin it above the keyboard (`visualViewport` resize → CSS var) or keep
>   the default bottom bar hidden on mobile while the editor is focused. That's the main unknown in the estimate.
> - Quick win to fold in: `PropertiesPanel`'s `collapsed` is `use_signal(|| false)` (`properties.rs:16`), so it
>   defaults open and resets on every file switch. Make it default collapsed (or auto-collapse when there are
>   no properties) and persist it. That's a 15-minute change.

### Modify Size of Sidebar

I ran into the issue the other day that the sidebar was to "short" to fit the document list in a convenient manner. 
This was because the outline took up a lot of space. One way around this is to use the width of the screen instead, i.e.
making sidebars moveable to the left or right of the writing area. If we have two panes for writing, a bar would go to
the far right or far left of both of the writing areas.

> **Status: quick fix done; full left/right sidebars not started.** The outline title now folds and unfolds the
> outline, and the state is remembered (`UiPrefs::outline_collapsed`). Level-1 outline entries also get their
> indent back. The movable-sidebar feature needs a design decision first (see the summary).
>
> **Analysis** — *Effort: quick fix XS (1–2 h); full left/right sidebars L (4–6 days).*
>
> The outline is rendered inside the Files panel (`vault.rs:1298`, `OutlinePane`) with `max-height: 35vh`
> (`main.css:713`). It always takes up to a third of the sidebar height, even when you're just browsing files.
>
> - **Quick fix (recommended now):** make the outline collapsible (persisted), and/or add a drag handle between
>   the file list and the outline. Optionally move Outline into its own `panel-tabs` entry. Most of the pain goes away.
> - **Full feature:** a second, right-hand `aside` with its own panel tabs, where each panel (Files, Search,
>   Outline, Backlinks, Tasks…) can be assigned to the left or right side. Requires: turning `Panel` into a
>   per-side selection, a persisted `{panel → side}` map, a second resize handle (`start_sidebar_resize`
>   currently drives one `--sidebar-w`), and grid changes so both sidebars sit outside the split editor panes. This
>   is a desktop/iPad feature only. On Android the sidebar is a single drawer, and a right drawer adds little, so
>   the mobile layout can ignore it (or open it as a second drawer from the right edge).

### Jumpy UX

I notice quite often when working with checkbox lists, say if I have written a shopping list, that the UI jumps around 
quite a bit when checking items - on my phone - so that would be in the actual mobile app. So I bring up the shopping list, in this instance I just want to check 
items off as a buy groceries, but every time I check an item the keyboard pops up and I am taken on a whirlwind travel 
across the document. Annoying.

> **Status: fixed on web; Android still to verify.** The checkbox mousedown now calls `preventDefault` (except on
> the line being edited, where the box is raw `[ ]` text). A related bug was found and fixed too: ticking a box
> while editing another line threw the caret to the top of the note. The toggle now repaints directly and
> restores the caret instead of going through `rendered_html`. Covered by `e2e/tests/task-checkbox-ux.spec.js`.
> I didn't add `contenteditable="false"`; `preventDefault` turned out to be enough.
>
> **Analysis** — *Effort: S (½–1 day incl. Android testing). Impact: high on mobile.*
>
> In `setup_tasks` (`packages/ui/assets/markdown_area.js:9`), the nav-link, rendered-action and rendered-block paths
> all call `e.preventDefault()` on mousedown so the caret doesn't move. The **in-source task checkbox path
> (`.md-task-checkbox` with `data-pos`) doesn't.** So tapping a checkbox:
> 1. places the caret in the editor → the editor gains focus → **Android opens the soft keyboard** (which resizes
>    the viewport);
> 2. fires `selectionchange` → that line becomes `md-line--active` → an `input`/linechange re-render shows the line
>    as raw `- [ ] …` source (layout shift);
> 3. `setup_scroll`'s `recenterActiveLine` then smooth-scrolls to re-centre the "active" line in a viewport that the
>    keyboard has just shrunk. That's the whirlwind.
>
> Fix: `preventDefault()` on the checkbox mousedown too (as the action path already does), plus
> `contenteditable="false"` on the rendered checkbox span when the line isn't active, so a touch can't land a caret
> inside it. The toggle then happens without focus, keyboard or recentring. If the editor *was* already focused, keep
> the caret where it was. Worth a unit/E2E check that toggling while focused still writes the right bracket (the
> `rendered_html.set` in `component.rs:739` while focused is borderline against our "don't repaint while focused" rule).

---

## Summary & recommendation

| # | Issue | Root cause found? | Effort | Platform impact | Status |
|---|-------|-------------------|--------|-----------------|--------|
| 1 | Jumpy checkbox toggling (mobile) | Yes: missing `preventDefault` on checkbox mousedown | S | Android (high) | Fixed (web); verify Android |
| 2 | Caret stuck before `[` | Likely: atomic `inline-flex` + `user-select:none` checkbox | S | Web + Android | Fixed (web); verify Android |
| 3 | Formatting toolbar dead | Yes: no repaint while focused + wrong offsets | S–M | All | Fixed |
| 4 | Properties default open | Yes: `collapsed` defaults `false`, not persisted | XS | All | Fixed |
| 5 | Configurable bars / Super Collapse | n/a (feature) | M | All; mobile keyboard caveat | Done; verify Android keyboard |
| 6 | Outline crowding sidebar | Yes: fixed 35vh outline in Files panel | XS quick fix / L full | Web/iPad | Quick fix done; full feature open |

**Start with #1 + #2 together.** They're the same component (task checkbox in `markdown_area.js` / `style.css`),
both are small, both have identified causes, and they hit the core "smooth editing" promise every day, especially
the shopping-list-on-the-phone case. One PR, tested on web and Android.

Then **#3** (toolbar). It's a real bug with a known fix, and it establishes a proper "apply an edit to the editor" API
that #5 and slash commands can reuse. Then **#4 + the outline quick fix (#6)** as a quick-win batch, and finally
**#5** as the one genuine feature project, once the toolbar it rearranges actually works.

**Open decision (#6, full feature):** a full two-sidebar system with any panel on either side is L
(4–6 days, desktop/iPad only). A cheaper option that targets the original pain is M (~1–2 days): an
optional right-hand rail for Outline and Backlinks, so the left sidebar is left to files.
