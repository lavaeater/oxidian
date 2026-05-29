Here is an MVP (Minimum Viable Product) specification for your Git-native Markdown knowledge management application.

Since the Git/GitHub synchronization is the underlying infrastructure, this specification focuses entirely on the user-facing features you requested. The requirements are structured into **Epics** (high-level feature sets) and broken down into actionable **User Stories** following the standard agile format: *As a [user], I want [action] so that [benefit]*.

---

### **Epic 1: Bookmarks**

*Features for saving and quickly accessing frequent notes.*

* **US 1.1:** As a user, I want to bookmark the currently active note or specific headings so that I can easily find them later.
* **US 1.2:** As a user, I want a dedicated Bookmarks sidebar pane so that I can see and reorganize my saved links in one place.

### **Epic 2: Command Palette**

*Keyboard-first execution of app commands.*

* **US 2.1:** As a user, I want to trigger a command palette via a keyboard shortcut (e.g., Cmd/Ctrl+P) so that I can execute actions without using a mouse.
* **US 2.2:** As a user, I want the command palette to support fuzzy searching so that I can find commands quickly even with typos or partial names.

### **Epic 3: Periodic Notes (Natural Language & Templates)**

*Time-based journaling and logging.*

* **US 3.1:** As a user, I want to create Daily, Weekly, and Monthly notes with a single click or command so that I can maintain a friction-free journal.
* **US 3.2:** As a user, I want periodic notes to automatically apply a predefined template upon creation so that my formatting is consistent.
* **US 3.3:** As a user, I want to use natural language dates (e.g., typing `@next friday` or `@today`) which automatically convert into standard date-formatted links (e.g., `[[2026-05-29]]`) so that I don't have to manually calculate dates.

### **Epic 4: Graph View**

*Visualizing relationships between files.*

* **US 4.1:** As a user, I want to see a global, interactive network graph of all my notes and their `[[wiki-links]]` so that I can visualize my knowledge base.
* **US 4.2:** As a user, I want a "Local Graph" for the active note that only shows directly connected notes (1 or 2 steps away) so that I can see the immediate context of what I am working on.

### **Epic 5: Note Composer**

*Tools for refactoring and reorganizing text.*

* **US 5.1:** As a user, I want to select a block of text and extract it into a brand new note (leaving a link behind) so that I can break large notes down easily.
* **US 5.2:** As a user, I want to merge the currently active note into an existing note so that I can consolidate duplicated information.

### **Epic 6: Outline / Table of Contents**

*Navigating within a single large document.*

* **US 6.1:** As a user, I want a sidebar pane that automatically generates a nested Outline based on Markdown headings (`#`, `##`, etc.) in the active note.
* **US 6.2:** As a user, I want to click any heading in the Outline pane to instantly scroll to that section of the document.

### **Epic 7: Page Preview (Hover)**

*Context without context-switching.*

* **US 7.1:** As a user, I want to hover my mouse over an internal `[[link]]` and see a popover preview of that note's contents so that I don't have to navigate away from my current work.

### **Epic 8: Properties (Metadata) View**

*Managing YAML frontmatter cleanly.*

* **US 8.1:** As a user, I want a visual UI at the top of my notes to view and edit YAML frontmatter properties (like text, lists, and dates) so that I don't have to manually type YAML syntax.
* **US 8.2:** As a user, I want to hide or collapse the properties view so it doesn't distract me while I am writing.

### **Epic 9: Publish & Export**

*Sharing the vault with others.*

* **US 9.1:** As a user, I want to export an individual note as a standalone HTML file so that I can share it outside the app.
* **US 9.2:** As a user, I want to compile a selected folder (or the whole vault) into a static HTML site/documentation format so that I can publish my knowledge base to the web.

### **Epic 10: Quick Switcher**

*Rapid file navigation.*

* **US 10.1:** As a user, I want to open a Quick Switcher modal via keyboard shortcut (e.g., Cmd/Ctrl+O) so that I can jump between files quickly.
* **US 10.2:** As a user, I want the Quick Switcher to fuzzy-search file names and prioritize recently opened files.

### **Epic 11: Search**

*Finding information across the vault.*

* **US 11.1:** As a user, I want a global search pane that performs full-text search across all markdown files in my vault.
* **US 11.2:** As a user, I want to filter my search results using parameters (e.g., `path:`, `tag:`, or `file:`) to narrow down results in large repositories.

### **Epic 12: Slash Commands**

*In-line formatting without markdown knowledge.*

* **US 12.1:** As a user, I want to type `/` in the editor to open a contextual menu so that I can quickly insert elements like tables, callouts, or templates without knowing the exact markdown syntax.

### **Epic 13: Slides**

*Presentations derived directly from text.*

* **US 13.1:** As a user, I want to define slide transitions in my markdown using horizontal rules (`---`).
* **US 13.2:** As a user, I want a "Presentation Mode" button that renders these segmented notes as a full-screen slideshow.

### **Epic 14: Tags View**

*Taxonomy and categorization.*

* **US 14.1:** As a user, I want a dedicated pane that lists all `#tags` used across my vault, displaying nested tags (e.g., `#work/projectA`) in a collapsible tree structure.
* **US 14.2:** As a user, I want to click a tag in the Tags pane to instantly populate the global search with that tag.

### **Epic 15: Templates**

*Reusability for common note structures.*

* **US 15.1:** As a user, I want to define a specific folder in settings where my template files live.
* **US 15.2:** As a user, I want to insert a template into my active note (via command palette or shortcut) so that I don't have to rewrite boilerplate text.

### **Epic 16: Word Counting**

*Writing metrics.*

* **US 16.1:** As a user, I want to see the current word and character count of the active note in the bottom status bar.
* **US 16.2:** As a user, I want the word count to dynamically update to show only the count of the currently highlighted text if I make a selection.

### **Epic 17: Editing Toolbar**

*Rich-text-like UI support.*

* **US 17.1:** As a user, I want an optional toolbar above the editor (or on text selection) with buttons for Bold, Italic, Strikethrough, Headers, and Lists so that I can format text using a mouse.

### **Epic 18: Dataview (Query Engine)**

*Treating the vault as a database.*

* **US 18.1:** As a user, I want to write a SQL-like query inside a specific code block (e.g., ```dataview) to dynamically list or filter notes based on their tags, folders, or YAML properties.
* **US 18.2:** As a user, I want to be able to render the results of my query as a List, Table, or Task list directly inside my markdown preview.

### **Epic 19: Kanban Board**

*Visual task management.*

* **US 19.1:** As a user, I want to convert a standard Markdown document containing lists (e.g., `# To Do`, `# Doing`, `# Done`) into a visual Kanban board view.
* **US 19.2:** As a user, I want to drag and drop tasks (markdown list items) between columns in the visual view, and have the underlying markdown file update automatically to reflect the move.
* **US 19.3:** As a user, I want the different "lanes" of the KanBan board to be represented by folders in the Vault. So, ToDo is a folder, Doing is a folder, etc. The board is basically a visualisation of what is in each folder + ordering them.