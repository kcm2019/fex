# HeckYeah TV — FMHY for Fire TV (tightened spec, draft Oct 26, 2026)

Working title: **HeckYeah TV** · Package: `com.heckyeah.tv` · Source: https://fmhy.net/

## Requested scope (core, locked)

1. Turn FMHY into a TV app: sections as tiles, tvOS-style interface.
2. Tap a section → page scaffolded by its FMHY headers, links as small
   applet tiles with name + description.
3. Tap a link tile → opens in an in-app browser.
4. Ad blocking in the browser.
5. Mouse-cursor navigation by default, toggle to D-pad navigation.
6. Browser chrome styled to look like Safari / iOS.

## Verified FMHY structure (checked live Oct 26, 2026)

- Homepage lists **12 top-level pages**:
  AI (/ai), Streaming — Movies/TV/Anime (/video), Listening (audio),
  Gaming, Reading (/reading), Downloading, Torrenting, Educational,
  Android / iOS, Linux / macOS, Non-English, Miscellaneous.
- Each page is long markdown: `##` = major section, `###` = sub-section,
  `- **Name** - Description / tags / mirror + community links`.
  Example — /video: "Streaming Sites" → Stream Aggregators /
  P-Stream Forks / Dedicated-Server / Multi-Server …
  Example — /ai: "AI Chatbots" → Official Model Sites / Multiple Model
  Sites / Specialized Chatbots / Local AI Frontends …
  Example — /reading: "Ebooks" → Public Domain / PDF Search / Calibre
  Libraries / Ebook Readers …
- Scale: /video alone is ~2,200 lines. All 12 pages together are an
  estimated **8,000–12,000 link entries**. UI must be lazy/shelved and
  searchable — never one giant list.
- One bullet often = 1 main site + 2–5 mirror URLs + Discord/Reddit/
  GitHub links. Those must NOT become separate tiles (see Extra E).

## Data strategy (proposed)

- **Bundled snapshot at build time:** parse the FMHY markdown source
  (GitHub `fmhy/FMHY` repo, the same source fmhy.net is built from)
  into a compact local data file shipped in the APK. Fast, works
  offline, no live scraping of fmhy.net at runtime.
- Parser rule: first link in a bullet = primary tile URL; name =
  bold/first link text; description = rest of the line with community
  links stripped into metadata; tags parsed from description
  (4K, Auto-Next, Sign-Up, No Sign-Up, etc.).
- Refresh without an APK update = Extra D.

## Screens

### 1. Home (tvOS-style)
- Dark gradient/blur backdrop, large "FMHY" wordmark top-left,
  Search + Settings top-right (like tvOS top shelf area).
- Grid of 12 large category tiles: gradient + icon glyph + title +
  FMHY's own one-line description. Focused tile scales up (~1.08×),
  white border + soft shadow + parallax-style lift — the tvOS focus
  effect, done with framework views (no Leanback library on API 25).
- Below the grid (if extras picked): Favorites shelf, Recents shelf.

### 2. Category page (tvOS shelves)
- Scaffolded exactly by FMHY headers: each `###` (grouped under its
  `##`) becomes a **shelf** — header label left-aligned, a horizontal
  row / wrapped grid of small applet tiles under it.
- Small applet tile: site name (bold), description (max 2 lines),
  tag pills underneath. Star/highlight for FMHY-bolded (recommended)
  entries.
- Very long shelves are capped with a "Show all →" tile that opens a
  full grid for that sub-section, so the page stays navigable.

### 3. In-app browser (Safari-style)
- Safari chrome: light frosted toolbar, centered rounded pill
  address field, ‹ › back/forward, reload, tabs-style buttons,
  blue accent #0A84FF (your AirPlay app default). Bottom toolbar
  option mirrors modern iOS Safari.
- WebView with JS + DOM storage on (streaming sites need it).
- Back on remote = browser back, then exit to category page.
- Long-press (mouse mode, hold center ~0.5s) = link menu:
  Open / Open mirror / Favorite / Copy URL.

## Navigation modes (core)

- **Mouse mode (default):** proven Fetch v1.1 pattern — full-window
  cursor overlay, D-pad moves cursor, center = click, hold center =
  long-press, pushing cursor past edge scrolls. Cursor speed in
  Settings (Slow/Normal/Fast).
- **D-pad mode:** native Android focus. Tiles are focusable, focus
  moves spatially, focused tile gets the tvOS scale/glow. Focus is
  remembered per screen.
- **Toggle:** remote Menu key, Settings row, and browser toolbar
  button. Persisted across launches.

## Ad blocking (core)

- Reuse Fetch's proven approach, expanded: `shouldInterceptRequest`
  returns empty response for known ad/tracker hosts (Fetch ships ~50;
  this app ships a larger list, updatable with the data refresh).
- Popup/redirect control: block `onCreateWindow` popups and
  cross-host redirect chains typical of streaming aggregators.
- Honest limit: WebView on API 25 has no extension support — this is
  host-level blocking like Fetch/Hush, **not** uBlock Origin.
  No cosmetic/element hiding, some first-party ads will get through.

## Build constraints (same as Fetch / AirPlay TV)

- Fire TV Stick, Fire OS 6.7.1.1 = Android 7.1.1, **API 25**,
  framework-only (no AndroidX/Leanback), manual aapt2 → javac → d8
  build via `build.sh`, LAUNCHER + LEANBACK_LAUNCHER, 320×180 banner.
- Binaries delivered by hosted link with size + sha256 only.
- Honest limit: the old WebView will not play DRM video and some
  modern sites will refuse it. Tiles open the sites; playback is
  each site's own player doing what the Stick's WebView allows.

