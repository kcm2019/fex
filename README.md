# fex — a terminal file explorer in Rust

A keyboard-driven file explorer for the macOS terminal, built with
[ratatui](https://ratatui.rs).

## Run it

```sh
cargo run
```

It opens in the directory you run it from. Requires Rust 1.70+.

## Features

- **Arrow-key navigation** — ↑/↓ move, →/Enter enters a directory, ← goes back
- **Preview pane** — text preview of the highlighted file, item count for directories
- **Search/filter** — `/` filters the list live as you type
- **Sorting** — `s` cycles Name → Size → Modified, `S` toggles direction
- **Open files** — Enter on a file opens it in the macOS default app
- **File operations** — new file (`n`), new directory (`N`), rename (`r`),
  delete with confirmation (`d`), copy (`y`) / cut (`x`) / paste (`p`)
- **Built-in text editor** — `e` opens the selected file; arrow keys/Home/End/PgUp/PgDn
  navigate, type to edit, `Ctrl+S` saves, `Esc` closes (asks about unsaved changes).
  Syntax highlighting for Rust, Python, JS/TS, TOML, JSON, Markdown, and shell
  (keywords, strings, comments, numbers). Refuses binary files and files over 512 KB.
- **Hidden files** — `.` toggles dotfiles
- **Help overlay** — `?`

> Note: terminal emulators don't pass the Cmd key through to terminal apps,
> so save is `Ctrl+S`, not `Cmd+S`.

## Layout

```
src/
  main.rs   — terminal setup and event loop
  app.rs    — application state (listing, selection, filter, sort, modes)
  editor.rs — built-in text editor (buffer, cursor, syntax highlighting)
  fs.rs     — filesystem operations (list, preview, create/rename/delete/copy/move)
  input.rs  — keyboard handling per UI mode
  ui.rs     — rendering (header, list, preview, popups)
```

Run `cargo test` for the filesystem unit tests.
