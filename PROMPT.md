# Build prompt (final spec)

Build a terminal-based file explorer in Rust for macOS.

Core requirements:
- Rust (2021 edition or later), using ratatui for the TUI
- Must run in the macOS terminal (Terminal.app, iTerm2, etc.)
- Navigate with arrow keys: up/down moves the selection, right (or Enter)
  enters a directory, left goes back to the parent
- Display the current directory path, and for each entry show name,
  file vs. directory indicator, size, and modification date
- Handle terminal resize gracefully
- On exit, fully restore the terminal to its original state

Additional features:
- File operations: new file, new directory, rename, delete (with
  confirmation), copy/cut/paste (with name-collision handling)
- Preview pane: text preview of the highlighted file, item count for
  directories, "[binary file]" for non-text
- Search/filter: `/` filters the list live as you type, Esc clears
- Sorting: cycle Name → Size → Modified, toggle ascending/descending
- Open files: Enter on a file opens it with the macOS default app
  (directories first in the listing; hidden files toggleable)
- Built-in text editor: `e` opens the selected text file; arrow keys, Home/End,
  PgUp/PgDn navigate; type to insert, Enter splits lines, Backspace/Delete edit;
  `Ctrl+S` saves (terminals can't see the Cmd key, so no Cmd+S), `Esc` closes and
  asks about unsaved changes; syntax highlighting for Rust, Python, JS/TS, TOML,
  JSON, Markdown, shell (keywords, strings, comments, numbers); refuses binary
  files and files over 512 KB

Code quality:
- Cargo project with modular structure (app state, UI rendering,
  input handling, filesystem operations separated)
- Compiles with zero warnings; include brief docs for public functions
- Unit tests for the filesystem operations

Deliverable: a working binary I can run with `cargo run`