## Extras — pick list

- **A. Favorites** — pin any applet tile to a Favorites shelf on Home.
- **B. Global Search** — search all 12 pages by name + description
  from Home; results as one shelf/grid.
- **C. Recents** — last-opened sites shelf on Home + browser history.
- **D. Live refresh** — "Update FMHY data" in Settings fetches the
  latest markdown from GitHub and re-parses on device; no APK update
  needed when sites change (they change often).
- **E. Mirror picker** — detail sheet per tile (OK long-press / info):
  primary + all mirrors + Discord/Reddit/GitHub links, so a dead
  primary isn't a dead end.
- **F. Recommended only** — filter toggle: show only FMHY-bolded
  (starred/recommended) entries per shelf.
- **G. Downloads** — Fetch-style: intercept .apk/.zip/.pdf links,
  download to /sdcard/Download with progress, tap APK to install.
- **H. Hide NSFW** — FMHY has NSFW sub-sections; toggle hides those
  shelves entirely (default: shown, matching FMHY).

Recommended bundle: **Core + A, B, D, E** — favorites, search, live
refresh and mirrors are what keep 10k links usable a month from now.
C, F, G, H are cheap adds if you want them.

## LOCKED PICKS (Oct 26, 2026)

Selected: **Core + recommended extras A, B, D, E** plus a new feature:

### Recently Watched (RW) — Home, first shelf on launch
- On the Home/main menu, the FIRST shelf (when non-empty) is
  "Recently Watched".
- Recording rule: when a site is launched from the Movies/TV
  (video) category — including via Search results sourced from
  video — every page visited in the browser is recorded with
  page title (WebView title), exact URL, source applet name, and
  timestamp. Resuming therefore lands on the exact episode/movie
  page last visited, not the site's homepage.
- Shelf tiles show page title + source site + time; tap reopens
  the browser at that exact URL; long-press removes one entry;
  Settings has "Clear Recently Watched".
- Cap 12 entries, deduped by URL, most-recent first, persisted
  in SharedPreferences. Only video-category launches are recorded
  (not AI/reading/etc.), matching the requested "launched from the
  movies or tv sections" wording.

### Verified data source for D (live refresh)
- Site repo is `github.com/fmhy/edit` (default branch `main`,
  verified Oct 26), content in `docs/*.md`:
  ai.md, audio.md, video.md, gaming.md, reading.md,
  downloading.md, torrenting.md, educational.md, mobile.md
  (Android/iOS), linux-macos.md, non-english.md, misc.md.
- Refresh fetches
  `https://raw.githubusercontent.com/fmhy/edit/main/docs/<file>`
  per category, parses markdown (##/### shelves, `- [Name](url) - desc`
  entries incl. VitePress `**bold**` recommended entries), caches
  to app-private storage, falls back to bundled seed on failure.

## Phone remote (v1.19 — Core + A–E, all picked)

A phone on the same Wi-Fi network controls the browser through a
web page the app itself serves — no phone app to install.

- **Core:** hand-rolled HTTP + WebSocket (RFC 6455) server on port
  18765 (`RemoteServer`), off by default (Settings → "Phone
  remote"), started/stopped with the toggle. Serves
  `assets/remote.html`: trackpad (drag = cursor move with velocity
  acceleration + app-side gain, tap = click, 500 ms hold =
  long-press, two-finger drag = scroll), Back / Forward / Reload /
  Play-Pause / Done buttons, a URL bar (same normalization as the
  TV URL dialog), and live state pushes (title, URL, fullscreen,
  mouse mode, browser-open, history availability) on navigation,
  fullscreen/pseudo transitions, pairing changes, and a 2.5 s
  heartbeat. All commands funnel into the same internals the
  remote control uses (`moveCursor`, `dispatchTap`/
  `dispatchFsTap`, `doScrollBy`, `loadResolved`, `togglePlayJs`,
  `closeBrowser`), so every clamp and freeze applies unchanged.
- **Pairing/security:** 4-digit PIN shown on the TV (regenerated
  at every server start); POST /pair issues an in-memory session
  cookie (max **2** phones — extra E), 5 failed attempts lock
  pairing for 60 s. Tokens never touch storage: restarts re-pair.
  The TV pairing panel (browser menu / browser settings →
  "Phone remote…") shows address, PIN, QR, paired count, and
  "New PIN" / "Disconnect all" controls.
- **A. QR code** on the panel encoding the address (in-app QR
  generator, ECC level M, verified standalone on a desktop JVM).
- **B. mDNS:** the service is advertised as "HeckYeah TV"
  (`_http._tcp.`) while the server runs; the panel also prints a
  `HeckYeah-TV.local` address (name resolution is reliable on
  iPhone; Android phones may need the IP).
- **C. Type into page:** a phone text field inserts text at the
  caret of the page's focused input/textarea/contenteditable
  element (same-origin iframes included) and dispatches `input`;
  an Enter button fires Enter key events at it. If nothing
  editable is focused, the phone shows a note.
- **D. Media keys:** volume up/down (`AudioManager`, needs
  MODIFY_AUDIO_SETTINGS) and ±10 s seek of the biggest playing
  (else first) `<video>` in the page.
- New permissions (both normal): ACCESS_WIFI_STATE (display the
  device's Wi-Fi address in the panel) and MODIFY_AUDIO_SETTINGS.

## Open choices

1. App name: HeckYeah TV (proposed) / FMHY TV / FreeMedia TV?
2. Home layout: 12 large tiles in one grid (proposed) or a tvOS
   top-shelf hero + smaller grid?
3. Safari toolbar: top (classic) or bottom (modern iOS)?
