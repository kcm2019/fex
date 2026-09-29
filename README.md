# fex — a terminal file explorer in Rust

A keyboard-driven file explorer that runs on macOS, Linux, and Windows,
built with [ratatui](https://ratatui.rs).

## Install (first time)

1. Install Rust. On macOS the easiest way is Homebrew:
   ```sh
   brew install rust
   ```
   (Any platform: [rustup.rs](https://rustup.rs) works too.)
2. Get the source and build it:
   ```sh
   cd "/Users/km/Documents/Scripts/fex 2"
   cargo build --release
   ```
   This takes a couple of minutes the first time while dependencies compile.

## Run it

From the project folder:

```sh
cd "/Users/km/Documents/Scripts/fex 2"
./target/release/fex
```

It opens in the directory you run it from. To browse somewhere else, `cd`
there first, or pass a path: `./target/release/fex ~/Documents`.

> Coming back later? Just `cd` into the folder and run
> `./target/release/fex` again — no rebuild needed unless the source
> changed. (While developing, `cargo run` builds and runs in one step, and
> `cargo test` runs the unit tests.)

## Views

- **List view** (`1`) — file list; press `P` for the preview pane on the
  right.
- **Miller columns** (`2`) — macOS Finder-style column browsing. ←/→ move
  between columns, ↑/↓ move within a column. With `P` on, a preview panel
  appears on the right when a file is selected.

The listing **auto-refreshes**: if files appear, change, or vanish behind
fex's back (a download landing, another program writing), the view updates
on its own within a fraction of a second. Your selection is preserved.

## Preview pane

The preview pane is **off by default** — press `P` to toggle it. When on,
the highlighted file is previewed automatically:

| File type | Preview |
|---|---|
| Text / code | First lines, as-is |
| PDF | Extracted text |
| Markdown | Styled (headings, emphasis, code, lists, quotes, links) |
| CSV | Aligned column table (first rows) |
| Directories | Item count |
| Anything binary (incl. images) | `[binary file]` placeholder |

Image thumbnails were removed — they were too laggy and don't work in a
plain text terminal.

`C` copies the preview's text (text, Markdown, PDF text, CSV) to the system
clipboard. Placeholders report "Nothing to copy."

## Themes & settings

Press `?` for help, then:

| Keys | Action |
|---|---|
| `t` | Cycle color theme: Dark → Solarized → Dracula → Mono |
| `l` | Toggle editor line numbers |

The theme and the line-number setting are saved to
`~/.config/fex/settings` and restored on the next launch.

The default Dark theme is deliberately plain: white text, purple
directories and highlights. Selections (the active tab, the highlighted
file, the `▸` marker) are shown with bold accent-colored text and never a
solid background, so nothing renders as a black box on terminals whose
own background isn't pure black.

## Tabs

The top strip shows every open tab — click one to switch, or use the
keyboard. Each file-browser tab keeps its own directory, selection, filter,
sort, and editor; the file clipboard (`y` / `x` / `p`) is shared across tabs.
A tab names its folder, or the open file while you're editing it (a new
blank document shows "untitled"; long names are truncated to 24 characters).

| Keys | Action |
|---|---|
| `` ` `` (backtick) | Open a terminal tab — a real interactive shell in the current folder (`$SHELL` on macOS/Linux, PowerShell on Windows) |
| `Ctrl+T` | New file-browser tab |
| `Ctrl+N` | New blank text document in its own tab |
| `Ctrl+W` | Close current tab (the last tab can't be closed) |
| `Ctrl+PgUp` / `Ctrl+PgDn` | Previous / next tab |
| `Alt+1` … `Alt+9` | Jump to tab |
| `Ctrl+G` then `1` … `9` | Jump to tab (macOS-friendly: Mac keyboards have no Alt key, and terminals turn Option+digit into special characters — `Ctrl+G` then `n` / `p` steps to the next / previous tab, `Esc` cancels) |
| Click a tab | Switch to it |

Inside a terminal tab almost every key goes straight to the shell, so
`Ctrl+C` there is SIGINT (interrupt), not copy — the tab-management keys
above still work. The shell is a full pty: colors, prompts, `vim`, `ssh`,
and full-screen programs all work.

### Session restore

Quitting (`q`) saves your tabs, and the next launch brings them back:
file-browser tabs (directory, selection, view mode, sort, hidden-files
setting), editor tabs (file, cursor, and even unsaved changes), and
terminal tabs (a fresh shell in the same folder — the dead process itself
can't be resurrected). Unsaved editor buffers are backed up on quit and
restored as dirty editors, so you pick up exactly where you left off.
Closing a tab with `Ctrl+W` destroys its saved state for good — including
any unsaved changes — and stale backups are cleaned up on every quit.
Session data lives under `~/.config/fex/session/`.

### Favorites

Press `*` to favorite (or unfavorite) the selected file or folder —
favorites get a `★` marker in the file list and Miller columns, and the
list is saved to `~/.config/fex/favorites` immediately. Press `F` to open
the favorites popup: `↑↓` move, `Enter` jumps the current tab there
(directories open; files are revealed in their parent folder), `*` removes
a favorite, `Esc` closes. Missing favorites are shown dimmed with a
`(missing)` tag instead of breaking the list.

## Drives & network

`G` opens one combined page: your local **drives** (volumes, USB sticks,
…) with free space, then the **network** devices — each group under its
own labeled separator line. `Enter` on a drive opens it as a regular
browser tab.

**On Windows**, fex also lists your already-connected **mapped network
drives** (the same `Z:` → `\\server\share` entries File Explorer shows
under This PC) in their own section — no manual host entry needed.
`Enter` opens one directly as a browser tab, since Windows already has
it connected.

Below the drives, fex listens for mDNS/Bonjour announcements (the same
discovery Finder and Windows Network use) and lists SMB file servers on
your LAN with their names and addresses — no root, no setup. Devices
with a share currently mounted are tagged `● mounted`.

- `↑` / `↓` — pick a drive, mapped network drive, or device; `Enter`
  opens the drive or lists the device's shares.
- `Enter` on a share **mounts it and opens it as a regular browser tab**,
  so preview, editing, search, and copy/paste all work on its files.
- The tab is named `//host/share`. Closing the tab unmounts the share
  (macOS); quitting fex unmounts everything.
- If a share needs a login, fex prompts for username then password
  (the password is masked; credentials live only in memory for the
  session). On Windows the OS handles authentication itself.
- `m` adds a host by hand (IP or hostname) for devices that don't
  advertise over mDNS. If listing shares fails, fex asks for the share
  name instead.

How it mounts, per OS:

| | Method |
|---|---|
| macOS | `mount_smbfs` into a temp dir (`smbutil view` lists shares) |
| Windows | none — `\\host\share` UNC paths browse directly (`net view` lists shares) |
| Linux | not supported: mount the share with your OS and browse it as a folder |

## Finding things

- `/` — live filter: narrows the list as you type (Esc clears).
- `f` — recursive search from the current directory:
  - **filename mode** (default): substring match on names.
  - **Tab**: toggles **deep mode**, which searches file *contents*
    (case-insensitive, skips binary files and files over 2 MB).
  - Results show the path (and line number + snippet for content hits).
    `Enter` jumps to the selected result, `Esc` leaves search.

## File operations

| Keys | Action |
|---|---|
| ↑ / ↓, PgUp / PgDn, Home / End | Move selection |
| → / Enter | Enter directory · open file in the default app |
| ← / Backspace | Parent directory |
| `n` / `N` | New file / new directory |
| `r` | Rename |
| `d` | Delete (asks first) |
| `y` / `x` / `p` or `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | Copy / cut / paste files |
| `Y` | Copy the selected file's full path to the system clipboard |
| `o` | Reveal the selected file/folder in the OS file explorer (Finder on macOS, Explorer on Windows, the default file manager on Linux) |
| `s` / `S` | Cycle sort key (Name → Size → Modified → Type) / toggle direction (works in column view too) |
| `.` | Show / hide hidden files |
| `?` | Help overlay |
| `q` / `Esc` | Quit |

Mouse: click a file or folder to select it, double-click to open it (works
in both list and column views — clicking a row in an earlier column jumps
there). Scroll wheel moves the selection.
(Hold `Alt`/`Option` while dragging if your terminal is set to pass mouse
events to the app and you want the terminal's native text selection.)

## Built-in text editor

`e` opens the selected text file (refuses binary files and files over 512 KB).

- **Navigation** — arrows, Home/End, PgUp/PgDn, `Ctrl+Left`/`Ctrl+Right`
  jump by word. Click anywhere to move the cursor; the scroll wheel scrolls.
- **Selection** — hold `Shift` with the arrows (or `Shift+Home`/`Shift+End`,
  `Ctrl+Shift+Left/Right` by word) to select text; releasing `Shift` and
  moving collapses the selection. Click + drag selects with the mouse.
- **Editing** — just type. **Pasting is instant**: the terminal's bracketed
  paste delivers the whole block as one event, so a 10,000-line paste lands
  at once. Even when the terminal delivers a paste character by character
  (e.g. under tmux), pending input is drained before each redraw, so it
  still lands as one update. You can also paste into the rename /
  new-file / filter / search fields the same way.
- **Undo / redo** — `Ctrl+Z` undoes, `Ctrl+Y` (or `Ctrl+Shift+Z`) redoes.
  A paste undoes as **one step** (the whole pasted block, not one
  character); characters typed in quick succession also undo together.
- **Select all** — `Ctrl+A` selects the whole file (`Ctrl+E` does the same,
  for terminals that grab `Ctrl+A` for their own "select all").
- **Copy / cut / paste** — `Ctrl+C` / `Ctrl+X` / `Ctrl+V`, synced with the
  system clipboard:
  - With a mouse selection (click + drag): copies the **exact selected
    text** — the line-number gutter is display-only and never copied.
    Typing or pasting with a selection replaces it. `Esc` clears the
    selection.
  - With no selection: VS Code style — the whole current line is
    copied/cut, and paste inserts below the current line.
- **Save / close** — `Ctrl+S` saves, `Esc` closes (asks about unsaved
  changes first). On a new blank document (`Ctrl+N` tab) `Ctrl+S` opens a
  save dialog instead: browse folders (`↑↓` select, `→` open, `←` up),
  type a file name, `Enter` saves (`Enter` again to overwrite an existing
  file, `Esc` cancels). Syntax highlighting picks up the new extension.
- **Syntax highlighting** — Rust, Python, JS/TS, Go, C/C++, Java, C#, Ruby,
  HTML, CSS, SQL, YAML, TOML, JSON, Markdown, shell. Colored separately:
  keywords, strings, comments, numbers, types (capitalized names,
  decorators, annotations, `#[attributes]`, lifetimes), function calls,
  C-style `#include` directives, CSS `@`-rules, and HTML tags (tag names,
  attribute strings, comments). Block comments carry across lines.

> Terminal emulators don't pass the Cmd key to terminal apps, so save is
> `Ctrl+S`, not `Cmd+S`, and copy/cut/paste use `Ctrl`, not `Cmd`. Editor
> copies are also written to the system clipboard, so `Cmd+V` pastes them
> anywhere on macOS.

## CSV viewer

`e` on a `.csv` file opens a table viewer (the preview pane also shows CSVs
as an aligned table). The first row is treated as the header; quoted fields
with commas and escaped quotes parse correctly. Cells are drawn with visible
grid lines.

| Keys | Action |
|---|---|
| Arrows | Move between cells |
| `Enter` | Edit the current cell (a popup; `Enter` commits, `Esc` cancels) |
| `Tab` / `Shift+Tab` | Next / previous cell |
| `a` | Add an empty row below the current one |
| `A` (`Shift+A`) | Add a column after the current one (a popup asks for the column name; blank becomes `colN`) |
| `Ctrl+S` | Save back to the CSV file |
| `Esc` | Close (asks about unsaved changes first) |
| Mouse | Click a cell to select it; wheel scrolls |

## Clipboard & opening files, per OS

fex shells out to the platform's native tools — no extra setup on macOS or
Windows:

| | Copy/paste | Open with default app |
|---|---|---|
| macOS | `pbcopy` / `pbpaste` | `open` |
| Windows | `clip` / PowerShell `Get-Clipboard` | `cmd /C start` |
| Linux | `wl-copy`/`wl-paste`, then `xclip`, then `xsel` | `xdg-open` |

On Linux, install one of `wl-clipboard`, `xclip`, or `xsel` for clipboard
support (Wayland: `wl-clipboard`; X11: `xclip` or `xsel`). Hidden files on
Windows are detected via the file attribute, not just the dot prefix.

## Layout

```
src/
  main.rs    — terminal setup, event loop, auto-refresh tick, pty polling
  app.rs     — application state (listing, selection, filter, sort, modes)
               + Workspace: the multi-tab strip (browser tabs, terminal tabs,
               shared theme/clipboard/quit)
  editor.rs  — text editor (buffer, undo/redo, selection, highlighting)
  sheet.rs   — CSV table viewer (parse, navigate, edit, save)
  shell.rs   — terminal tab: pty-backed interactive shell (portable-pty +
               vt100), key-to-bytes translation, screen rendering
  fs.rs      — filesystem ops (list, preview, search, create/rename/delete/copy/move)
  input.rs   — keyboard / paste / mouse handling per UI mode
  net.rs     — network: mDNS discovery, SMB share listing, mount/unmount
  ui.rs      — rendering (tab bar, list, columns, preview, editor, sheet, popups)
  theme.rs   — color themes + settings persistence (~/.config/fex/settings)
```

> This README documents every feature. It is updated whenever a feature is
> added or changed.
