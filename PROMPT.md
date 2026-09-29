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
  confirmation), copy/cut/paste (with name-collision handling) on `Ctrl+C` /
  `Ctrl+X` / `Ctrl+V` like any OS (`y` / `x` / `p` still work as aliases),
  and `Y` copies the selected entry's absolute path to the system clipboard
  (via `pbcopy`; `Ctrl+Shift+C` also works where the terminal reports it)
- Preview pane (off by default, `P` toggles): extracted text for PDFs, rendered markdown, text preview for
  other files, item count for directories, "[binary file]" for non-text
  (including images — thumbnails were removed as too laggy for a text
  terminal)
- Search/filter: `/` filters the list live as you type, Esc clears
- Sorting: cycle Name → Size → Modified → Type (Type groups folders, then
  files by extension), toggle ascending/descending; default is Modified,
  newest first
- Open files: Enter on a file opens it with the macOS default app
  (hidden files toggleable; folders are no longer pinned to the top except
  under the Type sort, which groups folders first, then files by extension)
- Built-in text editor: `e` opens the selected text file; arrow keys, Home/End,
  PgUp/PgDn navigate; type to insert, Enter splits lines, Backspace/Delete edit;
  `Ctrl+S` saves (terminals can't see the Cmd key, so no Cmd+S), `Esc` closes and
  asks about unsaved changes; `Ctrl+C` / `Ctrl+X` / `Ctrl+V` copy, cut, and paste
  the current line (also synced with the system clipboard via pbcopy/pbpaste);
  syntax highlighting for Rust, Python, JS/TS, Go, C/C++, Java, C#, Ruby,
  HTML, CSS, SQL, YAML, TOML, JSON, Markdown, shell (keywords, strings,
  comments, numbers, types, function calls); refuses binary
  files and files over 512 KB

Code quality:
- Cargo project with modular structure (app state, UI rendering,
  input handling, filesystem operations separated)
- Compiles with zero warnings; include brief docs for public functions
- Unit tests for the filesystem operations

Deliverable: a working binary I can run with `cargo run`

---

## Later additions (kept current with the build)

- **Cross-platform**: macOS (`pbcopy`/`pbpaste`, `open`), Windows (`clip` /
  PowerShell `Get-Clipboard`, `cmd /C start`, hidden-attribute detection),
  Linux (`wl-copy`/`wl-paste`, `xclip`, `xsel`, `xdg-open`). `cargo check
  --target x86_64-pc-windows-gnu` must pass.
- **Miller-column view** (`2`; `1` returns to list view): Finder-style
  columns, ←/→ between columns, preview panel for files when `P` is on.
- **Recursive search** (`f`): filename substring search; `Tab` toggles deep
  content search (case-insensitive, skips binaries/>2MB); `Enter` jumps to
  the hit, `Esc` exits.
- **Preview text copy** (`C`): copies text/Markdown/PDF/CSV preview text to
  the system clipboard.
- **Auto-refresh**: the listing re-reads the directory when its mtime
  changes behind the app (downloads etc.), preserving selection.
- **Instant paste**: bracketed paste inserts the whole block at once in the
  editor and in every input field.
- **Editor upgrades**: `Ctrl+Z` / `Ctrl+Y` (or `Ctrl+Shift+Z`) undo/redo;
  mouse click moves the cursor, click-drag selects text, wheel scrolls;
  `Ctrl+C`/`Ctrl+X` copy/cut the exact selection (never the line-number
  gutter), typing replaces the selection; `Esc` clears the selection first;
  `Ctrl+A` (or `Ctrl+E`, for terminals that grab `Ctrl+A`) selects the whole file; `Ctrl+Left`/`Ctrl+Right` jump by word.
- **CSV viewer** (`e` on a `.csv`): table view with header row and visible
  grid lines; arrows move, `Enter` edits a cell, `Tab` next cell, `a` adds a
  row, `A` adds a column after the current one (popup asks for the name),
  `Ctrl+S` saves, `Esc` closes (asks if unsaved); mouse click selects cells,
  wheel scrolls. CSVs also preview as an aligned table; `C` copies preview
  CSV text.
- **No image thumbnails**: removed — too laggy and useless in a plain text
  terminal. Images get the generic binary preview.
- **Themes**: Dark (purple accent, white text, no solid selection
  backgrounds), Solarized, Dracula, Mono.
  `?` opens help, where `t` cycles the theme and `l` toggles editor line
  numbers. Both persist in `~/.config/fex/settings`.
- **Tabs**: a clickable tab strip. `` ` `` opens a terminal tab (a real
  interactive shell in the current folder via a pty — `$SHELL` on Unix,
  PowerShell on Windows), `Ctrl+T` a new
  file-browser tab, `Ctrl+N` a new blank text document, `Ctrl+W` closes,
  `Ctrl+PgUp/PgDn` switches, `Alt+1-9`
  jumps (`Ctrl+G` then `1-9` is the macOS-friendly equivalent — Mac
  keyboards have no Alt key; `Ctrl+G` then `n`/`p` steps next/previous,
  `Esc` cancels). The file clipboard is shared across tabs. In terminal tabs,
  `Ctrl+C` is SIGINT, not copy; editor copies land on the system clipboard
  (so `Cmd+V` works on macOS — the terminal intercepts `Cmd+C` itself).
  On a blank document, `Ctrl+S` opens a save dialog: browse folders,
  type a name, `Enter` saves (`Enter` again overwrites, `Esc` cancels).
- **Session restore**: quitting saves the tabs under
  `~/.config/fex/session/` and the next launch restores them — browser
  tabs (dir, selection, view, sort, hidden setting), editor tabs (file,
  cursor, unsaved changes via per-tab `backup-N.txt` files), terminal tabs
  (fresh shell in the same folder). Closing a tab (`Ctrl+W`) destroys its
  saved state including unsaved changes; stale backups are cleaned on quit.
- **Favorites**: `*` favorites/unfavorites the selected file or folder
  (starred `★` in list and column views, persisted immediately to
  `~/.config/fex/favorites`); `F` opens the favorites popup — `Enter`
  jumps the current tab there (dirs open, files are revealed in their
  parent), `*` removes, `Esc` closes; missing paths show dimmed with a
  `(missing)` tag.
- **Sorting works in column view**: `s`/`S` re-sort every Miller column, not
  just the list view.
- **Mouse**: click a file/folder to select it, double-click to open it
  (both views — clicking a row in an earlier Miller column jumps there);
  two clicks within 500 ms on the same cell count as a double-click.
- **Reveal in the OS file explorer** (`o`): shows the selected file/folder
  in Finder (`open -R` selects a file, `open` opens a folder), Explorer
  (`/select,` highlights a file), or the default Linux file manager
  (`xdg-open` on the folder — or the parent folder for a file).
- **Drives & network** (`G`): one combined page — local drives/volumes
  with free space (via `sysinfo`, tagged `removable` for USB sticks;
  `Enter` opens one as a regular browser tab), then mDNS/Bonjour
  discovery of SMB servers on the LAN (pure-Rust `mdns-sd`, no system
  services; devices with a mounted share tagged `● mounted`). `Enter` on
  a device lists its shares (`smbutil view` on macOS, `net view` on
  Windows); `Enter` on a share mounts it and opens it as a regular
  browser tab — preview, edit, search, and copy/paste all work on its
  files. macOS mounts with `mount_smbfs` into a temp dir and unmounts on
  tab close (or quit); Windows browses `\\host\share` UNC paths directly;
  Linux mounting is not supported (browse an OS-mounted share as a folder
  instead). Auth-requiring shares prompt for username then password
  (masked, memory-only); `m` adds a host by hand; a failed share listing
  falls back to typing the share name.
- **README.md documents every feature** and is updated with each change.
