# HeckYeah TV

HeckYeah TV is a tvOS-style Fire TV app that turns [FMHY](https://fmhy.net/)
(Free Media Heck Yeah) into a remote-friendly interface: the catalog's
categories become large tiles, each category page becomes shelves of site
tiles based on FMHY's own section headers, and links open in a Safari-styled
in-app browser built for D-pad and mouse-style remote navigation.

## Features

- 12 FMHY categories as large Home tiles; category pages scaffolded by
  FMHY `##`/`###` headers into shelves of applet tiles
- Live catalog refresh from the `fmhy/edit` GitHub repository, with a
  bundled seed catalog as an offline fallback
- Favorites, global search, and a mirror picker for entries with
  multiple links
- Recently Watched shelf for video-category launches, with poster art
  and rating enrichment where available
- Safari-style in-app browser with mouse and D-pad sub-modes, an
  auto-hiding toolbar, and HTML5 fullscreen support (including handling
  for in-page "pseudo-fullscreen" players)
- Host-level ad/tracker and popup blocking, video-aware scroll
  clamping, and fullscreen diagnostics under the `HeckYeahFS` log tag
- Phone remote: a web page served by the app lets a phone on the
  same Wi-Fi control the whole app — an Apple TV Remote-style
  page with a trackpad for the browser and a clickpad D-pad for
  the menus — paired with a PIN shown on the TV — see
  "Phone remote" below

## Requirements

- Android device, API 25 or later (developed and tested on Fire TV
  with Fire OS 6 / Android 7.1.1)
- To build: JDK 17, Android SDK build-tools 34.0.0 and the android-34
  platform (`android.jar`), plus `bash`, `zip`, and `keytool`

## Build

Signing passwords are provided through the environment; they are never
stored in the repository. If `keystore.jks` does not exist, the build
generates a local development keystore (using a clearly warned default
password when no environment password is set).

```bash
export KEYSTORE_PASS='your-keystore-password'
export KEY_PASS="$KEYSTORE_PASS"   # optional; defaults to KEYSTORE_PASS
./build.sh
```

The signed APK is written to `build/manual/apk/HeckYeah.apk`.

## Install (adb)

```bash
adb connect <device-ip>:5555
adb install -r build/manual/apk/HeckYeah.apk
adb shell monkey -p com.heckyeah.tv -c android.intent.category.LAUNCHER 1
```

## Phone remote

With **Settings → Phone remote** turned on, the app serves a
control page on your local network at `http://<device-ip>:18765`.
Open it in a phone browser (no app to install), enter the 4-digit
PIN shown on the TV in the pairing panel (opened from **Home →
Phone**, the browser toolbar's **Phone** button, or the browser
menu — and, since v1.27, it also pops up by itself for 10
seconds when the browser opens with no phone paired), and the
phone becomes a full remote for the app. The page
is styled after Apple's TV Remote app and switches modes
automatically: in the browser it is a trackpad (cursor movement,
taps and long-presses, scrolling, Back / Forward / Reload /
Play-Pause / Done, URL entry with the phone keyboard, typing
into the focused field on the page); everywhere else the same
surface becomes a clickpad D-pad that drives the TV interface
with synthesized remote keys — menus, Home, Search, Settings
and dialogs included. A segmented control on the page can
override the mode until the TV's screen next changes. ±10-second
seek and volume work in both modes. The TV panel also shows a
QR code for the address and the paired-phone count (up to 2),
and can regenerate the PIN or disconnect all phones.

Permissions used by this feature (both normal Android
permissions): `ACCESS_WIFI_STATE` (to display the device's Wi-Fi
address in the pairing panel) and `MODIFY_AUDIO_SETTINGS` (the
phone's volume buttons).

**Security:** the remote is **off by default**. It is a
LAN convenience, not an internet service: pairing requires the
PIN shown on the TV, the PIN is regenerated at every server
start, session tokens live in memory only (a restart means
re-pairing), and 5 failed pairing attempts lock pairing for 60
seconds. Anyone who can see the TV screen and reach the device
on your network can pair — turn it off on networks you do not
trust.

## Limits

- Ad blocking is host-level only: known ad/tracker hosts are refused,
  but the API 25 WebView has no extension support or cosmetic
  filtering. This is not uBlock Origin.
- The old WebView cannot play DRM-protected video, and some modern
  sites may refuse or misrender in it.
- Video players embedded in cross-origin iframes cannot be inspected
  by page scripts, so some browser safeguards (such as video-aware
  scroll clamping) may not engage for those players.

## License

MIT — see [LICENSE](LICENSE).

---

# HeckYeah TV — FMHY for Fire TV

A tvOS-style Fire TV app that turns [fmhy.net](https://fmhy.net/)
(freemediaheckyeah) into tile navigation: 12 categories as large
tiles, category pages scaffolded by FMHY's own `##`/`###` headers
into shelves of small applet tiles, and an in-app Safari-styled
browser to open them. Built Oct 26, 2026.

## What it does (Core + A, B, D, E + RW)

- **Home (tvOS-style)** — dark backdrop, "HeckYeah TV" wordmark,
  Search / Settings / nav-mode buttons top-right. 12 large category
  tiles (gradient + glyph + FMHY description + site count); focused
  tile scales up with a white border in D-pad mode.
- **Recently Watched (RW) — first shelf on Home** when non-empty.
  When a site is launched from the Movies/TV (video) category —
  including via Search results sourced from video — one entry per
  launch session tracks the **latest content page**: URL + WebView
  title are maintained via `doUpdateVisitedHistory` (fires for
  SPA in-page navigation too) + `onPageFinished` +
  `onReceivedTitle`, snapshotted once more from
  `WebView.getUrl()/getTitle()` when Done is hit. A site-root URL
  never overwrites a deeper entry for the same host in the same
  session, so the tile quick-links back to the exact episode/movie
  page, not the homepage. On Done, the playing `<video>`'s
  `currentTime` is also captured (best-effort JS) and a reopened
  RW tile tries to seek back to it after load. Cap 12,
  most-recent first, persisted in SharedPreferences. Tap reopens
  the stored exact URL; long-press removes one entry;
  Settings → "Clear Recently Watched" wipes it. Only
  video-category launches are recorded.
- **Category pages** — scaffolded exactly by FMHY headers: each
  shelf header with its applet tiles (name, 2-line description,
  ★ for FMHY-bolded/recommended entries). Shelves over 9 tiles get
  a "Show all →" tile that opens the full shelf grid.
- **A. Favorites** — long-press any tile (or a browser link) to pin
  it; Favorites shelf appears on Home. Long-press detail sheet can
  remove it; Settings can clear all.
- **B. Global Search** — searches name + description + shelf across
  all categories (first 60 hits), results as applet tiles.
- **D. Live Refresh** — Settings → "Update FMHY Data" fetches the
  12 category files from `github.com/fmhy/edit` (`main` branch,
  `docs/*.md`) via
  `https://raw.githubusercontent.com/fmhy/edit/main/docs/<file>`,
  parses the markdown on-device (`##`/`###` shelves, first link in
  a bullet = primary tile, later links = mirrors, `**bold**` =
  recommended), caches to app-private storage, and reloads. Falls
  back to the bundled seed on failure (needs ≥6/12 files and a
  sane entry count before the cache is replaced).
- **E. Mirror Picker** — long-press a tile for its detail sheet:
  primary URL, every mirror URL, Add/Remove Favorite, Copy URL.
- **Safari-style browser** — toolbar with pill buttons, **Dark
  appearance by default** (v1.3, persisted Dark/Light setting in
  the gear sheet + Settings; Dark = `#1C1C1E` toolbar / `#2C2C2E`
  pills / white text, Light = the v1.1 `#F2F2F7` / white-pill look),
  centered pill **URL button** (it is not a text field: clicking it
  opens a URL editor dialog), ‹ › back/forward, reload, mouse/D-pad
  sub-mode toggle, **⚙ gear** (Browser Settings sheet: scale,
  ad blocking, JavaScript, desktop UA, cursor speed, clear
  cache/history), "✕ Done", page title bar underneath, thin
  progress bar, accent `#0A84FF`. JS + DOM storage on, zoom on,
  HTML5 fullscreen video supported. Back = exit fullscreen, then
  browser history back, then Done behavior.
- **Ad blocking + popup blocking** — `shouldInterceptRequest`
  returns an empty 204 for ~100 known ad/tracker/popup-network
  hosts (Fetch's ~50-host list expanded with pop/push/redirect
  networks); `onCreateWindow` popups are refused and multiple
  windows / JS window-open are disabled in WebSettings. Toggle in
  Settings.
- **Navigation modes (v1.1 rework)** — **D-pad/native focus
  everywhere outside the browser.** The mouse cursor exists ONLY
  inside the browser (Fetch-style overlay: D-pad moves, center =
  click, hold center ≈0.5 s = long-press, edge-push scrolls the
  page; Shift = fine movement; cursor speed in Settings/gear).
  The browser keeps two sub-modes — mouse (default) and D-pad —
  toggled by the remote **Menu key** (in the browser), the
  Settings row, or the browser toolbar button; persisted. In
  browser D-pad mode, arrows scroll the page, ▲ at the very top
  moves focus into the toolbar (‹ ›, URL pill, reload, mode, ⚙,
  Done), ▼ returns to the page, and OK taps the page / activates
  the focused button. **All D-pad and media keys are intercepted
  in `dispatchKeyEvent` while the browser is showing and never
  reach the web page**, so site video players can't mistake
  Left/Right for seek or Up/Down for volume.

## Bundled seed catalog

`src/main/assets/catalog.txt`, pipe-delimited:
`categoryId|shelf|name|url|description|recommended|mirrors`
(mirrors comma-separated). **193 entries**: video 29, ai 28,
reading 23, gaming 19, audio 18, linux-macos 16, downloading 15,
educational 15, torrenting 14, misc 14, mobile 1, non-english 1.
Video/AI/reading entries are taken from FMHY pages as rendered on
Oct 26, 2026. The mobile and non-english seed tiles open that
category's fmhy.net page and note that Settings → Update loads the
full list. Live Refresh (D) populates the full ~8–12k-entry
catalog; the seed is a starter + offline fallback.

## Target device / build

- Fire TV Stick, Fire OS 6.7.1.1 = Android 7.1.1, **API 25**:
  `minSdkVersion 25`, `targetSdkVersion 25`, framework-only Java
  (no AndroidX/Leanback), single `MainActivity`.
- LAUNCHER + LEANBACK_LAUNCHER, generated 320×180 TV banner +
  launcher icons (blue/violet gradient, white play glyph).
- Package `com.heckyeah.tv`, version 1.18 (versionCode 19).
  `android:hardwareAccelerated="true"` on app + activity (v1.1).
- Permissions: `INTERNET`, `ACCESS_NETWORK_STATE` only.

Manual SDK-tools build (Gradle daemon can't run in the sandbox):
aapt2 → javac (source/target 8, android.jar on `-cp`, NOT
`-bootclasspath`) → d8 `--min-api 25` → zipalign → apksigner.
Keystore: project `keystore.jks` (not tracked). Signing
passwords are supplied through `KEYSTORE_PASS` / `KEY_PASS`;
if the keystore is absent, `build.sh` generates a local
development keystore.

```bash
export KEYSTORE_PASS='change-me'
export KEY_PASS="$KEYSTORE_PASS"
./build.sh   # → build/manual/apk/HeckYeah.apk
```

## v1.32 (Oct 29, 2026) — remote text strip + fresh-page URL, 15s pairing card, self-healing cursor

Three parts from Kurt: the remote-page texts he asked to remove
were still on his phone, the pairing card's countdown goes from
10s to 15s, and toggling the browser input mode from the nav-bar
Mode button while a phone was connected killed the cursor until
an app restart.

- **(Part A) The remote text: really gone, and the phone is
  forced to fetch the fresh page.** v1.31 verifiably deleted
  `#modeCap`/`#footNote` and the server already sent
  `Cache-Control: no-store` — yet the texts persisted on Kurt's
  phone, because the pairing URL was identical for every build:
  a phone tab restored from its snapshot/cache at the old address
  can keep running the pre-update document while its WebSocket
  reconnects and makes the stale page look alive. Two fixes.
  (1) **Versioned pairing address:** the address shown in the
  pairing panel and the pairing card, and the URL encoded in the
  QR, is now `http://<ip>:18765/?v=<versionCode>` (the .local
  alt address in the panel carries the same suffix), built by one
  `remotePageUrl()` helper from the live package versionCode.
  `RemoteServer` already strips the query string when routing,
  and the page's WebSocket connects via `location.host`, so the
  versioned URL routes and pairs exactly as before; the
  `Cache-Control` header gains `must-revalidate` alongside
  `no-store`. (2) **The last non-functional texts are removed:**
  `#padHint` inside the trackpad surface ("Drag to move the
  cursor…", and its menu variant "Swipe to move · tap to
  select…") — element, CSS (`#padHint`, `#padHint.fade`), the
  `fadeHint()`/`hintFaded` machinery and both call sites, and the
  `applyMode()` writer — plus the two `.skipCap` divs ("Skip
  back" / "Skip forward") and their CSS rule; the skip buttons
  keep their `aria-label`s and `−10`/`+10` spans. The surfaces
  are now completely text-free. Fit is structurally unchanged:
  `#padHint` was absolutely positioned (no flex impact) and the
  media row's height is set by the 120dp volume rocker, not the
  skip captions, so the 100dvh budget is identical in both
  modes, panels closed or open. Audit: zero live references to
  any removed node, every `getElementById` target present in the
  DOM, extracted script passes `node --check`. Kept as functional
  labels: header brand + now-playing, the Trackpad/D-Pad
  segmented labels, Back/Forward/Reload pill text, URL/Type chip
  text, OK.
- **(Part B) Pairing card countdown 10s → 15s.** One constant,
  `PAIR_CARD_MS = 15000`: `startPairCardCountdown()` derives both
  the "Closes in Ns" counter start (`PAIR_CARD_MS / 1000`) and
  the progress-drain animator's duration from it, and that method
  is the single funnel for the show path and every restart
  (including the turn-on inside the card via `pairCardAction()`),
  so all paths count from 15. The manual pairing panel has no
  countdown and gains none. The unrelated 10s fullscreen cursor
  fade is untouched.
- **(Part C) Self-healing cursor.** Analysis of the old code:
  `updateCursorVisibility()` — the only show/hide gate — set
  visibility but never alpha; the fullscreen idle fade is the
  cursor's only alpha writer, and both of its restore paths
  (`pokeFsCursor`, `cancelFsFade`) are conditional on the
  `cursorFaded` flag, so any divergence between that flag and the
  view's real alpha (an animator completing or being cancelled
  across a mode/dialog/pairing transition) left a VISIBLE view at
  alpha 0 with no path back — `moveCursor` only calls
  `pokeFsCursor`, which no-ops when the flag is false.
  `setBrowserMouse(true)` — the exact transition in Kurt's
  report — neither cancelled a fade animation nor restored alpha.
  And the phone-connected transitions (`remoteSessionsChanged`,
  dialog show/dismiss, pairing card show/hide) never re-asserted
  the cursor at all. Fix: one `refreshCursor(why)` recomputes
  visibility from (browser current && mouse mode); when the
  cursor should show it forces VISIBLE, cancels any stray alpha
  animation and restores alpha 1 — unless the fsLike idle fade
  legitimately owns the alpha (`fsLikeActive() && cursorFaded`) —
  re-asserts decor z-order (cursor above content/fsContainer,
  feedback flash back on top), and invalidates; when it should
  hide, GONE. It is called from `setBrowserMouse` (both
  directions), `showScreen`, `remoteSessionsChanged`, every
  `trackDialog` show/dismiss transition, pairing card show/hide,
  and `showFullscreen`/`hideFullscreen` (the four sites that
  previously called `updateCursorVisibility`, which now delegates
  to it). A watchdog at the top of `moveCursor()` — the funnel
  for D-pad movement and the phone's `remoteMove` — heals on the
  spot if the cursor should be visible but is GONE or
  alpha-faded outside a legitimate fade. HeckYeahFS logs only on
  change or rescue: `cursor show/hide/rescue why=<reason>`.

## v1.31 (Oct 29, 2026) — remote text cleanup + layered play/pause

Two parts from Kurt: the phone remote still carried explanatory
text he didn't want, and "Play and Pause on a regular remote and
the website remote still does not work."

- **(Part A) Remote page text removals (remote.html).** The mode
  caption element (`#modeCap` — "TRACKPAD — BROWSER",
  "TRACKPAD — MENU · SWIPE TO NAVIGATE", "D-PAD — MENU") and the
  bottom footnote element (`#footNote` — in D-pad mode: "Flick,
  tap, or hold a direction on the clickpad to navigate the TV…")
  are **deleted outright**: DOM nodes, CSS rules (including the
  short-screen footnote rule) and every JS writer
  (`applyMode()` no longer touches either). The freed rows
  collapse through the flex layout, so the surface absorbs the
  space and the 100dvh fit math is unchanged in both modes. The
  in-surface trackpad hint (`#padHint`) stays. An audit confirms
  zero remaining `modeCap`/`footNote` references and zero
  `getElementById` calls pointing at missing nodes, so mode
  switches both ways are error-free by construction; the page
  script still passes `node --check`.
- **(Part B) Play/pause rebuilt as ONE layered path.** Both
  entry points — the physical remote's `MEDIA_PLAY_PAUSE` in
  browser mouse **and** D-pad sub-modes, and the phone remote's
  playpause command — now call the same `togglePlayPause(src)`.
  Diagnosis of the old path: the toggle JS itself was already
  ES5 (the audit found no arrow functions / let / const /
  template literals / optional chaining; the guarded Promise
  `.catch` access is ES5-valid and non-load-bearing), so syntax
  wasn't the killer — the flow was: anything fullscreen-like
  (including pseudo-FS) short-circuited straight to the injected
  media key, which a WebView largely ignores for cross-origin
  embeds like atlantic.st's player; the mouse-mode key handler
  had **no repeat guard**, so a held key could toggle twice and
  look like a no-op; and D-pad **toolbar focus swallowed
  `MEDIA_PLAY_PAUSE` entirely**. The new layers, in order:
  **L1** — ES5 JS toggle (top document + recursive same-origin
  iframes; pause the largest playing / play the largest paused;
  structured `none:seen=N,xframes=M` when nothing is reachable).
  **L2** — a JS-dispatched `click()` on that same largest video,
  for players that only honor their click handler. **L3** — a
  **real injected tap** at the video's center through the
  existing Instrumentation path (a genuine touch reaches a
  cross-origin player's own handlers, like tapping with the
  cursor): the point is the fresh (<2s) poll rect's center —
  the poll now also stores the video's CSS *viewport* rect, and
  the conversion is `center × webView.getScale()` + the
  WebView's window offset, the scroll-lock conversion — or the
  WebView center while fullscreen-like; it **never** taps after
  L1/L2 landed, and never taps blindly with no known target.
  **L4** — the existing media-key injection fallback. Real
  fullscreen keeps its existing behavior (the L4 key lands in
  `handleFullscreenKey` as the center tap). Anti-double-fire:
  callers ignore `repeatCount > 0` (new in mouse mode) and the
  shared function has a ~300ms re-entry guard. Logging under
  `HeckYeahFS` at every step: `playpause src=key|phone`, each
  layer's result verbatim (`playpause L1 js result=…`,
  `L2 click result=…`, `L3 tap result=tap x=… y=…` /
  `skip:no-target`, `L4 mediakey result=inject`), exactly one
  final `playpause via=js|click|tap|mediakey|none` (the tap
  line carries its coordinates), and a ~450ms post-action probe
  (`playpause post src=… via=… state=playing|paused|none`) from
  a dedicated ES5 state script that — unlike the playing-only
  poll — can name the paused state. The v1.26 center feedback
  flash keeps its rule on the new path: the JS layer flashes
  its known result immediately; click/tap/mediakey flash only
  when the post probe provably names the new state (silent when
  indeterminable, and in real fullscreen). All page JS in this
  path is strict ES5 and was extracted from the built source
  and passed `node --check`. Seek/volume/nav paths untouched.
- Manifest versionCode **32** / versionName **1.31**;
  `alive version=1.31 code=32`.

Build verification (build-verified only; the removals and the
layered toggle — especially L3 against atlantic.st's player —
are device-test-only):

- `aapt2 dump badging`: `com.heckyeah.tv` versionCode
  **32** / versionName **1.31**, sdk 25 / target 25.
- APK: 611,560 bytes, sha256
  `ce74bb0d5580abbd2e5f552c61e6ef65c4cac2c6ce0ba01acaa00fb6cc9eaabf`.
- `apksigner verify`: OK; signer certificate SHA-256
  `220529f9…b92fab5` (unchanged signing key).
- `unzip -t`: no errors; the APK's `assets/remote.html` is
  byte-identical to the source file.
- Remote page script extracted and passed `node --check`;
  the three play/pause page scripts (L1/L2/state probe)
  extracted and passed `node --check` with no non-ES5 tokens.

## v1.30 (Oct 29, 2026) — centered nav icons + arrow tips verified in a real render

Two fixes from Kurt's on-device look at v1.29: "icons are shifted
left of where they should be, please center them" and (caps his)
"FIX ARROW TIPS FOR RELOAD AND FAST FORWARD AND REVERSE".

- **Centering — root cause, not a nudge.** Every toolbar icon
  rode on its control as a LEFT compound drawable of a `Button`
  with empty text. `TextView.onDraw` draws that drawable at
  `x = mPaddingLeft + getHorizontalOffsetForDrawables()` — and
  the offset is 0 for Button/TextView — so the 30dp glyph was
  pinned to the left edge of every slot: 9dp left of center in
  the 48dp segments, 7dp in Done's 44dp, 5dp in Reload's 40dp
  (`Gravity.CENTER` only centers the zero-width text). The
  controls are now `FrameLayout`s holding one `Gravity.CENTER`
  `ImageView` child — the glyph canvas center coincides with the
  slot center exactly (0.0dp) for all 8 stops, verified by a
  layout-true composite render. Second defect in the same
  report: the v1.29 bar centered the address capsule with two
  equal-weight spacers, but the flanks are unequal (left cluster
  109dp, right cluster + Done 216dp), parking the capsule ~53dp
  left of screen center at 1080p. The bar is now a `FrameLayout`:
  nav cluster pinned left, capsule at `Gravity.CENTER`, right
  cluster + Done pinned right — capsule center = bar center,
  exactly. The Back/Forward chevron geometry also moved +0.02
  (0.64/0.36) so its ink bounding box is exactly canvas-centered.
- **Arrow tips — third pass, verified in a REAL render.** The
  v1.28 "verified" heads were checked with a PIL approximation
  of the SVGs; CairoSVG renders of the actually-shipped markup
  show what Kurt saw: the arc ran UNDER a head narrower than
  ~1.4x the stroke, rendering as a thorn/kite. All four
  circular-arrow icons (TV toolbar Reload, the TV center
  feedback ±10 pair, the phone Reload, the phone skip pair) are
  rebuilt on one construction, checked in real renders before
  porting (CairoSVG for the exact remote.html markup; faithful
  PIL replication of the exact Canvas calls for the Java icons):
  the arc is drawn only as far as the head's BASE CENTER, and a
  filled triangular head grows out of that point — tip on the
  circle at the leading end, base corners radial at ±0.19–0.29
  of the radius (base ≈ 2–2.4x the stroke). The phone SVGs now
  use explicit arc paths + polygons (no dasharray/rotate tricks);
  the feedback glyphs lose their tip-overshoot kite; seek
  intervals and labels stay ±10.

## v1.29 (Oct 29, 2026) — studio redesign of the browser nav bar

Kurt's verdict on the v1.28 bar, with a TV photo: "The nav bar UI
looks like ass. Make it look like a design studio actually made
this." The v1.27/28 bar was a full-width opaque blue-gray slab,
every control isolated in its own outlined circle ("soap
bubbles") at the same visual weight, a huge mostly-empty URL
pill, Done as a flat tomato-red disc, and the page title on its
own strip inside the slab, so the header read as two stacked
bars. This version replaces the slab with ONE floating glass bar
and changes nothing else: same 8 stops, same order, same actions.

- **Floating glass, not a slab.** The header block is now just a
  transparent positioning frame, inset 20dp at the sides and 12dp
  at the top. The bar's elements carry a translucent vertical
  gradient (lighter at the top edge — the fake-glass sheen), a
  1dp hairline border, 20–22dp corner radii, and a soft elevation
  shadow. API 25 has no blur (no RenderEffect/RenderScript), so
  translucency + gradient + shadow is the whole trick. Dark
  appearance = graphite glass (~rgba(24,25,29,0.88), border
  rgba(255,255,255,0.14), text #F5F5F7, secondary #A1A1A6);
  light = porcelain glass (~rgba(248,248,250,0.92), border
  rgba(0,0,0,0.08), text #1D1D1F, secondary #6E6E73). Both were
  mock-verified over dark and bright backgrounds.
- **Integrated clusters; buttons lost their circles.** LEFT: one
  segmented capsule, Back | hairline divider | Forward. RIGHT:
  one segmented capsule, Mode | Phone | Gear. Segments are
  48×44dp slots with NO background at rest; focus/hover paints a
  translucent highlight + 2dp accent ring inset inside the
  cluster (never clipped, no layout shift). Accent blue still
  appears only where it means something (paired Phone, focus).
- **Address capsule with the title inside.** The center capsule
  (460×56dp) shows the host at 15sp semibold with the page title
  as an 11sp secondary second line under it — the separate title
  strip under the bar is deleted, so the header is one ~64dp row
  instead of two stacked bars. Reload is a Safari-style ghost
  inside the capsule's trailing edge (its own focus highlight is
  a circular ghost ring). The capsule itself is the URL focus
  stop; the full-URL editor behavior is unchanged.
- **Done, restrained.** A red-TINTED glass circle (~20% #FF453A
  fill, hairline red-tinted border, red ✕) in the same material
  family as the clusters; focus strengthens the fill to ~35% and
  rings it solid red. No more flat solid disc.
- **Behavior frozen.** Same `toolbarViews` order [Back, Forward,
  URL, Reload, Mode, Phone, Gear, Done], same clicks, tooltips
  (they already fall below the bar; mouse hover now hit-tests in
  reverse so Reload wins over the capsule that contains it),
  Back/Forward dimming, Phone paired tint, Mode icon swap,
  header hide/show, URL editor. The pairing card still anchors
  10dp below the header block; the unmeasured-height fallbacks
  were updated for the slimmer header (88dp bar, 90dp card).
- **Verification:** the design was prototyped as a full-width
  PIL mock (real Inter, v1.28 glyph geometry) over dark-video
  and bright backgrounds in both appearances plus a focus-state
  strip, read back and judged at full size and at 960px
  (TV-photo distance); one iteration widened the segments
  44→48dp and the capsule 440→460dp to fix the cluster/capsule
  balance before any Java was written. Manifest versionCode
  **30** / versionName **1.29**; `alive version=1.29 code=30`.

## v1.28 (Oct 29, 2026) — themed pairing card + redrawn toolbar icons + remote arrow audit + unified remote controls

Four parts, all from Kurt's v1.27 TV photo and follow-ups.

- **(Part 1) The auto pairing popup is now a themed in-window
  CARD.** The v1.27 auto variant of the pairing dialog — stock
  gray, off-theme — is gone. In its place, a card built into
  the browser pane (above the WebView, below the cursor
  overlay), anchored **top-right, 10dp below the header block,
  20dp off the right edge, ~430dp wide**, in the app's own dark
  glass: browser-chrome color at ~92% alpha, 18dp radius, 1dp
  hairline border, Inter, accent highlights; the light browser
  appearance gets the light-chrome equivalent. Server on:
  "PHONE REMOTE" label, "Pair your phone", the address in
  monospace, the PIN as large bold accent spaced digits, and
  the QR at ~104dp in a white rounded tile. Server off:
  "Phone remote is off" + "Tap this card or press OK to turn
  it on". A 3dp accent progress track under the content
  **drains full → 0 over the 10 seconds**, with a "Closes in
  Ns" line. Slide+fade in from the right (~240ms), out ~180ms.
  Because the card is a plain view — not a dialog window — it
  cannot disturb a playing video's surface (no videokick class,
  no browserDialogDepth involvement). Interaction: a tap that
  lands on the card runs the card action (off → turn the remote
  on, rebuild to the on-state, restart the full 10s; on → open
  the full manual pairing panel and hide the card); a tap
  anywhere else dismisses the card without being consumed. In
  D-pad mode, CENTER/OK performs the card action and BACK
  dismisses it — both intercepted at the activity key level
  while the card is visible, since the card is non-focusable
  by design. Trigger/lifecycle semantics carry over from
  v1.27 unchanged: once per browser session, ~700ms after
  browser entry, only with 0 phones paired, flag consumed at
  fire time, re-validated at fire time; a phone pairing while
  it's up swaps it to "Phone connected ✓" and slides it out
  ~800ms later; it hides on countdown end, screen change,
  fullscreen entry, Done, any dialog opening, and manual panel
  opening. The v1.27 auto-dialog machinery (auto variant of
  openRemotePanel, its countdown footer, the identity-checked
  clear funnel) is removed; the manual panel is byte-for-byte
  the v1.26 behavior for its existing callers. Toolbar
  tooltips are suppressed while the card is up (it takes the
  place of the old dialog-depth clause).
- **(Part 2) Toolbar icons redrawn to SF Symbols discipline.**
  Kurt's photo verdict on v1.27's set: too thick, and the gear
  read as a sun/asterisk. All eight `strokeIcon` kinds were
  redrawn — strokes ~0.07–0.075 of the canvas (was 0.13–0.15),
  glyph ~70% of the canvas, round caps/joins — and the
  geometry was **verified before porting** by rendering the
  identical numbers in a Python/PIL prototype at button scale
  (2 iterations). Back/Forward are single tall narrow chevrons
  ((0.62, 0.18) → (0.34, 0.50) → (0.62, 0.82), stroke 0.075).
  Reload: thinner 300° arc (stroke 0.07, butt cap) + a compact
  filled head — tip exactly at the arc endpoint (288°), base
  0.38·r back along the arc, corners radial. Gear: the
  prototype picked the **dashed-ring construction** — 8 arc
  segments of 20° on r=0.395 with a 0.13 stroke (band
  0.33–0.46) over a stroked ring r=0.295 and a filled hub
  r=0.095 — over trapezoid teeth (which rendered as a sun);
  it unmistakably reads as a gear at 48dp. Mouse: slimmer body
  (0.072), short wheel dash, butt-capped inset divider (the
  old round-cap divider poked nubs outside the body). D-pad:
  one plus-outline path + filled center dot. Phone: slimmer
  outline, smaller speaker dash + home dot. Done ✕ at 0.09,
  still tinted red by the caller. All tint/state machinery
  (paired accent, nav dimming, appearance retint, Mode swap)
  is unchanged in behavior.
- **(Part 3) Full arrow audit of the phone remote page
  (remote.html).** Every arrow-bearing icon was re-prototyped
  in PIL at display scale (1 iteration — the construction
  discipline below rendered correct on the first sheet after
  the v1.26 kite failure was diagnosed). **Circular arrows**
  (skip pair + nav Reload): the v1.26 head was a kite — apex
  overshooting the arc endpoint by ~8.5 units along the
  tangent, base straddling the circle, and the base sitting
  over the GAP instead of the swept arc. New construction
  (SF-style): the arc is drawn UP TO the endpoint (butt end),
  the head's **tip is exactly the endpoint**, its base center
  sits on the circle a head-length before it (0.30·r for the
  skips, 0.32·r for Reload), and the base corners step
  radially ±0.30·head-length, so the arc visibly thickens
  into a crisp point. Skip glyphs: dash re-anchored
  (rotate 28.5°) so the arc *ends* at the head (309.5°,
  top-right for forward, mirrored for back), gap upper side,
  −10/+10 labels clear of the head. Reload: 270° arc as an
  explicit path (start at 30°, end at 300°), gap upper-right,
  head at the top end with a right-and-slightly-down tangent.
  **Chevrons** (clickpad ×4, nav pills ×2, round-Back
  chevron-in-rect): re-proportioned tall-and-narrow to the
  same SF fractions as the TV set (stroke ratio stays 1.8/24
  ≈ 0.075), points verified unclipped inside their viewBoxes.
  Audited and left alone: play/pause (not an arrow), Menu
  hamburger, Done ✕ (tips verified inside the viewBox), the
  URL globe and Type keyboard chips (no arrowheads), volume
  +/−, the PIN sheet. Buttons, sizes and commands are
  identical — only icon geometry changed; the page script
  still passes `node --check`.
- **(Part 4) The phone remote's bottom controls are unified
  across modes.** Everything below the surface is now
  identical in trackpad and D-pad modes: the round row is
  [Back] [Play/Pause] [Menu] **[Done]** in both (Done was
  trackpad-only), the nav pill row (Back/Forward/Reload)
  shows in D-pad mode too (its disabled states still follow
  the canBack/canFwd state pushes — on menu screens they
  honestly read disabled), and the URL/Type launcher chips +
  Dock-zoom panels show and behave the same in D-pad mode.
  The media cluster (skip circles + volume rocker) was
  already shared and is pixel-identical. Only the surface
  (trackpad vs clickpad), the segmented control and the
  caption/hint still differ between modes. TV-side command
  safety was verified, not assumed: `remoteDone`,
  `remoteBack`, `remoteForward` and `remoteReload` were
  already browser-gated clean no-ops off-browser — unchanged.
  Two gaps were fixed: `remoteUrl` no longer drops the
  command off-browser — from a menu screen it now **opens
  the browser** on the resolved address (the URL-dialog
  normalization was factored out and shared); `remoteType`
  off-browser now answers with the "No text field focused on
  TV" note instead of silence. Fit-to-screen: the D-pad row
  set is now literally the trackpad row set, so the v1.25
  flex math carries over unchanged — at a 667px viewport the
  surface gets ~215px with panels closed, and an open panel
  takes its height from the surface; the page total stays
  exactly 100dvh.
- Manifest versionCode **29** / versionName **1.28**;
  `alive version=1.28 code=29`.

Build verification (build-verified only; the card's feel, the
redrawn icons at TV distance, the remote arrows and the
unified D-pad layout are device-test-only):

- `aapt2 dump badging`: `com.heckyeah.tv` versionCode
  **29** / versionName **1.28**, sdk 25 / target 25.
- APK: 607,464 bytes, sha256
  `eac96fea6213ac32445ac10ba55f0ce47587129953bfdd69e2f7005af665d06f`.
- `apksigner verify`: OK; signer certificate SHA-256
  `220529f9…b92fab5` (unchanged signing key).
- `unzip -t`: no errors; the APK's `assets/remote.html` is
  byte-identical to the source file.
- Remote page script extracted and passed `node --check`.

## v1.27 (Oct 29, 2026) — auto pairing popup + browser toolbar redesign

- **Auto pairing popup (replaces the v1.20 hint bubble).** The
  small "tap Phone" tooltip is retired. Now, when a browser
  session starts and no phone is paired, the pairing panel
  itself opens automatically ~700ms after the browser appears,
  with a **10-second auto-dismiss countdown** shown in a footer
  line inside the panel ("Closes automatically in Ns"). Tapping
  **Turn on** (or New PIN / Disconnect all) inside the popup
  re-opens the panel and restarts the countdown at a full 10s,
  so you always have time to read the address and PIN. If a
  phone pairs while the popup is up, the countdown stops, the
  footer flashes "Paired!", and the panel closes ~800ms later.
  A manually opened panel (Phone button, Home, browser menu)
  never gets a countdown. The popup fires at most once per
  browser session; the session flag is consumed even if the
  popup is suppressed at fire time (fullscreen or a dialog
  already up), so it never appears late. Toolbar tooltips stay
  suppressed while it is open via the existing rule that hides
  them under any tracked dialog.
- **Browser toolbar redesigned.** The bar is now three
  sections in one row. **Left:** Back and Forward as circular
  icon buttons. **Center** (held screen-centered by equal
  spacers): the URL pill, shortened to a fixed ~300dp, showing
  **only the site's host** (e.g. `atlantic.st`) — the URL
  editor still opens with the full URL — with the Reload
  circle right next to it. **Right:** Mode, Phone, Settings and
  Done circles. All buttons are full circles (Back/Forward
  52dp, Reload 50dp, right cluster 48dp) with bigger
  custom-drawn icons (~26–28dp glyphs on a 30dp canvas,
  extending the v1.25 canvas-icon helper): bold chevrons for
  Back/Forward, an arc-and-arrowhead Reload, a toothed gear
  for Settings, and Mode/Phone keeping their v1.25 glyphs at
  the new size. Back/Forward icons now dim when that direction
  is unavailable. **Done** has no text: a red ✕ (#FF453A) on a
  translucent-red circle in both dark and light appearances,
  with a solid-red ring when focused. D-pad toolbar order,
  hover/focus name tooltips, paired-state Phone tint, Mode
  icon semantics and the header auto-hide are unchanged.

## v1.26 (Oct 29, 2026) — center action feedback animation + fixed skip glyphs

- **(G) Center-screen action feedback (TV side).** A seek
  (±10s) or a play/pause toggle in the browser now flashes a
  big glyph in the center of the screen, YouTube-style: a
  92dp decor-level `ImageView` (black circle at ~55% alpha,
  white glyph drawn at runtime into a bitmap — play triangle,
  pause bars, or a circular arc arrow with −10/+10) that
  scales/fades in over ~120ms, holds ~350ms, and fades out
  over ~300ms; retriggering swaps the glyph and restarts via
  a generation counter, and leaving the browser cancels it.
  Because the view lives at the decor level it renders the
  same over the inline browser, real fullscreen and
  pseudo-fullscreen. Hooks are the shared paths, so both
  remotes get it: the phone's seek command flashes its icon
  immediately with the command, and the v1.25 play/pause
  toggle flashes the *resulting* state — the JS path knows it
  directly (`js-paused` → pause icon, `js-played` → play);
  the mediakey fallback samples the video poll ~450ms after
  the key (retrying once if no poll has landed yet) and
  flashes only when the playing state provably flipped —
  inconclusive means no icon, never a guessed one (in real
  fullscreen the poll is cleared by design, so that path
  stays silent).
- **(H) Phone skip glyphs rebuilt (remote.html).** The
  v1.24 hand-computed arc paths rendered with the arrowhead
  floating detached at the top. The glyph is now a `<circle>`
  with `stroke-dasharray` (r 15.5 in a 48 viewBox,
  circumference ≈ 97.4; dash "76 21.4" = a 280.9° arc with
  the gap rotated to center at the top) plus an arrowhead
  triangle inside a `<g>` rotated about the center to the
  arc-end angle, drawn pointing along the tangent so the
  rotation aligns it exactly — no hand-computed tangent
  coordinates anywhere. The back glyph is the forward glyph
  mirrored (`translate(48,0) scale(-1,1)`), so the two are
  symmetric by construction; the −10/+10 labels are HTML
  spans outside the SVG and are never mirrored. Geometry
  verified numerically (arc-end −50.45°, head direction
  39.55° = the exact clockwise tangent there). Button size,
  glass style, captions and seek commands unchanged.

## v1.25 (Oct 29, 2026) — toolbar icons + tooltips, menu-swipe trackpad, fit-to-screen remote page, play/pause reliability, Dock-style URL/Type panels

Six parts, from a screenshot review round of the phone remote:

- **(A) Browser toolbar icons + tooltips (TV side).** The Mode
  and Phone toolbar buttons are icons now, drawn at runtime on a
  Canvas (`strokeIcon`: mouse = rounded-rect body + wheel dash +
  top-third line; D-pad = cross of two rounded rects + center
  circle; phone = tall rounded rect + speaker dash + home dot) —
  font glyphs for these are not trustworthy on Fire OS. The Mode
  icon follows the old text label's rule exactly: it shows the
  CURRENT sub-mode. Tinting is a `PorterDuff SRC_IN` color
  filter on the bitmap drawable, so `refreshPhoneBtn()`'s paired
  accent now drives the Phone icon (Home's text Phone button is
  unchanged), and dark/light chrome re-tints via
  `updateModeBtn()` / `refreshPhoneBtn()`. A single name bubble
  (pairTip's visual family, child of the browser pane root above
  the WebView, below the cursor overlay) names the toolbar
  element under attention — Back / Forward / URL / Reload /
  Mouse-or-D-pad / Phone / Settings / Done. Mouse hover hit-tests
  the cursor against the toolbar rects in `moveCursor()` with a
  350 ms dwell; D-pad toolbar focus schedules it at 150 ms from
  `styleToolbarFocus()`. It hides on any tap, dialog open,
  fullscreen, header hide, or leaving the browser, and never
  stacks with the pairTip. Position: centered on the element,
  clamped in the pane, above it when there's room — the toolbar
  sits at the screen's top edge, so in practice it lands just
  below the element.
- **(B) Trackpad-on-menus swipe navigation (phone page + one TV
  branch).** When the page is in trackpad mode but the TV is not
  in the browser (trackpad manually picked on Home / Search /
  Settings / category screens), the surface stops driving a
  cursor and navigates like the Apple TV Remote's touch surface:
  drags feed step accumulators — every 48 px along the dominant
  axis (recomputed per segment) fires one `{"t":"key"}` step and
  is subtracted, so a long drag walks several tiles and a flick
  fires one; a tap sends `ok`; two-finger vertical scroll steps
  up/down with the same accumulator (fingers down = step up,
  matching the browser scroll convention). The caption reads
  "TRACKPAD — MENU · SWIPE TO NAVIGATE" and the surface hint
  swaps to "Swipe to move · tap to select / Hold for
  long-press". TV side: `remoteLongTap` off-browser now injects
  a long DPAD_CENTER (new `injectKeyHold`, ~600 ms hold through
  the v1.23 instrumentation path, dispatch fallback kept), so
  Home tiles open their long-press menus; in-browser longtap is
  unchanged. When the TV returns to the browser, the same
  surface drives the cursor again — context is evaluated per
  state push, no reload.
- **(C) The remote page fits the screen — no scrolling, ever.**
  `html, body` are height 100% with overflow hidden; the body is
  a flex column at `100dvh` (100vh fallback) where every row is
  `flex: 0 0 auto` except the surface (`flex: 1 1 auto`,
  min-height 150 px, fixed aspect ratio dropped) — the surface
  absorbs all slack. Section gaps tightened to 10 px; a
  max-height 700 px media query compacts round buttons to 48 px,
  skip circles to 52 px, the rocker to 104 px and bars to 40 px,
  and under 620 px the footnote line is hidden. Flex math at a
  667 px-tall viewport (panels closed, conservative non-media
  numbers): 504 px of fixed rows + gaps leaves a ~163 px
  surface; with the short-screen query active at that height
  the fixed rows shrink to ~452 px and the surface is ~215 px.
- **(D) Play/Pause works first tap, every time.** Phone side:
  the button is press-bound like the volume halves (fires on
  touchstart/mousedown, 700 ms timestamp guards against the
  trailing synthetic click, bare click kept for keyboard /
  assistive use) instead of a bare click that a shifting page
  could swallow. TV side: one shared toggle for the phone
  command AND the physical media keys (`togglePlayJs`). The JS
  scan recurses the top document + same-origin iframes (the
  video poll's approach): pause the largest PLAYING video
  (`js-paused`) or play the largest paused one (`js-played`)
  among visible rects ≥ 160×90. If the scan finds nothing —
  the player lives in a cross-origin iframe — or the browser
  is in (pseudo-)fullscreen where the scan is blind, the
  fallback injects `KEYCODE_MEDIA_PLAY_PAUSE` through the
  instrumentation path; a one-shot flag in `dispatchKeyEvent`
  lets that one key fall through to the WebView (Chromium's
  media-session handling reaches cross-origin players) instead
  of looping back into the toggle, and in real fullscreen it
  lands in `handleFullscreenKey`'s center tap as before. Each
  press logs one line: `playpause via=js-paused | js-played |
  mediakey | none`.
- **(E) Menu is a round button; the D-pad Menu pill is gone.**
  The full-width "☰ Menu" pill row is deleted. Menu is now a
  56 px round glass button (☰ inline SVG) in the round row:
  [Back] [Play/Pause] [Menu] in D-pad mode, [Back] [Play/Pause]
  [Menu] [Done] in trackpad mode (added there — in the browser,
  MENU opens the browser nav menu). Same `{"t":"key","k":"menu"}`
  command.
- **(F) URL / Type bars collapse into Dock-style launchers
  (trackpad mode).** The two always-visible bars are replaced by
  one launcher row of two glass chips — globe "URL" and keyboard
  "Type" (inline SVGs). Tapping a chip pulses it (scale 1 →
  1.12 → 1, 180 ms, like a Dock icon) and grows its panel out of
  it: the wrap's max-height animates 0 → panel height over
  260 ms with `cubic-bezier(.2,1.35,.3,1)` overshoot while the
  inner scales 0.85 → 1 / fades in with transform-origin at the
  top edge under the owning chip (22% / 78%); collapse reverses
  at 180 ms ease-in. The URL panel is the existing input + Go;
  the Type panel is the existing input + Send + Enter, and the
  server-note line moved inside it. One panel at a time (tapping
  the other chip swaps, tapping the open chip collapses),
  panels start collapsed, opening never auto-focuses an input,
  and any mode switch collapses both. An open panel takes its
  height from the surface (the only flex-grow row;
  `body.panelOpen` lowers the surface min-height to 88 px), so
  the fit from (C) holds with a panel open too.

## v1.24 (Oct 29, 2026) — Siri Remote-style volume rocker + skip icons on the phone remote

Remote-page visuals only (`assets/remote.html`, 32,058 → 36,879
bytes; script passes `node --check`). No protocol change, no
Java change beyond the version bump: every button sends exactly
the messages it sent in v1.23. Modeled on the 1st-generation
Siri Remote — glossy black, round buttons, and a tall vertical
volume rocker.

- **Volume rocker.** The two separate `Vol −` / `Vol +` pills
  are now ONE vertical capsule (64 px wide, 120 px tall, fully
  rounded, same dark glass fill as the round buttons) with a
  hairline divider across the middle, a `+` glyph in the top
  half (volume up) and a `−` in the bottom half (volume down),
  each half with press feedback for the whole hold. **Holding a
  half repeats the volume step**: v1.23's key-only repeat helper
  is generalized into `startRepeat(sendFn, el, intervalMs, dir)`
  / `stopRepeat()` — it takes a send-thunk, fires it immediately
  on press-down, then re-fires after the same
  `REPEAT_DELAY_MS = 400` ms at the caller's interval (keys stay
  at 100 ms; volume uses `VOL_REPEAT_INTERVAL_MS = 120` ms,
  slightly slower because TV volume steps are coarse). The
  single-active-repeat rule is now shared across both controls:
  starting a volume repeat stops a clickpad repeat and vice
  versa. Stop paths are the clickpad's discipline, unchanged:
  release, `touchcancel`, sliding off the half, mode switch,
  WebSocket close, document `mouseup`, window blur — plus the
  timestamp guards that keep touch and mouse from double-firing
  (a quick tap is still exactly one step).
- **Skip icons.** The `⟲10s` / `10s⟳` text pills are now two
  circular glass buttons (60 px) in the iOS
  `gobackward.10` / `goforward.10` style: an inline SVG arc of
  ~300° (white, 2 px stroke, round caps) with a triangular
  arrowhead at the opening — top-left, counter-clockwise for
  back; top-right, clockwise for forward — drawn around a
  centered `−10` / `+10` label. The label is an HTML `<span>`
  overlaid on the SVG (crisper text than SVG `<text>` across
  phone browsers). Still single-fire, still sending
  `{"t":"seek","s":∓10}`.
- **Layout.** The old four-across media pill row is now a media
  cluster echoing the Siri Remote: on the left, the two skip
  circles side by side, each with a small caption underneath
  ("Skip back" / "Skip forward"); on the right, the 120 px
  volume rocker, vertically centered against them. The cluster
  shows in both trackpad and D-pad modes, as before. Nothing
  else moved: surface, round Back/Play-Pause/Done row, nav
  pills / Menu pill, URL bar, type bar and notes are untouched,
  and no function was dropped.

## v1.23 (Oct 29, 2026) — hold-to-repeat, slower trackpad scroll, input injection for native popups

Three phone-remote fixes. Two live entirely in the remote page
(`assets/remote.html`, 27,584 → 32,058 bytes; script passes
`node --check`); the third reworks how the app delivers taps
and phone keys to the page.

- **Hold-to-repeat on the clickpad directions** (remote page
  only; no protocol change). In v1.22 each phone tap/swipe was
  one discrete key — the page now closes the nuance the v1.22
  notes flagged. Pressing a direction zone (up/down/left/right)
  fires its key **immediately on press-down**, and if the press
  is still held after `REPEAT_DELAY_MS = 400` ms it keeps firing
  every `REPEAT_INTERVAL_MS = 100` ms until the press ends.
  Repeats are generated phone-side by sending more of the same
  `{"t":"key",...}` messages. A shared
  `startKeyRepeat(dir)` / `stopKeyRepeat()` pair owns the one
  active repeat: a second finger (or a new press) on another
  zone *replaces* it, never stacks. The zone's `.press`
  highlight now stays lit for the whole hold instead of flashing
  once. Every exit stops the timers: touch release and
  `touchcancel` (both land in the existing `cpEnd`, which
  suppresses its synthetic click as before), the finger sliding
  off the zone (checked per `touchmove` via
  `elementFromPoint`), a recognized flick, a mode switch
  (`applyMode`), WebSocket close, and — for the mouse fallback —
  `mouseup`, leaving the surface or the zone, and window blur.
  A quick tap is unchanged in effect (one key, no repeats; the
  400 ms delay guarantees it), it just fires on press-down
  rather than release. Flick-vs-hold is disambiguated by the
  existing 44 px swipe threshold: recognizing a flick cancels
  the hold outright, so a finger lingering after a flick never
  starts repeating; and because a touch that began on a
  direction zone already delivered that key on press-down, the
  flick does not add a second one (a flick that began off a
  zone — OK or the gaps — fires exactly one key, at recognition,
  as before). **OK never repeats** (repeated selects would be
  destructive); Back, Menu and all other buttons are
  single-fire, unchanged. Mouse holds repeat too
  (`mousedown` fires + arms, the trailing `click` is
  suppressed as already-handled).
- **Trackpad two-finger scroll slowed by half** (remote page).
  The two-finger vertical scroll now scales each finger delta
  by `SCROLL_FACTOR = 0.5` (defined at the top of the script
  next to the repeat constants) into the existing accumulator.
  The accumulator is fractional end-to-end: scaled deltas build
  up across `touchmove` events, whole scroll units are sent
  (preserving the existing 3× output gain and sign convention),
  and the fractional remainder is carried into the next move —
  so slow small drags still register smoothly instead of being
  truncated away per event. Cursor movement gain, tap and
  long-press, and the clickpad are untouched.
- **Input injection for native WebView popups** (`MainActivity`;
  the only Java change besides the version bump). Bug, from a
  TV photo: on a streaming site, tapping an HTML `<select>`
  (a "Season 1" dropdown) opens Android WebView's *native*
  select popup — a framework list in its **own window** floating
  over the page. Our taps were `MotionEvent`s dispatched
  directly to a view in our window, so they landed on the page
  *underneath* the popup and could never choose an item, from
  either the phone remote or the TV remote's mouse mode. Taps
  and phone keys now go through the **system input queue**
  (`Instrumentation.sendPointerSync` / `sendKeySync`), which
  delivers to the top window at the coordinates like a real
  finger: the select popup when it is open (it is created by
  the WebView in our own process/UID, so injection is
  permitted), the page otherwise; keys likewise reach the
  foreground window automatically, and this app's own
  `dispatchKeyEvent` interception still runs for injected keys
  exactly as for hardware, because that is how hardware keys
  arrive.
  - `dispatchTap` / `dispatchFsTap` / `dispatchLongTap` keep
    their names, callers and semantics (page tap vs fullscreen
    tap vs long press), but build the gesture for injection:
    events carry `InputDevice.SOURCE_TOUCHSCREEN` and a single
    monotonic `downTime`; a tap is DOWN+UP ~40 ms apart; a
    long press is DOWN, ~550 ms sleep, UP — a genuine hold, so
    the WebView's link long-press menu flow is preserved.
    Coordinates are **display coordinates**: callers pass
    cursor/window coordinates (window is fullscreen), converted
    explicitly through the root view's / fullscreen
    container's `getLocationOnScreen` rather than any
    WebView-local conversion. Phone-remote taps inherit the
    fix automatically — the `RemoteServer.Host` tap/long-tap
    callbacks funnel into these same three functions (verified:
    no other path dispatches its own touch events).
  - The v1.22 `injectKey` now tries `sendKeySync` (DOWN+UP)
    first; the previous dialog/activity direct dispatch is
    kept verbatim as `injectKeyViaDispatch` and is the
    per-event fallback. This also makes phone D-pad keys work
    *on* the select popup (Up/Down/OK/Back in the list).
  - Thread model: `sendPointerSync`/`sendKeySync` block until
    delivery, so all injection runs on one lazily created
    single-thread executor, which also serializes every
    gesture in submission order; the `Instrumentation` instance
    is created lazily and only touched on that thread. The UI
    thread only enqueues.
  - Failure handling: any exception (e.g. `SecurityException`)
    falls back for that gesture only — the whole gesture via
    view dispatch if its DOWN never landed, or a completing UP
    if it did (never a duplicated gesture) — and logs
    `inject fallback <tap|fstap|longtap|key N>: <exception>`
    under `HeckYeahFS`. The first successful injection logs
    one `inject mode=instrumentation` line per app session.
  - The video scroll clamp/poll, cursor behavior and
    everything else are unchanged.
- Manifest versionCode **24** / versionName **1.23**;
  `alive version=1.23 code=24`.

Build verification (build-verified only; hold-repeat feel,
scroll sensitivity and popup tapping are device-test-only):

- `aapt2 dump badging`: `com.heckyeah.tv` versionCode
  **24** / versionName **1.23**, sdk 25 / target 25.
- APK: 595,176 bytes, sha256
  `f6a2990aeb1198f0d40aeaa393a8abc15d5d9946d58cbd50a1ba04b071201833`.
- `apksigner verify`: OK; signer certificate SHA-256
  `220529f9…b92fab5` (unchanged signing key).
- `unzip -t`: no errors; the APK's `assets/remote.html` is
  byte-identical to the source file.
- Remote page script extracted and passed `node --check`.

## v1.22 (Oct 29, 2026) — phone D-pad mode + Home pairing + Apple TV-style remote page

Three additions, all around the phone remote (Kurt's ask: the
phone should be able to drive the main menu too, pairing should
be reachable from Home, and the phone page should look like the
Apple TV Remote app).

- **D-pad mode for non-browser screens.** New WebSocket command
  `{"t":"key","k":"up|down|left|right|ok|back|menu"}` (the
  v1.19 `k:"enter"` type-field command is unchanged and still
  routes to the page-JS Enter). `RemoteServer` forwards it to a
  new `Host.remoteKey`, implemented in `MainActivity` by
  synthesizing the matching `KeyEvent` (DPAD arrows/center,
  BACK, MENU) as an ACTION_DOWN + ACTION_UP pair and delivering
  it exactly where a physical remote press lands:
  - **A tracked dialog is showing** → the pair goes to the
    dialog's own `dispatchKeyEvent`. Physical keys target the
    foreground window, so D-pad moves dialog focus and BACK
    cancels the dialog instead of acting on the screen behind
    it. To make this possible, `trackDialog` now records the
    showing dialog in an `openDialog` field, and the three
    dialogs that previously bypassed the funnel (RW tile menu,
    entry/mirror dialog, OMDb key editor) are now tracked too —
    so phone keys work under *every* dialog.
  - **Otherwise** → the pair goes through the activity's own
    `dispatchKeyEvent`, so the app's interception layer runs
    identically to hardware keys (MENU handling, fullscreen
    first, browser key swallowing) and unhandled keys fall
    through to the stock focus pipeline. BACK needs no special
    case: it is not a browser key, so the pair falls to the
    stock Activity callback (DOWN starts BACK tracking, UP
    fires this app's `onBackPressed` override). Keys therefore
    work on Home, category pages, Search, Settings, dialogs,
    and in the browser (where they behave exactly like the
    physical D-pad does today, e.g. moving the browser cursor).
    One hardware nuance that cannot be replicated over discrete
    messages: key auto-repeat on hold — each phone tap/swipe is
    one discrete press.
  - State JSON gains a **`"screen"`** field (`home` / `search`
    / `settings` / `browser` / `other`) alongside the existing
    `"browser"` bool, and `showScreen()` now calls
    `pushRemoteState()` on **every** screen change (previously
    pushes happened only on navigation callbacks, browser
    open/close and the 2.5 s heartbeat), so a paired phone
    follows the TV's screen/mode without lag.
- **Pairing from the main menu.** Home's header row is now
  Search · **Phone** · Settings (Kurt's explicit placement).
  The Phone button opens the *existing* pairing panel
  (`openRemotePanel`, reused unchanged — including its inline
  Turn-on when the server is off; nothing in the panel is
  browser-specific, and its dialog already rides `trackDialog`,
  whose video kick no-ops off-browser). Its label tints accent
  `#0A84FF` while any phone is paired, same rule as the browser
  toolbar button: `refreshPhoneBtn()` now updates both buttons,
  and `buildHome()` re-applies the tint on every rebuild.
- **Remote page redesigned** (`assets/remote.html`, 14,615 →
  27,584 bytes; script passes `node --check`). Pure-black
  (`#000`) Apple TV Remote styling: `-apple-system` stack,
  hairline `rgba(255,255,255,.1)` separators, sparing `#0A84FF`
  accent, safe-area insets, hidden scrollbars. Top bar keeps the
  connection dot + live title/URL (restyled, truncated). One
  large rounded surface (radius 30px, `#2C2C2E → #1C1C1E`
  gradient, hairline border) swaps content by mode:
  - **Trackpad mode** (TV in browser): the v1.19 gesture engine
    unchanged (accelerated drag, tap, 500 ms long-press,
    two-finger scroll, mouse fallback), with a centered hint
    that fades after the first touch.
  - **D-pad mode** (TV anywhere else): a clickpad — 92px
    circular OK button with radial highlight, four chevron tap
    zones sending the new key commands, decisive-swipe
    detection (a flick anywhere on the surface fires that
    direction, clickpad-style), and scale/opacity press
    feedback on the active zone.
  - **Mode switching is automatic** (state `browser` true →
    trackpad, false → D-pad), with a segmented
    `[Trackpad | D-Pad]` control under the top bar: tapping it
    overrides manually, and the override sticks until the TV's
    browser flag *next changes*, then auto resumes. The active
    mode is announced in small gray caps (`TRACKPAD — BROWSER`
    / `D-PAD — MENU`).
  - Below the surface: a row of 56px round glass buttons —
    Back (iOS-style chevron-in-rounded-rect, sends the new
    `key/back`, i.e. the TV remote's Back: dialog cancel,
    page-back, screen-back), Play/Pause (combined glyph), and
    Done (browser only; hidden in D-pad mode). A pill row holds
    Nav Back / Forward / Reload in trackpad mode and swaps to a
    single **Menu** pill (sends `key/menu`) in D-pad mode. The
    URL bar and type-into-page field + Send + Enter are kept
    (restyled as inset dark fields; shown in trackpad mode,
    where they function), as are the volume −/+ and seek
    ⟲10s / 10s⟳ pills (visible in both modes) and the server
    note line. All main-control icons are inline stroke SVGs —
    no emoji glyphs. The PIN sheet is restyled to match (black
    sheet, big letter-spaced PIN input, accent Connect button,
    `#FF453A` errors); the pairing flow, protocol, message
    shapes and reconnection behavior are otherwise identical —
    the only protocol addition is the one `key` command.
- Manifest versionCode **23** / versionName **1.22**;
  `alive version=1.22 code=23`.

Build verification (build-verified only; D-pad key delivery,
dialog routing, Home pairing and the new page's feel are
device-test-only — no emulator exercises the LAN server):

- `aapt2 dump badging`: `com.heckyeah.tv` versionCode
  **23** / versionName **1.22**, sdk 25 / target 25.
- APK: 591,080 bytes, sha256
  `e5cbdf18e95491340166b2aa77d70d00096cf76ea759036ffbe235da9956dc97`.
- `apksigner verify`: OK; signer certificate SHA-256
  `220529f9…b92fab5` (unchanged signing key).
- `unzip -t`: no errors; the APK's `assets/remote.html` is
  byte-identical to the source file.

## v1.21 (Oct 28, 2026) — revive black video after dialogs/pairing (video-layer kick)

Field report on v1.20: a video is playing inline; the user
opens the pairing panel, pairs the phone, and closes the
panel — the video area is **black and stays black** until
the browser bar is revealed, whose resize revives it.

Mechanism: on this API-25 Chromium, page video renders as a
hardware surface shining through a hole the compositor cuts
in the page. A second window (the app's dialogs are real
`AlertDialog`s) opening/closing over the activity desyncs
that hole, and only a WebView size change forces Chromium
to recompute it — exactly what showing/hiding the header
does. The fix reproduces that resize programmatically at
the moments the desync can occur.

- **`kickVideoLayer(why)`:** one guarded 1px nudge — the
  bottom padding of `browserCol` (the WebView's direct
  parent) is set to 1px (its current padding captured
  first), `requestLayout` on the column and the WebView
  plus `webView.invalidate()`, then after ~50 ms the
  captured padding is restored exactly and layout is
  requested again. A re-entrancy flag covers the restore
  window. No header or visibility toggling: nothing
  visible changes (1px for one frame is the point). Each
  kick that actually fires logs `videokick <why>` under
  `HeckYeahFS`.
- **Gating:** browser screen current, no real fullscreen
  session, WebView/column live, and the freshest video
  poll (< 2 s old) reporting a playing video (`videoFound`
  + `videoDataAt`). One stated exception: `why="pair"`
  also fires with **no poll data at all**
  (`videoDataAt == 0`), because a first pairing usually
  happens while the pairing dialog has starved the poll
  (the poll skips while any dialog is open). `"dialog"`
  and `"tip"` get no exception, and stale poll data never
  kicks.
- **Call sites (three):** (a) the shared `trackDialog`
  dismiss funnel — when `browserDialogDepth` returns to 0,
  a posted `kickVideoLayer("dialog")`. The pairing panel,
  which previously open-coded its own show/dismiss
  counting, now uses a `trackDialog` overload (its dismiss
  cleanup runs as an extra runnable) so the reported flow
  is covered by the same funnel. (b)
  `remoteSessionsChanged()` on the paired-count transition
  0 → >0 (`lastRemotePairedCount`), kicked *before* the
  pairing tip is hidden so the pair kick is the one that
  fires/logs when both apply. (c) `hidePairTip()` when it
  actually hid a **visible** tip while the browser is
  current.
- No timers, no heartbeat kicks, nothing else changed.
  Manifest versionCode **22** / versionName **1.21**;
  `alive version=1.21 code=22`.

Build verification (build-verified only; the black-video
revival itself is device-test-only):

- `aapt2 dump badging`: `com.heckyeah.tv` versionCode
  **22** / versionName **1.21**, sdk 25 / target 25.
- APK: 586,984 bytes, sha256
  `b2ba58920c4f7ea7e009b0415e2ad2b60eb43bcb3b27da732438750e4953488a`.
- `apksigner verify`: OK; signer certificate SHA-256
  `220529f9…b92fab5` (unchanged signing key).
- `unzip -t`: no errors.

## v1.20 (Oct 28, 2026) — Phone button in the browser nav + pairing tooltip

Two discoverability additions for the v1.19 phone remote; no
server or protocol changes.

- **Phone button in the browser toolbar:** a plain-text "Phone"
  button (no glyph — the bundled Inter font's symbol coverage on
  Fire OS is not trustworthy) sits between the Mouse/D-pad mode
  button and the ⚙ gear in both layout order and the D-pad
  toolbar-focus order (`toolbarViews`), styled exactly like its
  neighbors (focus shown by the pill ring, no scale). Tapping it
  opens the existing v1.19 pairing panel (which still handles
  the server-off state with its inline "Turn on"). The label
  tints accent `#0A84FF` while at least one phone is paired and
  returns to the normal toolbar color at zero; the tint is
  refreshed on session changes, on server start/stop, on every
  browser entry, and via the 2.5 s state heartbeat, so it cannot
  go stale.
- **Launch tooltip:** when the browser becomes current with
  zero phones paired, a hint bubble appears once per browser
  session (~700 ms in, re-validated at fire time: browser still
  current, no dialog open, not fullscreen, still unpaired):
  "Pair your phone to use it as a trackpad and keyboard for
  this browser — tap Phone in the toolbar." The bubble is a
  non-clickable, non-focusable TextView in the browser pane
  (above the WebView, below the cursor overlay), positioned
  just below the header with its right edge aligned to the
  Phone button's right edge (computed from on-screen positions
  at show time, fixed-margin fallback). It fades in over
  200 ms, auto-hides after 6 s, and hides immediately when the
  Phone button is tapped, a phone pairs, the screen changes,
  fullscreen starts, or the browser closes. The shown-flag
  resets on browser close, so a later launch with still-zero
  phones hints again — but never twice in one session.

## v1.19 (Oct 28, 2026) — phone remote (Core + A–E)

A phone on the same Wi-Fi network becomes a trackpad and browser
controller through a web page the app itself serves. Core + all
five extras were picked.

- **Server (`RemoteServer`, new):** hand-rolled HTTP + WebSocket
  (RFC 6455: SHA-1/Base64 accept, masked client frames unmasked,
  ping→pong, close, minimal fragmentation tolerance) on port
  18765, framework APIs only. 1 accept thread + at most 6
  connection threads; every command is posted to the UI handler;
  everything exception-guarded so the server cannot crash the
  app. Serves `assets/remote.html` at `GET /`, pairing at
  `POST /pair`, control channel at `GET /ws` (cookie-gated).
- **Lifecycle/settings:** pref `remote_on`, **default OFF**;
  Settings row "Phone remote"; server starts in `onCreate` when
  the pref is on, stops in `onDestroy`, and follows the toggle
  immediately. Browser menu and browser settings gained
  "Phone remote…", opening the TV pairing panel (address, PIN,
  QR, paired count n/2, "New PIN", "Disconnect all"; an inline
  "Turn on" when the server is off).
- **Pairing/security (Core + E):** 4-digit PIN regenerated at
  every server start; a successful pair issues a UUID session
  cookie kept **in memory only**; max **2** concurrent sessions;
  5 failed attempts lock pairing for 60 s (HTTP 429); unpaired
  `/ws` upgrades get 401. "New PIN" regenerates the PIN and drops
  all sessions; "Disconnect all" drops sessions.
- **Phone page (`assets/remote.html`, new):** self-contained dark
  page — PIN overlay (auto-shown when the socket is refused),
  live header (current title + URL, connection dot), trackpad
  (drag = move with velocity acceleration; the server applies a
  1.6× gain before `moveCursor`; tap = click; ~500 ms hold =
  long-tap; two-finger vertical drag = scroll), Back / Forward /
  Reload / Play-Pause / Done, URL bar (submit reuses
  `loadResolved`, the exact normalization of the TV URL dialog),
  "type into page" field + Enter (extra C), and a media row
  (extra D). A footer note appears when the browser isn't open
  on the TV. Page JS was extracted and passed `node --check`.
- **Commands reuse the app's own internals:** move →
  `moveCursor` (edge-scroll and fade-poke intact), tap/long-tap
  → `dispatchTap`/`dispatchFsTap`/`dispatchLongTap` at the
  cursor (fullscreen taps go to the fullscreen surface), scroll
  → `doScrollBy` (so the document/video clamps and the
  fullscreen and pseudo-fullscreen freezes govern the phone
  exactly like the remote), play/pause → FS center tap or
  `togglePlayJs`, Done → `closeBrowser`. Page commands are
  browser-gated; volume is global.
- **State pushes:** `{t:"state", title, url, fs, mouse, browser,
  canBack, canFwd, paired}` on navigation callbacks
  (`onPageStarted`/`onPageFinished`/`doUpdateVisitedHistory`/
  `onReceivedTitle`), on real- and pseudo-fullscreen
  transitions, on pairing changes, on browser close, and on a
  2.5 s heartbeat. A failed type-in sets a one-shot `note` in
  the next push ("No text field focused on TV").
- **Extra A — QR code (`QrCode`, new):** compact QR generator
  (byte mode, ECC level M, versions 1–10, algorithm structure
  after the MIT-licensed Nayuki generator) rendered to a bitmap
  with a 4-module quiet zone in the pairing panel. Verified
  standalone on a desktop JVM before building: a
  25-character `http://<device-ip>:18765` address selects
  version 2 / 25×25 and a 94-character input selects version
  6 / 41×41 (4 blocks); format-information BCH validated with
  ECC indicator 00 (M); Reed-Solomon syndromes all zero under
  an independent GF(256) implementation; the encoded bitstream
  round-trips to the exact input.
- **Extra B — mDNS:** `NsdManager` advertises "HeckYeah TV"
  (`_http._tcp.`, port 18765) with the server start/stop, all
  callbacks guarded (failure never affects the server). The
  panel prints `http://HeckYeah-TV.local:18765` as a secondary
  address (resolution reliable on iPhone; Android may need the
  IP, which comes from `WifiManager` with a `NetworkInterface`
  fallback).
- **Extra C — type into page:** injected JS inserts the text at
  the caret of the focused input/textarea/contenteditable
  (setRangeText where available, `execCommand('insertText')`
  for contenteditable) in the top document and same-origin
  iframes, dispatches `input`, and reports back; Enter fires
  keydown/keypress/keyup (keyCode 13) at the focused element.
- **Extra D — media keys:** volume via `AudioManager.
  adjustStreamVolume(STREAM_MUSIC, …, FLAG_SHOW_UI)`; seek
  moves the biggest playing (else first) `<video>` by ±10 s
  (same-origin iframes included).
- **Manifest:** versionCode **20** / versionName **1.19**;
  permissions are now INTERNET, ACCESS_NETWORK_STATE,
  ACCESS_WIFI_STATE and MODIFY_AUDIO_SETTINGS (the last two are
  new, both normal permissions).
- `alive version=1.19 code=20`.

Build verification (compile-verified; **all network behavior is
device-test-only** — nothing about the server, pairing,
WebSocket, mDNS, or the phone page has run on hardware yet):

- `aapt2 dump badging`: `com.heckyeah.tv` versionCode **20** /
  versionName **1.19**, sdk 25 / target 25, permissions
  INTERNET + ACCESS_NETWORK_STATE + ACCESS_WIFI_STATE +
  MODIFY_AUDIO_SETTINGS.
- APK: 586,984 bytes, sha256
  `738344285e0acdef1ec4392da4daf883018bc90ebf29fcd712902ca4b2bdd7a0`;
  `assets/remote.html` (14,615 bytes) present in the APK.
- `apksigner verify`: OK; signer certificate SHA-256
  `220529f9…b92fab5` (unchanged signing key).
- `unzip -t`: no errors.
- QR encoder: desktop-JVM checks above passed (version
  selection, format BCH, RS syndromes, payload round-trip).
- Phone page JS: extracted from the HTML and passed
  `node --check`.

## v1.18 (Oct 28, 2026) — package rename + public repo prep

- **Package renamed** to `com.heckyeah.tv` (previously a personal
  namespace). Android treats this as a different app: it installs
  fresh, alongside any earlier build, and does not inherit the
  earlier app's data.
- **Personal references scrubbed** from code comments and docs;
  field reports are described in neutral terms throughout.
- **build.sh hardened:** signing passwords come from the
  `KEYSTORE_PASS` / `KEY_PASS` environment variables (no
  credentials in the repository); a missing keystore is generated
  as a local development keystore; JDK and SDK locations come from
  `JAVA_HOME` / `ANDROID_HOME` with local fallbacks.
- **Repo prep:** GitHub front section (top of this file), MIT
  `LICENSE`, and a `.gitignore` that keeps `keystore.jks`, APK
  files, and `build/` out of version control.
- Includes the v1.17 cursor-fade fix unchanged
  (`alive version=1.18 code=19`).

Build verification:

- `aapt2 dump badging`: `com.heckyeah.tv` versionCode **19** /
  versionName **1.18**, sdk 25 / target 25, permissions
  INTERNET + ACCESS_NETWORK_STATE.
- APK: 562,344 bytes, sha256
  `654de1abf1807470459b6cb9172006c2964626d0bcd42b11764bd522127f0f33`.
- `apksigner verify`: OK; signer certificate SHA-256
  `220529f9…b92fab5` (unchanged signing key).
- `unzip -t`: no errors.

## v1.17 (Oct 28, 2026) — cursor fade works in pseudo-fullscreen too

v1.16 CONFIRMED WORKING on a Fire TV Stick (Oct 28): the
problem site's video now fullscreens properly and fills
the screen — via the pseudo-FS path (header force-hide +
scroll freeze; `fsView` stays null the whole time). New
bug from that same session: **in fullscreen the mouse
cursor no longer fades away.**

Root cause: the v1.3 cursor fade was real-FS-only.
`pokeFsCursor()` scheduled `fsFadeRunnable` only
`if (fsView != null && browserMouse)`, the runnable
itself re-checked `fsView != null`, and `pokeFsCursor`
was only ever called from `showFullscreen` /
`handleFullscreenKey`. In pseudo-FS none of that ever
runs, so the regular browser cursor sat at alpha 1 over
the video forever.

Fix — the fade is generalized to "fullscreen-like" state:

- New `fsLikeActive()` = `fsView != null || pseudoFs`
  (raw field: while pseudo is active the ~750ms poll
  keeps it current, every pseudo exit path below cancels
  the fade explicitly, and the runnable re-checks
  `fsLikeActive()` at fire time regardless).
- `fsFadeRunnable` fades when
  `fsLikeActive() && browserMouse && cursorView != null`;
  `pokeFsCursor()` schedules the 10s fade under the same
  condition (restore-alpha-first behavior unchanged).
- Interaction resets in pseudo: `moveCursor()` now calls
  `pokeFsCursor()` on every move (also covers real-FS
  moves, which already poked via `handleFullscreenKey` —
  a duplicate poke just re-arms the timer), and the
  browser mouse-mode CENTER tap path pokes when
  `fsLikeActive()` (that's the tap that toggles play in
  pseudo-FS).
- Arming: the pseudofs ON transition (poll callback,
  next to `hideHeader()`) calls `pokeFsCursor()`, so the
  10s count starts at fullscreen entry even if the user
  never touches a key.
- Cleanup: the pseudofs OFF transition calls
  `cancelFsFade()` (restore alpha, kill the timer), and
  `closeBrowser()` now calls `cancelFsFade()` too — a
  pseudo-FS exit via Done never reaches `hideFullscreen`
  (`fsView` is null), so without it a faded cursor / live
  timer could leak out of the browser. (`hideFullscreen`
  already cancelled for the real-FS path.)
- Regression guard: normal browsing is untouched —
  `fsLikeActive()` is false, `pokeFsCursor()` schedules
  nothing and touches no alpha (its animate-cancel is
  still guarded by `cursorFaded`), so the cursor NEVER
  fades outside fullscreen-like state.
- The `alive` proof line now reads
  `alive version=1.17 code=18`.

## v1.17 verification (Oct 28, 2026)

- `bash build.sh`: BUILD OK (javac source/target 8,
  android.jar on `-cp`; d8 `--min-api 25`; zipalign;
  apksigner).
- Copied to `HeckYeah-1.17.apk`: **562,344 bytes**, sha256
  `a777c23fb2b2b97d6ac305175eb321282598259729a5337e19f398398d59ebe6`.
- `aapt2 dump badging`: `com.heckyeah.tv` versionCode
  **18** / versionName **1.17**, sdkVersion 25 /
  targetSdkVersion 25.
- `apksigner verify`: Verifies, 1 signer, cert SHA-256
  `220529f9…b92fab5` (unchanged — `adb install -r`
  preserves app data).
- `unzip -t`: no errors.
- Build-verified only — **NOT yet tested on the physical
  Stick, NOT uploaded.** On-device check: enter the same
  pseudo-fullscreen as v1.16 and don't touch anything —
  the cursor should fade ~10s after entry; any D-pad
  move or center tap should bring it back instantly and
  restart the 10s count; exiting fullscreen should leave
  the cursor solid; and in normal browsing the cursor
  must never fade.

## v1.16 (Oct 28, 2026) — app-start proof + pseudo-fullscreen detection/handling

Latest field evidence (video, Oct 28): the user taps the player's
fullscreen control; the browser chrome disappears, but the
video renders as a SMALLER CENTERED BOX that does not fill
the TV — and `adb logcat -d -s HeckYeahFS` STILL shows
nothing. Two live hypotheses: (H1) the running build is
pre-v1.14 (install unconfirmed), or the `-s` filter is
failing him (his reliable past workflow was
`logcat -d | grep <tag>`); (H2) this site's "fullscreen"
is PSEUDO-fullscreen inside the page — the player makes its
own container cover the viewport — so
`WebChromeClient.onShowCustomView` never fires: no FS
handler, no guard, no logs, and the page's own sizing
decides the box. v1.16 kills the H1 ambiguity forever and
detects/handles H2. Everything else (v1.15 clamps, real-FS
freeze/guard/logging, scrolllock ON/OFF logs) is unchanged.

**1) App-start proof of life:** `onCreate` now logs
`Log.d("HeckYeahFS", "alive version=1.16 code=17")` as its
first statement after `super.onCreate`. Every launch of
this build leaves exactly one unmistakable line; an empty
dump after launching the app can only mean this build is
not the one running (or the buffer was cleared after the
fact).

**2) Pseudo-FS detection** (the v1.15 ~750 ms poll is
extended; everything it already returned is kept). New
poll-JS fields, top document only for `fe`
(`fullscreenElement` is a top-document concept):

- `fe` — `document.fullscreenElement ||
  document.webkitFullscreenElement` ? its `tagName` : `""`.
- `vl/vt/vr/vb` — the winning (largest playing) video's
  rect in CSS VIEWPORT coords, i.e. the composed rect
  BEFORE the page-scroll offset is added (the v1.15
  `l/t/r/b` page rect is still returned too).
- `ps` — 1 when that viewport rect covers >= 90% of the
  viewport width AND >= 90% of the viewport height.

Java side: `pseudoFs = videoFound && fsView == null &&
ps == 1`, re-evaluated every poll, with `pseudoFsAt`
refreshed while it holds (fresh = < 2 s). Transitions log
under HeckYeahFS: `pseudofs ON rect=[l,t,r,b] fe=<tag|none>`
(rect = CSS viewport coords) and `pseudofs OFF`.
Separately, if `fe` is non-empty while `fsView == null`,
the poll logs ONCE per page (flag reset in
`onPageStarted`): `inpage-fs fe=<tag> (no custom view
handed to app)` — the smoking gun for a fullscreen the app
never received. `clearVideoData()` (navigation / real-FS
entry / Done) resets the pseudo state silently, mirroring
how it already resets `videoFound`.

**3) Pseudo-FS handling** while `pseudoFs` is active and
fresh:

- `doScrollBy` is a no-op — the same hard freeze as real
  fullscreen, sharing the early-return block at its head:
  the video already covers the viewport, so any page
  scroll can only shift the page under/around it.
- On the ON transition only, the browser header is forced
  out of the way through the existing header-hide path
  (`hideHeader()`), so the WebView gets the full window
  height — the pseudo overlay can only fill the WebView's
  own viewport, and a visible header shrinks exactly that.
  (Codebase note: `showHeader(false)` RESTORES/shows the
  bar in this code; `hideHeader()` is the hide path.) On
  OFF, nothing special — the v1.4 auto-hide logic resumes
  naturally.
- Deliberately NOT in this build: JS `requestFullscreen`
  promotion of the pseudo overlay into a real fullscreen.

**Expected HeckYeahFS lines with v1.16 installed** (this
is the H1 kill-shot — every case MUST produce lines):

- **App launch:** `alive version=1.16 code=17` (exactly
  once per launch).
- **Plain inline play:** `scrolllock ON rect=[l,t,r,b]
  viewport=<w>x<h> doc=<w>x<h>` when the poll first finds
  the playing video; `scrolllock OFF` when a later poll
  sees it stop (pause/end) while staying in the browser.
- **Real fullscreen tap:** the v1.14/v1.15 lines —
  `[attach] check … fixed=<n>` (plus any correction-detail
  line), `enter viewClass=… container=[…] view=[…]
  real=<WxH> window=<WxH> orientation=landscape
  scroll=<x>,<y>`, `[post]/[global]/[layout]/[focus]/
  [delay150]/[delay400]/[delay900] check …` summaries
  (identical clean `global` repeats are suppressed),
  `[jsprobe enter] …`, `[jsprobe delay400] …`,
  `[jsprobe delay900] …`, and `exit container=[…]
  view=[…]` on leaving fullscreen. No `pseudofs` lines:
  the poll does not run during a real FS session.
- **Pseudo fullscreen tap:** NO `enter`/`exit`/enforcer
  lines (no custom view is ever handed to the app).
  Instead: `pseudofs ON rect=[l,t,r,b] fe=<tag|none>` on
  the transition (`scrolllock ON …` too, if that same poll
  is also the first to find the video playing), plus
  `inpage-fs fe=<tag> (no custom view handed to app)` if
  the page actually set `fullscreenElement`; then
  `pseudofs OFF` (and `scrolllock OFF` if playback also
  stopped) when the overlay/video shrinks or stops.

## v1.16 verification (Oct 28, 2026)

- `bash build.sh`: BUILD OK (javac source/target 8,
  android.jar on `-cp`; d8 `--min-api 25`; zipalign;
  apksigner).
- Copied to `HeckYeah-1.16.apk`: **562,344 bytes**, sha256
  `9c37b48ee519bfa79e04bcd4aa07ae2eeaab028aa71226c97e8afa90708b9ea5`.
- `aapt2 dump badging`: `com.heckyeah.tv` versionCode
  **17** / versionName **1.16**, sdkVersion 25 /
  targetSdkVersion 25.
- `apksigner verify`: Verifies, 1 signer, cert SHA-256
  `220529f9…b92fab5` (unchanged — `adb install -r`
  preserves app data).
- `unzip -t`: no errors.
- Build-verified only — **NOT yet tested on the physical
  Stick, NOT uploaded.** On-device checks: (1) just launch
  the app and dump `HeckYeahFS` — the `alive` line must be
  there; if not, this build is not running, full stop.
  (2) Play a video inline: expect `scrolllock ON …`.
  (3) Tap the problem site's fullscreen control: if the
  dump gains `pseudofs ON …`, it was H2 (pseudo-FS) — the
  header should now hide and scrolling should freeze for
  as long as the video covers the viewport; if instead the
  `enter …` + check lines appear, it was a real fullscreen
  all along and the guard data applies.

## v1.15 (Oct 28, 2026) — video-bounded scroll area + fullscreen scroll freeze

On-device report on v1.13/v1.14: (1) `adb logcat -d -s
HeckYeahFS` STILL returned nothing (install/buffer-order question; this build's
logs make sessions provable either way, see D); (2) inline, a
playing video could still be scrolled around and moved
before fullscreen — v1.13's poll only queried the TOP
document, so players living in same-origin iframes were
never detected, and where detection DID fire the binary lock
was the wrong tool anyway; (3) in fullscreen the video
appeared to shift toward the lower left. His words: "The
browser when video is playing needs to better determine the
max scrollable area and have the full screen not move to the
lower left."

**A) Video-bounded scroll area** (replaces the v1.13 binary
lock's internals; the `video_scroll_lock` setting and the
once-per-page Toast concept carry over):

- `VIDEO_POLL_JS` now returns a JSON object (via
  `JSON.stringify`; parsed in Java with `org.json` after
  unwrapping the quoted string `evaluateJavascript` hands
  back). It scans the top document AND same-origin iframes
  recursively — per-frame try/catch, cross-origin frames
  skipped — accumulating iframe element rect offsets so the
  video rect is in TOP-page CSS px. A video counts as
  playing when `!paused && !ended && (readyState >= 2 ||
  currentTime > 0)`; size bar `width>=160 && height>=90`
  with a substantial viewport intersection (visible area
  >= 50% of the video's area, or >= 50% of the viewport);
  the LARGEST such video wins. The payload always carries
  `iw/ih` (viewport CSS size), `dw/dh` (document scroll
  size, max of `documentElement`/`scrollingElement`), and
  `sx/sy` (current scroll); with a video, its PAGE rect
  (client rect + window scroll at poll time).
- All scrolling is centralized in `doScrollBy(int, int)`.
  Every former `gateScroll()` + `webView.scrollBy/pageUp/
  pageDown` site (mouse keys, D-pad keys, PgUp/PgDn/FF/Rew,
  mouse edge transit, the v1.11 top-edge rule) now calls it;
  page keys use `doScrollBy(0, ∓(webView.getHeight() -
  dp(48)))` via `pageScrollStep()`. Grep proof: the only
  direct `webView.scrollBy` calls left in the file are the
  two inside `doScrollBy` itself, and the only direct
  `webView.scrollTo` is the fullscreen restore in (B).
  `gateScroll()`/`videoScrollLocked()` are gone.
- Clamp math (physical px deltas converted with
  `webView.getScale()`, targets computed in CSS px):
  document clamp ALWAYS (when poll data is < 2 s old) —
  target in `[0, max(0, docW-innerW)] x [0, max(0,
  docH-innerH)]` CSS px, i.e. the page's true max scrollable
  area rather than WebView overscroll slack. Video clamp
  ADDITIONALLY (video found + setting ON) — target CSS
  scroll must stay in `[vidR-innerW, vidL] x [vidB-innerH,
  vidT]` with endpoints normalized: for a video smaller
  than the viewport that is exactly the range keeping it
  fully in view; for one larger, the range where it still
  covers the viewport; the result is intersected with the
  document range. The first VIDEO-clamp truncation per page
  shows the v1.13 hint Toast once (`videoHintShown`, reset
  in `onPageStarted`). Cursor movement itself is untouched.
  One refinement in `moveCursor`: the top-transit scroll
  only marks `transitScrolledUp` if the page's scrollY
  actually changed, so a clamped-to-zero transit no longer
  suppresses the v1.11 top-edge rule on the same move.
- Stale/absent poll data: `doScrollBy` falls back to a
  plain unclamped `scrollBy` — pre-v1.15 behavior. Browsing
  never freezes because detection failed.

**B) Fullscreen scroll freeze + anchoring:**

- `showFullscreen` snapshots `fsSavedScrollX/Y` from the
  WebView BEFORE attaching anything; the v1.14 `enter` log
  line gains `scroll=<x>,<y>`.
- `enforceFullscreenGeometry` restores that scroll whenever
  it finds drift: `webView.scrollTo(saved, saved)`, counted
  as a correction (`fsPassFixCount++`) with detail
  `webscroll [x,y] -> [sx,sy]; ` in the HeckYeahFS line. A
  page scrolled under the fullscreen video (player JS is
  the usual suspect) is also a prime suspect for the video
  LOOKING shifted toward the lower left — the page moves,
  the surface compositing follows it.
- While `fsView != null`, `doScrollBy` is a no-op:
  fullscreen never scrolls the page through app input.
- Every place the enforcer (re)adds `fsContainer` to the
  decorView or resets its `LayoutParams` now uses
  `FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT)`
  with `gravity = Gravity.FILL`, and the params check flags
  non-FILL gravity as bad. (The custom view keeps its
  CENTER gravity; FILL is container-only.) The initial
  attach in `onCreate` uses FILL too, so the first enforcer
  pass doesn't spend a correction on it.
- `fsJsProbe("enter")` fires once right after attach, in
  addition to delay400/900 — the fullscreen element's rect
  at t=0 is now captured.
- `hideFullscreen` leaves the page scroll at the saved
  values (it never moved).

**C) Setting semantics unchanged:** `video_scroll_lock`
default ON; OFF disables ONLY the video clamp — the
document clamp still applies (that's just correct bounds).
Settings labels untouched.

**D) Provable logging:** poll detection flips log under
HeckYeahFS — `scrolllock ON rect=[l,t,r,b] viewport=<w>x<h>
doc=<w>x<h>` on false->true, `scrolllock OFF` on
true->false. Together with the v1.14 FS enter/check/probe/
exit lines, an empty dump from a session that browsed a
playing video and went fullscreen now unambiguously means
this build is not the one running.

## v1.15 verification (Oct 28, 2026)

- `bash build.sh`: BUILD OK (javac source/target 8,
  android.jar on `-cp`; d8 `--min-api 25`; zipalign;
  apksigner).
- Copied to `HeckYeah-1.15.apk`: **562,344 bytes**, sha256
  `9c0a83fdee65db367087f2fa5d3d4c08da345d3f24732c2936d09d3c6bd69998`.
- `aapt2 dump badging`: `com.heckyeah.tv` versionCode
  **16** / versionName **1.15**, sdkVersion 25 /
  targetSdkVersion 25.
- `apksigner verify`: Verifies, 1 signer, cert SHA-256
  `220529f9…b92fab5` (unchanged — `adb install -r`
  preserves app data).
- `unzip -t`: no errors.
- Scroll routing grep-verified (see above); zero leftover
  references to `videoLockActive`/`gateScroll`/
  `videoScrollLocked`.
- Build-verified only — **NOT yet tested on the physical
  Stick, NOT uploaded.** On-device checks: (1) play a video
  inline (ideally an iframe-embedded player) and push the
  mouse/D-pad — the page should stop at the exact point the
  video would start leaving the screen, in all four
  directions; pause it and full document scrolling returns
  within ~1 s. (2) `adb logcat -d -s HeckYeahFS` should now
  show `scrolllock ON …` for that session. (3) Go
  fullscreen: the video should stay anchored; the dump's
  `enter` line shows `scroll=<x>,<y>`, and any
  `webscroll [x,y] -> [sx,sy]` correction line names a
  drift the guard caught.

## v1.14 (Oct 28, 2026) — DIAGNOSTIC: HeckYeahFS speaks at every fullscreen checkpoint

A field test ran `adb logcat -d -s HeckYeahFS` after a fullscreen
session and got NOTHING. Code inspection confirmed why: the
whole app had exactly ONE `Log` call (inside
`enforceFullscreenGeometry`), fired ONLY when a geometry
deviation was actually corrected. Silence was therefore
ambiguous — guard satisfied vs guard never ran vs log buffer
rotated — so the v1.8 diagnostic design was too quiet. v1.14
changes NO behavior except logging; all of it is
`Log.d("HeckYeahFS", …)`:

1. **FS ENTER** (`showFullscreen`, right after the "attach"
   enforcer pass): `enter viewClass=<fqcn> container=[l,t,r,b]
   view=[l,t,r,b] real=<WxH> window=<WxH> orientation=landscape`
   — custom view class name, container/view laid bounds, real
   display metrics (`getRealMetrics`), decorView size, and the
   requested orientation. Bounds are as-laid at attach time;
   the later passes show the settled values.
2. **Every enforcer pass** now logs a summary line even when
   clean: `[<trigger>] check container=[l,t,r,b] view=[l,t,r,b]
   surface=<class|none> holderFixed=<WxH|none> fixed=<n>`,
   where `fixed` counts the corrections applied in that pass
   (new per-pass counter incremented at all 7 correction sites:
   3 container + 4 in `enforceFsTree`). When corrections WERE
   made, the pre-existing detail line (`[<trigger>] <view>
   bounds=[...] -> [0,0,W,H] full-bleed; ...`) still logs
   first, unchanged. Volume control: triggers are only the
   ones that already existed (attach, post, global, layout,
   focus, delay150/400/900) — no polling loop added. The
   chatty "global" trigger suppresses a CLEAN summary when it
   is identical to the previous global clean summary (the key
   resets on any correction, any other trigger's summary, and
   each new fullscreen session); correction lines always log.
3. **FS EXIT** (`hideFullscreen`, just before the custom view
   is detached): `exit container=[l,t,r,b] view=[l,t,r,b]`.
4. **JS VIDEO PROBE** at the +400 ms and +900 ms checkpoints
   (piggybacked on the existing `fsDelay400`/`fsDelay900`
   runnables — no new timers): `[jsprobe delay400] inner=<W>x<H>
   fs=<tag|none> rect=[l,t,r,b] video=<videoWidth>x<videoHeight>`
   — the largest `<video>` element's `getBoundingClientRect()`
   (CSS px), its intrinsic size, whether
   `document.fullscreenElement` exists and its tag, and
   `window.innerWidth/innerHeight`. `video=none` when the page
   has no `<video>`. Fully guarded try/catch; a failed or
   empty eval logs nothing.

Helpers added (all logging-only): `fsBounds(View)`,
`findFsSurface(View)`, `fsJsProbe(String)`, `FS_PROBE_JS`.
The ONLY behavioral delta vs v1.13 is the logging above —
geometry enforcement, scroll lock, and everything else are
unchanged in behavior.

## v1.14 verification (Oct 28, 2026)

- `bash build.sh`: BUILD OK (javac source/target 8, android.jar
  on `-cp`; d8 `--min-api 25`; zipalign; apksigner).
- Copied to `HeckYeah-1.14.apk`: **562,344 bytes**, sha256
  `97bc605911af62b609e89d610df3995204f5fdbe04c8f54960dc30c5526c2c7f`.
- `aapt2 dump badging`: `com.heckyeah.tv` versionCode **15** /
  versionName **1.14**, sdkVersion 25 / targetSdkVersion 25.
- `apksigner verify`: Verifies, 1 signer, cert SHA-256
  `220529f9…b92fab5` (unchanged — `adb install -r` preserves
  app data).
- `unzip -t`: no errors.
- Build-verified only — **NOT yet tested on the physical
  Stick.** On-device check: `adb logcat -c`, enter fullscreen
  on any video, exit, then `adb logcat -d -s HeckYeahFS`
  should now show the `enter` line, one `check` summary per
  checkpoint (plus detail lines for any corrections), the
  two `[jsprobe …]` lines, and the `exit` line — even for a
  perfectly behaved session.

## v1.13 (Oct 26, 2026) — scroll lock while a video plays inline

On-device report: with a video playing INLINE (before
the user presses the player's fullscreen button), the page is still
scrollable — moving the mouse toward the player's controls
(e.g. down toward the fullscreen button) edge-scrolls the
page and the video slides away; D-pad scrolling does the
same. On this device EVERY page scroll is app-driven (remote
only — no touch, no wheel events ever reach the page), so
the app can simply refuse to scroll while a video is playing.

**Detection:** while the browser is the current screen and
NOT in HTML5 fullscreen, a Handler loop polls every ~750 ms
via `evaluateJavascript` (kicked from `showScreen()` when the
browser pane shows; the loop self-terminates at its next
cycle once another screen is current, so Done never leaks
it). The poll is skipped while a browser dialog is open
(`browserDialogDepth > 0`) or the browser is suspended, and
fullscreen forces the state off. The script returns "1" if
ANY `<video>` is `!paused && !ended && readyState > 2` with
a `getBoundingClientRect()` of at least 200×100 substantially
inside the viewport (the size bar means small/hidden
autoplay widgets don't count; a big visible playing ad DOES
count, briefly, by design — pausing/ending unlocks within
one poll cycle). State resets in `onPageStarted`, on Done,
and on entering fullscreen.

**Gating:** every app-initiated page scroll passes one
helper, `gateScroll()`: locked = setting ON (prefs
`video_scroll_lock`, default ON) AND `videoLockActive` fresh
(< 2 s). Gated call sites (grep-verified complete — these
are ALL the `webView.scrollBy/pageUp/pageDown` sites in the
file): mouse-mode edge scrolling in `moveCursor()` (all
four edges incl. the v1.11 top-edge rule), mouse-mode
PAGE_UP/PAGE_DOWN + FF/REW keys, D-pad-mode arrow scrolling
(all four directions), and D-pad-mode PAGE_UP/PAGE_DOWN +
FF/REW keys. Cursor MOVEMENT is never gated — only scroll
actions. A refused scroll shows a Toast — "Video playing —
scrolling locked (pause to scroll)" — at most once per
navigation (the shown flag resets in `onPageStarted`);
further refusals are silent. Non-goals kept: the page's own
JS/internal scrolling is untouched, and nothing is gated in
HTML5 fullscreen.

**Setting:** "Video scroll lock: ON/OFF" in the gear sheet
(⚙ Browser Settings, below Cursor speed) AND "Lock
scrolling while a video plays (browser)" in the main
Settings browser section — both write the same prefs key
and take effect immediately, no reload.

## v1.13 verification (Oct 26, 2026)

- `bash build.sh`: BUILD OK (javac source/target 8, android.jar
  on `-cp`; d8 `--min-api 25`; zipalign; apksigner).
- Copied to `HeckYeah-1.13.apk`: **558,248 bytes**, sha256
  `24a507e527ac39e41f0d08bc6cd9c900ada348ba95a199cca2e5551355c84733`.
- `aapt2 dump badging`: `com.heckyeah.tv` versionCode **14** /
  versionName **1.13**, sdkVersion 25 / targetSdkVersion 25.
- `apksigner verify`: Verifies, 1 signer, cert SHA-256
  `220529f9…b92fab5` (unchanged — `adb install -r` preserves
  app data).
- `unzip -t`: no errors.
- Build-verified only — **NOT yet tested on the physical
  Stick.** On-device check: play a video inline (not
  fullscreen) and try to scroll with mouse edges and D-pad —
  the page should hold still and show the hint once; pause
  the video → scrolling frees within ~a second; go
  fullscreen → no lock at all. Toggle it off in ⚙ Browser
  Settings → scrolling is never gated.

## v1.12 (Oct 26, 2026) — RW screenshots use the resume mechanism; stale pre-v1.10 shots purged

Micro-build from a user question: "Can the screenshots of the
Recently watched be doing the same screenshot mechanism as the
resume preview?"

**Part 1 — parity (verified in source, no change needed):**
both screenshot consumers already capture exclusively through
the shared strict helper from v1.10 — `captureResumeShot()`
and `captureRwShot()` each call `captureViewportBitmap()`
(bitmap at the WebView's exact on-screen size,
`webView.draw()` on an untransformed canvas, downscale only
afterwards) and neither carries any draw/scale logic of its
own. A grep for draw/scale/canvas operations across the file
finds exactly one `webView.draw(...)`, inside the helper
itself. So RW tile screenshots and the resume preview have
been the same mechanism since v1.10 — identical framing, the
exact visible viewport at the current scroll position.

**Part 2 — purge the stale pre-v1.10 shots:** the old
(squeezed-capture) image files persist in app storage across
`adb install -r` updates, so tiles would keep DISPLAYING old
bad shots even though every new capture is correct. New
one-time migration `migrateShotsIfNeeded()` (modeled on the
v1.8 `enr_ver` migration, called in `onCreate` right after
`loadRw()` / `migrateEnrichmentIfNeeded()`): a new prefs key
`shot_ver` (current **2**); when below 2 it deletes every
file in `files/rw_shots/`, deletes `files/resume_shot.jpg`,
clears both in-memory decoded-shot caches (`shotCache`, and
the resume preview's stamp cache via `deleteResumeShot()`),
and stamps `shot_ver = 2`. RW records keep their shot
filename field — the files are simply gone, and both loaders
already treat a missing file as "no screenshot" (audited:
the RW tile loader's `decodeFile` returns null → shot view
stays GONE, poster shows instead when present; the resume
preview loader gates on `isFile()` and the expansion only
runs when a bitmap exists). No poster/enrichment data is
touched. Fresh shots re-capture by themselves through the
existing triggers (RW: next Done / debounced page capture;
resume: next Done).

## v1.12 verification (Oct 26, 2026)

- `bash build.sh`: BUILD OK (javac source/target 8, android.jar
  on `-cp`; d8 `--min-api 25`; zipalign; apksigner).
- Copied to `HeckYeah-1.12.apk`: **558,248 bytes**, sha256
  `9e2001c3e63b6548831d2f48a143fec973b841c5c955c5b77d49358ef6d94431`.
- `aapt2 dump badging`: `com.heckyeah.tv` versionCode **13** /
  versionName **1.12**, sdkVersion 25 / targetSdkVersion 25.
- `apksigner verify`: Verifies, 1 signer, cert SHA-256
  `220529f9…b92fab5` (unchanged — `adb install -r` preserves
  app data).
- `unzip -t`: no errors.
- Build-verified only — **NOT yet tested on the physical
  Stick.** On-device check: after installing, existing RW
  tiles should show poster-or-nothing (old screenshots gone);
  watch something and hit Done → the RW tile and the resume
  preview should show fresh viewport-true shots.

## v1.11 (Oct 26, 2026) — top-edge push reveals the bar AND keeps scrolling

Single-issue fix from the v1.10 on-device test, in the
browser's **mouse mode** with the auto-hiding header: pushing
the cursor to the top of the screen revealed the nav bar (the
v1.4 behavior) but the page **stopped scrolling up** — "it
reveals the browser bar but does not scroll to the top of the
site when I am hitting the top of the screen. It needs to
scroll as well."

**Cause:** mouse-mode edge scrolling is driven per key-repeat
inside `moveCursor()`: holding UP fires repeated moves, and
the scroll-up only happens inside a check that requires the
cursor to be **inside the WebView's window rect** while it
crosses the WebView's top edge (`cursorY >= loc[1]` and
`cursorY + dy < loc[1]`). With the header hidden, the WebView
fills the screen, its top edge is y=0, and scrolling works —
until the cursor pins at y=0 and the v1.4 reveal block calls
`showHeader()`. The header taking its space back drops the
WebView's top edge to header height, so the cursor — still
parked at screen y=0, now over the bar — fails the
inside-the-page test on every subsequent repeat, and no
scroll ever fires again. The reveal didn't cancel a timer
(there is none — scrolling is per-repeat-event);
`showHeader()`/`hideHeader()` never touched the scroll
path. The reveal simply invalidated the geometry the
scroll gate depended on. The bottom edge never had this
problem because the WebView reaches the screen bottom in
both header states; the top is the only edge whose page
boundary moves when the header toggles.

**Fix (`moveCursor()` only):** after the cursor is clamped,
an UP push on the browser now also scrolls when the cursor
is **pinned to the screen top** (`cursorY <= 0`) or **parked
above the page's top edge** (over the revealed bar), unless
the in-page crossing check already scrolled on that same
move (no double scroll on the crossing step). The reveal
itself is unchanged and fires first in the same move, so
one continuous hold now does both in sequence: pin at the
top → bar reveals → page keeps scrolling up on every
further repeat until `scrollY == 0` (`scrollBy` no-ops at
the top). Because the new rule is keyed to the cursor
rather than page geometry, the header's show animation /
layout change cannot disarm it mid-hold, and it works
identically when the header was already visible. Stop
conditions mirror the existing edge-scroll exactly — it is
the same `moveCursor()` entry point: leaving the edge,
leaving mouse mode, a browser dialog opening, Done, and
fullscreen (`fsView != null`) all skip it as before. The
auto-hide hysteresis is untouched — scrolling up via the
edge naturally keeps the bar shown. Bottom/left/right
edge-push behavior and the D-pad sub-mode are unchanged.

## v1.11 verification (Oct 26, 2026)

- `bash build.sh`: BUILD OK (javac source/target 8, android.jar
  on `-cp`; d8 `--min-api 25`; zipalign; apksigner).
- Copied to `HeckYeah-1.11.apk`: **558,248 bytes** (same byte
  count as v1.9/v1.10 by coincidence; contents differ), sha256
  `1ef8bdb90df07c89fcb0e4d3f1666e3c2ef05ad3f19eeed89d68c62acb4c8804`.
- `aapt2 dump badging`: `com.heckyeah.tv` versionCode **12** /
  versionName **1.11**, sdkVersion 25 / targetSdkVersion 25,
  LAUNCHER + LEANBACK_LAUNCHER, banner present.
- `apksigner verify`: Verifies, 1 signer, cert SHA-256
  `220529f9…b92fab5` (unchanged — `adb install -r` preserves
  app data).
- `unzip -t`: no errors.
- Build-verified only — **NOT yet tested on the physical
  Stick.** On-device check: in mouse mode, scroll down a long
  page until the bar auto-hides, then push the cursor to the
  top edge and keep holding — the bar should reappear and
  the page should keep scrolling up to the very top without
  lifting off or re-pushing; repeat with the bar already
  visible (park the cursor over the bar and hold UP) — same
  continuous scroll to the top.

## v1.10 (Oct 26, 2026) — viewport-true screenshots + Clear card to the far right

Two corrections from the v1.9 on-device test (with TV photo
of the resume preview). Everything else from v1.9 is unchanged.

### 1. Screenshots now capture the exact visible viewport

The resume hover preview showed a capture that was "a little
bugged": content from well past the visible area squeezed into
the frame — it did not match what was on screen. **Cause:**
both capture paths (the resume shot in `captureResumeShot()`
and the RW shots in `captureRwShot()`) created the bitmap at
the already-shrunk storage size and called `cv.scale()` on the
canvas *before* `webView.draw()`, so the WebView rasterized
its document through that shrink transform instead of the app
copying the visible viewport at native size — a re-render
squeezed through the transform, not a faithful copy of the
screen, framing included.

**Fix:** one shared strict helper, `captureViewportBitmap()`:

- The bitmap is the WebView's **exact on-screen pixel size**
  (`getWidth()` × `getHeight()` at capture time); a zero-size
  view skips the capture (the resume path keeps its
  delete-the-shot behavior for that case).
- `webView.draw()` runs on an **untransformed canvas** — no
  scale, no translate, no content-height sizing, no scrolling
  the page, no measure/layout tricks — so the bitmap is the
  visible region at its current scroll position, centered
  exactly as the user saw it.
- Only *after* capture is the finished bitmap resampled
  (`createScaledBitmap` via `downscaleToWidth()`, aspect
  preserved, never upscaled) to the storage widths — 640px
  for the resume shot, 480px for RW shots — where a resample
  cannot change the framing.

Capture timing is unchanged (resume shot at the top of
`closeBrowser()` while the browser is still visible; RW shots
at their usual Done/debounced points), and the honest limit is
unchanged: a last screen of protected/fullscreen video can
still capture black, because those surfaces never composite
into a view draw at any size.

### 2. Clear Recently Watched card moved to the far right

Correction to the v1.9 placement: the ✕ Clear card
sits at the **end** of the RW shelf, not the start. In
`buildHome()` the card is now added to the shelf row **after**
the entry loop, so the order is **[entry1 … entryN, Clear]**.
Nothing else about the card changes — same look, same
confirmation dialog, same factored `clearAllRw()` + Home
rebuild on confirm.

Focus consequences, verified in code: `firstRwTile` is still
captured from the entry loop, so Search DOWN still lands on
the first (leftmost) content entry (the v1.6 contract); a
native RIGHT-walk now traverses all entries and reaches Clear
**last**; the card keeps its `setNextFocusUpId(bSearch)`
symmetry; nothing else in the app referenced its old position.

## v1.10 verification (Oct 26, 2026)

- `bash build.sh`: BUILD OK (javac source/target 8, android.jar
  on `-cp`; d8 `--min-api 25`; zipalign; apksigner).
- Copied to `HeckYeah-1.10.apk`: **558,248 bytes** (same byte
  count as v1.9 by coincidence; contents differ), sha256
  `cc0fe7bfb743c1310c31c0ed45633b46a5903d04d4b604d4d15ff82ce71da7a0`.
- `aapt2 dump badging`: `com.heckyeah.tv` versionCode **11** /
  versionName **1.10**, sdkVersion 25 / targetSdkVersion 25,
  LAUNCHER + LEANBACK_LAUNCHER, banner present.
- `apksigner verify`: Verifies, 1 signer, cert SHA-256
  `220529f9…b92fab5` (unchanged — `adb install -r` preserves
  app data).
- `unzip -t`: no errors.
- Build-verified only — **NOT yet tested on the physical
  Stick.** On-device checks: browse partway down a page, hit
  Done, hover the Back-to bar — the preview should be a
  faithful miniature of exactly what was on screen (same
  scroll position, nothing squeezed in from further down);
  the RW shelf should read entries first with the ✕ Clear
  card last on the right, reached by walking RIGHT past the
  final entry.

## v1.9 (Oct 26, 2026) — RW Clear card + resume hover preview

Two requested items; everything else from v1.8 is
unchanged.

### 1. "Clear" card at the left of the Recently Watched shelf

When the RW shelf is built on Home (rw list non-empty), the
FIRST tile in the shelf row — before the most-recent entry —
is a utility card: a large **✕** glyph (the text glyph in the
app's Inter fonts, not an emoji) in iOS red over a two-line
"Clear Recently Watched" label. It carries the same tile
chrome and focus behavior as an RW tile (focus border,
`focusIntoView()` scroll-into-view, translationZ lift, 1.03
scale) but a dim red-tinted fill (`#241417`, red-tinted
border) so it reads as a utility action, not content. It is
narrower than a content tile (170dp vs 300dp) with a 96dp
minimum height and vertical centering in the row to sit level
with the entries.

- **Click → confirmation dialog** ("Clear Recently Watched?" /
  "This removes all N entries, their screenshots and poster
  art." / **Clear** | **Cancel**), built with the app's dialog
  pattern (`trackDialog`). On confirm it runs the exact same
  wipe as Settings → "Clear Recently Watched": that lambda and
  this card now both call one factored `clearAllRw()` (entries
  + `rw_shots` files + poster files + enrichment sidecars +
  caches), then Home rebuilds — the shelf and the card vanish.
- **Focus wiring (the v1.6 contract survives):** the Search
  button's `nextFocusDownId` still points at the first
  `rwTile()` — the first content *entry*, NOT the Clear card —
  because `firstRwTile` is captured from the entry loop exactly
  as before. The card joins native Left/Right inside the row:
  LEFT from the first entry reaches it, RIGHT from it reaches
  the first entry; it also gets `setNextFocusUpId(bSearch)`
  for symmetry with the first entry. After a clear, the shelf
  is empty, so Search DOWN falls back to the first category
  tile, as wired in v1.6.

### 2. Resume ("‹ Back to …") hover preview

When the Back-to bar at the top of Home / category / search
gains focus, the **last screenshot of the page it will resume
to** expands open underneath it; when focus leaves, it
collapses. Poster art is deliberately NOT used here — the
request was for the last screen itself.

- **Capture (any browser session, not just video):** resume is
  browser-wide while RW shots are video-category-only, so the
  preview has its own file, `files/resume_shot.jpg`. At the
  top of `closeBrowser()` — the same Done moment whose
  finalize saves the new resume target via `updateResume()`,
  and while the browser is still on screen (the finalized
  path hides the pane, and an invisible WebView may draw
  blank) — the current WebView viewport is drawn with the
  same machinery as the RW shots, downscaled to ~640px wide,
  JPEG 75, overwriting the file every time. No current page
  (blank WebView) = no capture; the previous shot still pairs
  with the unchanged target. **Stale-pairing guard:** whenever
  `openEntry` / `openRw` replaces the resume target with a
  *different* URL, `resume_shot.jpg` is deleted on the spot —
  the fresh shot arrives at that session's Done — and
  Settings → "Clear saved browser page" deletes it too.
- **Display + animation:** `resumeBar()` now returns a
  vertical block **[bar] + [hint] + [preview area]**. The
  preview is an `ImageView` (480×270dp max, CENTER_CROP)
  starting GONE at height 0, loading `resume_shot.jpg` when it
  exists (decoded once, cached by file stamp). The bar's
  existing focus listener (focusIntoView + restyle + scale —
  extended, not replaced) drives it: focus gain → VISIBLE and
  a `ValueAnimator` grows the preview's `LayoutParams.height`
  0→270dp over ~200ms with a DecelerateInterpolator, alpha
  tracking the height fraction; focus loss → the reverse over
  ~150ms, then GONE. **Why height, not scaleY:** a
  scale/alpha-only reveal on a VISIBLE view leaves a
  full-height hole in the scroll content (rows below never
  move), while a GONE→VISIBLE height animation makes the
  content below genuinely shift down and back up — the
  expand/collapse feel — and composes cleanly with the parent
  ScrollViews. Rapid focus in/out cannot stick it: each
  transition bumps a generation counter, cancels the running
  animator, starts from the current height, and stale
  animators' end-actions check the generation before touching
  final state.
- **Scroll interaction:** none needed, by design. The bar is
  pinned at the very top of every screen it appears on and
  `focusIntoView()` on the bar runs at the same focus gain, so
  the preview unfolds downward into already-visible space and
  pushes the content below it down; the preview block's own
  container joins the v1.4 unclip chain so nothing clips the
  growing image. When focus moves on (collapsing), the target
  view was already chosen by the focus system, and its own
  focusIntoView scrolls it into place after the layout
  settles.
- **No shot = v1.8 behavior:** a resume target saved before
  v1.9, or a failed capture, simply never expands. Same
  honest limit as the RW shots: if the last screen was
  fullscreen video, the preview may be black.

## v1.9 verification (Oct 26, 2026)

- `bash build.sh`: BUILD OK (javac source/target 8, android.jar
  on `-cp`; d8 `--min-api 25`; zipalign; apksigner).
- Copied to `HeckYeah-1.9.apk`: **558,248 bytes**, sha256
  `309ba72f7c9e2d3bddaf511ad633be2feceb7db5b147cce9193a48fc1ff309bd`.
- `aapt2 dump badging`: `com.heckyeah.tv` versionCode **10** /
  versionName **1.9**, sdkVersion 25 / targetSdkVersion 25,
  LAUNCHER + LEANBACK_LAUNCHER, banner present.
- `apksigner verify`: Verifies, 1 signer, cert SHA-256
  `220529f9…b92fab5` (unchanged — `adb install -r` preserves
  app data).
- `unzip -t`: no errors.
- Build-verified only — **NOT yet tested on the physical
  Stick.** On-device checks: the RW shelf opens with the ✕
  Clear card at its left edge; Search → DOWN still lands on
  the first RW *entry* (LEFT then reaches Clear); Clear asks
  for confirmation and empties the shelf; after any browser
  session + Done, hovering the Back-to bar expands the last
  screen's screenshot and moving focus away collapses it.

## v1.8 (Oct 26, 2026) — scrollable Settings, poster matching v2, fullscreen geometry guard

Three items from the v1.6/v1.7 on-device tests.

### 1. Settings page is scrollable (bug fix)

In v1.7 the Settings screen was a fixed `LinearLayout` pane: the
rows (browser mode, scale, appearance, ad blocking, cursor
speed, OMDb key + note, Update, status, Clear RW / saved page /
Favorites / cache, data-source note) ran past the bottom of the
TV and the lower rows could not be reached or selected. The
Settings column now lives inside a vertical `ScrollView` built
exactly like the other menu screens — an `unclip()`'ed
`ScrollView` wrapping a `pageColumn()` (same 40dp side / 18dp
top / 28dp bottom page padding, clipChildren/clipToPadding off
on the chain) — with the header (‹ Home + "Settings") inside the
scroll area, matching the Home/Category pattern. Every row is a
`safariBtn` using the shared focus styler, so focus gain runs
`focusIntoView()` and scrolls the row fully into view, down to
the last row. Entering Settings posts a focus request to the
first row's control (browser control mode); the header is one UP
away; Back exits to Home as before.

### 2. Poster matching v2 (chain rework)

v1.7's fuzzy iTunes/TVmaze scoring produced a false positive:
the "Clayface" RW entry (an upcoming movie with thin catalog
coverage) got artwork from an animated Batman series, because a
candidate whose name merely *contained* or *started with* the
title could win. The chain is now, in order:

1. **OMDb (key only) — unchanged.** Exact `&t=`/`&y=` lookups
   plus the `&s=` + `&i=` search fallback; still the only source
   of IMDb/RT scores.
2. **Wikipedia page image (new, no key).**
   `GET en.wikipedia.org/w/api.php?action=query&generator=search&gsrnamespace=0&gsrlimit=6&prop=pageimages&piprop=original|thumbnail&pithumbsize=600&format=json&gsrsearch=<q>`.
   Primary query: `intitle:"<T>" (film|series) <YEAR?>`; if it
   yields no accepted page, fallbacks `<T> film <YEAR>` and
   `<T> TV series`. A page is accepted ONLY when its title is
   exactly the cleaned title, `<T> (film)`, `<T> (<YEAR> film)`,
   `<T> (TV series)`, or `<T> (<...> TV series)`
   (normalize-compared); the first accepted page (by search
   rank) with a page image wins — `original.source`, else the
   thumbnail. Wikipedia film/series article images are the
   official poster in the overwhelming majority of cases.
3. **DuckDuckGo image search (new, no key; requested
   approach).** Unofficial two-step token flow, desktop browser
   User-Agent on every request: (1)
   `GET duckduckgo.com/?q=<query>&iax=images&ia=images`, vqd
   token extracted from the HTML by regex `vqd[="':]+([0-9-]+)`;
   (2) `GET i.duckduckgo.com/i.js?l=us-en&o=json&q=<query>&vqd=<token>&f=,,,&p=1`
   → JSON `results[]` with `image`, `width`, `height`, `title`.
   Queries in order: `"<T>" <YEAR> movie poster`, `<T> <YEAR>
   movie poster`, `<T> official poster`, `<T> poster`. A
   candidate must pass a declared-dimension pre-filter (portrait
   w/h 0.55–0.85, width ≥ 250) and its result title
   (normalized) must contain the cleaned title; candidates are
   then downloaded (max 6 across all queries) and must pass on
   their ACTUAL decoded dimensions (portrait 0.55–0.85, width
   ≥ 250). First passing image wins. Best-effort only: DDG can
   bot-challenge or change this endpoint at any time; every
   failure degrades silently to the next step.
4. **iTunes + TVmaze — demoted to last resort, STRICT.**
   Exact normalized-title equality only (the v1.7 contains /
   startsWith wins are gone — that scoring is what let the
   Batman series in); a known year off by more than 1 is a
   reject; iTunes `kind == "tv-episode"` results are rejects.
   Nothing passing = `miss` → the tile shows its screenshot.

- **Migration (`enr_ver` = 2):** posters fetched by the v1
  matcher may be wrong on devices already running v1.7, so the
  first launch under v1.8 clears every stored poster file +
  lookup state (poster/state/ts) and each current RW entry is
  re-looked-up with the new chain. Stored imdb/rt values are
  kept — they only ever came from OMDb exact lookups.
- **Manual correction:** the RW tile long-press dialog is now a
  choice list — **↻ Re-fetch poster** (deletes the entry's
  poster + sidecar and kicks a fresh lookup; Home rebuilds when
  it lands, per the existing completion rules), **✕ Remove
  poster art** (deletes the poster file and sets sidecar
  `poster_blocked=1`; auto-lookup — including the
  OMDb-key-saved forced pass — skips blocked entries until a
  Re-fetch clears the flag; the tile falls back to its
  screenshot), **Remove entry** and **Cancel**, as before.

*Clayface walkthrough (no year extracted, upcoming film):*
OMDb is skipped without a key (with a key, its exact lookups
either find the real film or nothing). Wikipedia actually HAS a
"Clayface (film)" article and it passes the title rule — but as
of this build the article exposes no page image through the
API, so the chain moves on. DDG then searches `"Clayface" movie
poster`: only results whose titles name Clayface and whose
downloaded bytes are true portrait posters qualify — an
animated-series key art titled "Batman: The Animated Series"
fails the title-contains test, and series posters naming
Clayface fail portrait/title checks in practice; if DDG is
challenged or nothing qualifies, strict iTunes/TVmaze require
the candidate name to be exactly "Clayface", so the Batman
artwork that won under v1.7's loose scoring is now a reject by
construction — worst case is `miss` → screenshot, never the
wrong poster. And if a wrong image ever does slip through,
Re-fetch / Remove poster art fix it from the tile itself.

### 3. Fullscreen geometry guard (continuous, not one-shot)

Fullscreen video still occasionally shifted off-center after
v1.4's fix. The one-shot pass could be undone when Chromium (or
the player page) re-laid-out the custom view / reset the
surface afterward. v1.8 enforces the geometry continuously
while fullscreen is active; every trigger funnels into ONE
idempotent `enforceFullscreenGeometry()` (a re-entrancy flag
prevents listener loops; it touches nothing when geometry
already conforms):

- **Triggers:** entry ("attach" + a posted "post"), an
  `OnGlobalLayoutListener` on the fullscreen container
  ("global"), an `OnLayoutChangeListener` on the custom view
  itself ("layout"), window-focus regain ("focus"), and
  scheduled re-asserts at +150/+400/+900 ms after entry
  ("delay150/400/900") for slow immersive transitions.
  Listeners and pending runnables are removed on exit — nothing
  leaks into the next session.
- **Each pass asserts:** the container is a MATCH_PARENT child
  of the decorView above the app content with no margins or
  transforms (the cursor overlay stays above it); the custom
  view has fresh `FrameLayout.LayoutParams(MATCH_PARENT,
  MATCH_PARENT, gravity CENTER)`, zero margins/padding and
  reset translation/rotation/scale, and — if its laid bounds
  still differ from the container's — is laid out directly at
  (0, 0, containerW, containerH). RECURSIVELY over all
  descendants: ViewGroup children get MATCH_PARENT params (of
  their own params' runtime type, gravity CENTER for
  FrameLayout params) with zero margins; every SurfaceView
  additionally gets `getHolder().setFixedSize(realW, realH)`
  from `getRealMetrics` (re-asserted only when the view/size
  changed, so the surface is not churned); every TextureView
  gets MATCH_PARENT.
- **Orientation is locked** to landscape on entry
  (`SCREEN_ORIENTATION_LANDSCAPE`) and the previous requested
  orientation is restored on exit — portrait-frame layout races
  are gone.
- **Order fix:** the window flags + immersive system UI are now
  applied BEFORE the custom view is attached, so its first
  layout pass happens at the final window size (v1.4 attached
  first, flagged after).
- **Diagnostics:** a pass that actually CORRECTS a deviation
  logs `Log.d("HeckYeahFS", "[<trigger>] <view> bounds=[...]
  params=... -> [0,0,W,H] full-bleed; ...")` — nothing is logged
  when geometry is already correct. If a shifted screenshot
  ever appears again, pair it with `adb logcat -s HeckYeahFS`.

## v1.8 verification (Oct 26, 2026)

- `bash build.sh`: BUILD OK (javac source/target 8, android.jar
  on `-cp`; d8 `--min-api 25`; zipalign; apksigner).
- Copied to `HeckYeah-1.8.apk`: **554,152 bytes**, sha256
  `27604996b71050b6d64db9c7bbfd39e933acba82d613551443929b28cc6eca29`.
- `aapt2 dump badging`: `com.heckyeah.tv` versionCode **9** /
  versionName **1.8**, sdkVersion 25 / targetSdkVersion 25,
  LAUNCHER + LEANBACK_LAUNCHER, banner present.
- `apksigner verify`: Verifies, 1 signer, cert SHA-256
  `220529f9…b92fab5` (unchanged — `adb install -r` preserves
  app data).
- `unzip -t`: no errors.
- Endpoint checks from the build sandbox: the Wikipedia
  generator=search query returns "Clayface (film)" as an
  accepted title (it currently exposes no page image, so the
  chain falls through as designed); the DDG html page yields a
  vqd token with the shipped regex (the sandbox's datacenter IP
  gets a bot challenge on the i.js step — the flow is
  best-effort and degrades silently; a residential Stick
  connection is the real test). The Wikipedia acceptance rule
  and the vqd regex were additionally unit-checked on a desktop
  JVM against the shipped logic (10 title cases + token
  extraction — all as designed).
- Build-verified only — **NOT yet tested on the physical
  Stick.** On-device checks: Settings scrolls to the last row
  and every row is selectable; the Clayface RW tile re-fetches
  under chain v2 on first Home build (wrong Batman art is wiped
  by the migration); fullscreen stays centered, and any
  correction shows in `adb logcat -s HeckYeahFS`.

## v1.7 (Oct 26, 2026) — RW posters + IMDb/RT scores replace the (usually black) screenshots

Field report on v1.3–v1.6: the Recently Watched screenshot is black most
of the time (the known protected-surface limit), so detect the
movie/show from the RW title, pull a real poster from an online
source, and show IMDb + Rotten Tomatoes scores when possible.
The screenshot stays as the fallback when no poster is found.

- **Title cleaning** — `cleanTitle(pageTitle, url)`: cut at the
  first of ` | ` / ` – ` / ` — ` / ` - `; strip leading junk words
  repeatedly (`Watch`, `Watch Free`, `Stream`, `Watch Online`);
  remove junk phrases anywhere (`Full Movie`, `Full Episode`,
  `Online Free`, `Watch Online`, `Free Online`, `HD`,
  `Movie Online`); extract a year (first `(YYYY)` group, else
  first bare `YYYY` token — extracted and removed); cut at
  episode markers (`Season N`, `S01E01`-style, `Episode N`). If
  <2 chars remain, no lookup. Examples, verified on the JVM
  against the shipped code: `"Project Hail Mary | Movy"` →
  title `Project Hail Mary`, year null;
  `"Watch Project Hail Mary (2026) Full Movie Online Free |
  Fmovies"` → `Project Hail Mary`, 2026;
  `"The Pitt Season 2 Episode 5 | Movy"` → `The Pitt`.
- **Sidecar state (RW record format unchanged)** — prefs keys
  `enr_<urlhash>_{poster,imdb,rt,state,ts}` where `<urlhash>` is
  the same hex hash used for `rw_shots` filenames; poster bytes
  at `files/posters/<urlhash>.jpg`. Entry removal (tile
  long-press), Clear Recently Watched, and cap-12 eviction
  delete the poster file + sidecar keys, next to the existing
  screenshot deletion; if a live session's URL changes to a
  different page, the old URL's enrichment is deleted so a
  poster can never follow the session to the wrong page.
- **Lookup chain** (background thread, one in flight per entry,
  8 s connect/read timeouts, every failure degrades silently):
  1. **OMDb — only when a key is configured:**
     `GET omdbapi.com/?apikey=<key>&t=<title>[&y=<year>]`; on
     `Response=False` retry without the year, then `&s=<title>`
     search — take the result whose Title normalizes closest
     (strip non-alnum, case-insensitive; exact > startsWith >
     contains) and fetch the full record by `&i=<imdbID>`
     (search hits carry no ratings). From a hit: `Poster` (skip
     `N/A`), `imdbRating`, and the `Ratings[]` entry with
     Source `Rotten Tomatoes`.
  2. **iTunes Search API (no key):**
     `itunes.apple.com/search?term=<title>&media=movie&limit=8`,
     then `&media=tvShow&limit=8`. Candidates scored by the same
     normalized title match, preferring release year within 1
     of the extracted year; `artworkUrl100` is upgraded
     `100x100bb.jpg` → `600x600bb.jpg` and downloaded.
  3. **TVmaze (no key, shows):**
     `api.tvmaze.com/search/shows?q=<title>` — best name match
     with `show.image != null`, prefer `image.original` over
     `image.medium`; downloaded.
  A downloaded poster must decode as a real image or it is
  rejected. If OMDb produced ratings but no poster, the ratings
  are still stored and the poster chain continues. End state is
  `ok` when a poster and/or any score was found, else `miss`
  (timestamped; a `miss` retries only after 24 h, or immediately
  when a key is newly saved).
- **Triggers** — (i) the Done snapshot path (entry created /
  updated: URL + title are final there), and (ii) every Home
  build, for entries with no state or a stale miss — throttled
  by the in-flight set so a build kicks at most the missing
  ones, once. When a lookup finishes and Home is showing, Home
  rebuilds so the poster appears — unless focus is currently
  inside the RW shelf, in which case the rebuild is skipped
  (the focused tile reads the poster file on its next focus
  gain; nothing is yanked out from under the user).
- **Display (focused RW tile)** — with a poster on disk, the
  focus expansion is a horizontal row instead of the
  screenshot: poster (92×138dp, CENTER_CROP) at left; right
  column keeps the header title/link lines above it and adds
  `IMDb ★ <rating>` and `Rotten Tomatoes 🍅 <value>` lines
  under it, each omitted when absent. Without a poster, the
  v1.3 screenshot expansion is byte-for-byte the old behavior,
  and screenshots keep being captured exactly as before.
- **Settings** — new row "OMDb API key — posters + IMDb/RT
  scores", button reads **Not set** / **Key saved**. Tapping
  opens a text-input dialog (prefilled with the current key,
  blank removes it) whose hint/subtext reads: "Free key at
  omdbapi.com (1,000 lookups/day). Without a key, posters still
  load; scores need the key." Saving persists the key (prefs
  `omdb_key`), clears all `miss` sidecars, and kicks lookups
  for all current RW entries (a forced kick still skips an
  entry that already has a poster and, when a key is set, a
  stored IMDb score).

## v1.7 verification (Oct 26, 2026)

- `bash build.sh`: BUILD OK (javac source/target 8, android.jar
  on `-cp`; d8 `--min-api 25`; zipalign; apksigner).
- Copied to `HeckYeah-1.7.apk`: **550,056 bytes**, sha256
  `e7b49cae7508f0a792393bee413d3b66ca1570a93dcdc2cfbaa92dd4009ed159`.
- `aapt2 dump badging`: `com.heckyeah.tv` versionCode **8** /
  versionName **1.7**, sdkVersion 25 / targetSdkVersion 25,
  LAUNCHER + LEANBACK_LAUNCHER, banner present.
- `apksigner verify`: Verifies, 1 signer, cert SHA-256
  `220529f9…b92fab5` (unchanged — `adb install -r` preserves
  app data).
- `unzip -t`: no errors.
- `cleanTitle` additionally unit-checked on a desktop JVM with
  the exact shipped source (10 representative page titles:
  separators, junk words/phrases, `(YYYY)` + bare-year
  extraction, `Season N` / `SxxExx` cuts, give-up cases — all
  as designed). The network chain itself is exercised only
  on-device.
- Build-verified only — **NOT yet tested on the physical
  Stick.** On-device check: watch something in the browser,
  hit Done, return Home and focus its RW tile — a poster
  should appear (may take a few seconds on the first build);
  add an OMDb key in Settings to also get the IMDb/RT lines.

## v1.6 (Oct 26, 2026) — fix: entering Recently Watched from above lands on the right side (from the v1.5 test)

Single-issue focus fix; everything else from v1.4/v1.5 is
unchanged.

- **Home focus path resume → DOWN → Search → DOWN → Recently
  Watched landed on a right-side RW tile instead of the first
  (leftmost) one.** Two non-deterministic mechanisms were in
  play: Android's focus memory (a ViewGroup restores its last
  focused descendant when re-entered) and geometry-based
  FocusFinder choosing a tile from the wide header row. Fix is
  deterministic explicit directional focus, wired in
  `buildHome()` on every build (Home is rebuilt on every show):
  - `bSearch.setId(View.generateViewId())` — Home's Search
    button is the only focusable search control above the shelf
    (the `searchInput` EditText lives on the separate Search
    screen, which is untouched). `safariBtn` views otherwise
    carry no id.
  - The first `rwTile()` created in the RW row is captured; if
    it has `View.NO_ID` it gets `View.generateViewId()`. Then
    `bSearch.setNextFocusDownId(firstRwTileId)` and
    `firstRwTile.setNextFocusUpId(bSearchId)`. An explicit
    next-focus id is consulted before FocusFinder geometry and
    overrides the shelf's remembered focused descendant, so
    DOWN from Search ALWAYS starts on the leftmost RW entry and
    UP from that entry returns to Search.
  - **Empty-RW fallback, decided in code by `firstRwTile ==
    null`:** the first category tile (leftmost tile of the
    first category row, id assigned the same way) is captured
    during the grid build, and the wiring target is
    `downTarget = firstRwTile != null ? firstRwTile :
    firstCatTile`; `bSearch.setNextFocusDownId(downTarget)`.
    Ids are generated fresh and wired in the same build pass,
    so no nextFocus id can point at a view from a previous
    build.
  - Belt-and-braces: the RW `HorizontalScrollView` gets a
    posted `scrollTo(0, 0)` after layout on every build, so the
    shelf is entered at its left edge.
  - No ids are wired among the RW tiles themselves — Left/Right
    inside the shelf stays native sequential — and no other
    screen's focus behavior changed. No `bringToFront()` was
    added in any menu path (the v1.5 rule stands; the only call
    left is the cursor overlay over the fullscreen container).

## v1.6 verification (Oct 26, 2026)

- `bash build.sh`: BUILD OK (javac source/target 8, android.jar
  on `-cp`; d8 `--min-api 25`; zipalign; apksigner).
- Copied to `HeckYeah-1.6.apk`: **545,960 bytes**, sha256
  `57c31647a9bc976260328f9f25001c9981e80f5c1b99888db4be87fab6c450a1`.
- `aapt2 dump badging`: `com.heckyeah.tv` versionCode **7** /
  versionName **1.6**, sdkVersion 25 / targetSdkVersion 25,
  LAUNCHER + LEANBACK_LAUNCHER, banner present.
- `apksigner verify`: Verifies, 1 signer, cert SHA-256
  `220529f9…b92fab5` (unchanged — `adb install -r` preserves
  app data).
- `unzip -t`: no errors.
- Build-verified only — **NOT yet tested on the physical
  Stick.** On-device check: on Home, focus the resume bar →
  DOWN to Search → DOWN: selection must land on the leftmost
  Recently Watched tile every time; RIGHT moves to the second
  tile; UP from the first tile returns to Search. With RW
  cleared, Search → DOWN must land on the first category tile.

## v1.5 (Oct 26, 2026) — hotfix: menu navigation regression (from the v1.4 test)

Single-issue fix; everything else from v1.4 is unchanged.

- **Pressing LEFT in the menus swapped tile positions and focus
  stayed pinned on the right side of the screen.** Root cause:
  v1.4's focus-clipping fix called `View.bringToFront()` whenever
  a tile gained focus (inside the shared `focusIntoView()` helper,
  used by the shared focus styler, the RW tile listener, and the
  resume bar). `bringToFront()` is a draw-order API, but in a
  LinearLayout it physically REORDERS the child to the end of the
  parent's child list — so each focus gain moved the focused tile
  to the end of its row, tiles visibly swapped places, and focus
  search geometry broke. Fix: the `bringToFront()` call is gone.
  `focusIntoView()` now raises the focused tile with
  `setTranslationZ(dp(8))` instead — draw order only, layout
  order and focus order are never touched (API 21+, fine on 25) —
  and each of the three focus listeners resets
  `setTranslationZ(0)` on focus loss. The rest of the v1.4
  clipping fix is untouched: clipChildren=false /
  clipToPadding=false chain, ≥10dp shelf padding, and the
  10dp-expanded requestRectangleOnScreen scroll-into-view (none
  of which reorder anything). Audit: no removeView/addView or
  setLayoutParams swaps happen on focus gain anywhere. The one
  remaining `bringToFront()` in the file is the mouse-cursor
  overlay re-asserting itself over the fullscreen container in
  the window DecorView (a FrameLayout); the cursor is
  non-focusable and not in any navigation row, so it cannot
  affect menu navigation.

## v1.5 verification (Oct 26, 2026)

- `bash build.sh`: BUILD OK (javac source/target 8, android.jar
  on `-cp`; d8 `--min-api 25`; zipalign; apksigner).
- Copied to `HeckYeah-1.5.apk`: **541,864 bytes** (same byte size
  as v1.3/v1.4 by coincidence; contents differ), sha256
  `65a0aef92bd3c685a931b4cc425ea56a3cb5bfcdd89b8ea74c0745a7e06e8d8d`.
- `aapt2 dump badging`: `com.heckyeah.tv` versionCode **6** /
  versionName **1.5**, sdkVersion 25 / targetSdkVersion 25.
- `apksigner verify`: Verifies, 1 signer, cert SHA-256
  `220529f9…b92fab5` (unchanged — `adb install -r` preserves
  app data).
- `unzip -t`: no errors.
- Build-verified only — **NOT yet tested on the physical
  Stick.**

## v1.4 (Oct 26, 2026) — browser menu, auto-hiding bar, three fixes (from the v1.3 test + TV photos)

1. **Long-press Browser Menu** — holding CENTER/OK for ~0.5 s in
   the browser (not fullscreen) opens a "Browser Menu" dialog
   replicating every toolbar action in toolbar order: ‹ Back,
   › Forward, Edit URL…, ⟳ Reload, Mouse/D-pad mode toggle
   (label shows the current mode; choosing it updates the toolbar
   Mode button), ⚙ Browser Settings…, ★ Add/Remove current page
   to/from Favorites (built from the live page title+URL when no
   catalog entry matches), ✕ Done. Reachable three ways: mouse
   sub-mode hold (a hold ON a link still gets the link menu; the
   nav menu is the fallback when no link menu appeared), D-pad
   sub-mode hold with the page block focused, and the remote
   MENU key (which no longer toggles the sub-mode outside
   fullscreen — the toggle lives in this menu, the toolbar, and
   the gear sheet). D-pad hold detection: key-down arms a 500 ms
   runnable; if it fires, a flag suppresses the tap that the
   following key-up would otherwise dispatch, so short presses
   are unchanged. The dialog uses the current browser
   appearance (dark default / light) and the trackDialog()
   depth mechanism.
2. **Auto-hiding nav bar (Safari-style)** — the whole header
   block (toolbar + page-title strip + progress bar) hides while
   scrolling down and returns on scroll-up, at the page top, or
   when a new page starts loading. Hiding animates
   translationY/alpha (~180 ms) then sets the block GONE so the
   WebView expands to full height. Scroll deltas come from a
   WebView subclass overriding onScrollChanged (covers native
   D-pad scrolling, mouse edge/wheel scrolling, and the page's
   own JS scrolling), with direction-aware hysteresis: hide only
   past 150dp with >40dp accumulated downward delta, show on
   >20dp accumulated upward delta or scrollY = 0. Extra reveals:
   mouse pushed against the top screen edge; D-pad UP while
   hidden (shows the bar and moves toolbar focus into it).
   Hiding resets toolbar focus; the bar is force-restored
   (no animation) on Done and on fullscreen enter/exit, and it
   never hides while a browser dialog is open.
3. **Focus-outline clipping fix (all menu screens)** — a focused
   tile scales to 1.03 and its white border was sliced wherever
   the tile touched its container. Approach per container: the
   clip-flags route everywhere — clipChildren=false +
   clipToPadding=false on the full ancestor chain (tile rows,
   HorizontalScrollViews, vertical ScrollViews, page columns,
   content frame, window root), ≥10dp padding on all sides
   inside the Home shelves, bringToFront + requestRectangleOnScreen
   with a 10dp-expanded rect on focus gain so the focused tile
   draws over neighbors and scrolls FULLY into view. The
   no-scale fallback was NOT needed anywhere: the 40dp page
   padding plus ≥10dp shelf padding exceeds the ~8dp maximum
   scale growth on every screen, so a complete border is
   guaranteed with the scale kept.
4. **"‹ Back to" resume button stuck on the first page** —
   root cause: the resume fields had exactly ONE writer,
   closeBrowser()'s finalize, and its fallback wrote back
   sessUrl — a field the live-resume path itself seeds FROM the
   saved resume values, so a resumed session could re-save its
   own starting page forever; nothing updated the target during
   browsing or when a new entry opened. Fix: a single
   updateResume() that always overwrites + persists immediately
   (no write-once guards), called at openEntry/openRw,
   onPageFinished, doUpdateVisitedHistory (SPA pushState),
   onReceivedTitle, and in the Done snapshot, which now takes
   webView.getUrl()/getTitle() authoritatively (fallback: last
   tracked navigation, never sessUrl). The bar re-reads
   SharedPreferences each time a screen is built, and resume
   reuses the live session only when the WebView is actually on
   the button's URL — otherwise it loads that URL fresh.
   Traced: [open A → Done → open B → Done] leaves the button on
   B loading B; [open A → in-page navigate to episode → Done]
   leaves it on the episode page.
5. **Fullscreen video sometimes shifted off-center** — cause:
   Chromium hands the custom view over with its in-page geometry
   still attached (the view could arrive already parented, with
   baked-in layout params/offsets), and the view was laid out
   once before the immersive-flag transition finished resizing
   the window. Fix: on fullscreen entry the view is detached
   from any previous parent, all transforms zeroed (x/y,
   translation, rotation, scale, padding), and added with a
   FRESH MATCH_PARENT LayoutParams with gravity CENTER;
   every SurfaceView/TextureView in its hierarchy is forced to
   MATCH_PARENT and each SurfaceView holder gets
   setFixedSize(realDisplayWidth, realDisplayHeight) from
   getRealMetrics; a post-attach layout pass re-runs the
   centering, and onWindowFocusChanged re-runs it again while
   fullscreen so the late immersive resize can't leave stale
   geometry. The surface keeps the video's own aspect handling —
   black bars now come only from the video's aspect ratio and
   are symmetric.

## v1.4 verification (Oct 26, 2026)

- `bash build.sh`: BUILD OK (javac source/target 8, android.jar
  on `-cp`; d8 `--min-api 25`; zipalign; apksigner).
- Copied to `HeckYeah-1.4.apk`: **541,864 bytes**, sha256
  `78939d936f0cae8bf597846274a7d505537fc91e67a02b608c39a15d40ec915b`.
- `aapt2 dump badging`: `com.heckyeah.tv` versionCode **5** /
  versionName **1.4**, sdkVersion 25 / targetSdkVersion 25.
- `apksigner verify`: Verifies, 1 signer, cert SHA-256
  `220529f9…b92fab5` (unchanged — `adb install -r` preserves
  app data).
- `unzip -t`: no errors.
- Build-verified only — **NOT yet tested on the physical
  Stick.**

## v1.3 (Oct 26, 2026) — polish release (from the v1.2 test + toolbar photo)

1. **Fullscreen cursor auto-fade** — in fullscreen + mouse
   sub-mode, the cursor fades out (~300 ms alpha) after 10 s with
   no remote interaction; any handled key makes it reappear and
   restarts the timer. Timer cancels on fullscreen exit or a
   switch to the D-pad sub-mode. Non-fullscreen cursor unchanged.
2. **Recently Watched screenshots** — at snapshot time (Done, plus
   a debounced refresh on page-finished for video sessions) the
   WebView viewport is captured (max 480 px wide, JPEG ~70) to
   `files/rw_shots/<url-hash>.jpg` and the filename stored as a
   new trailing field in the RW record (older records still
   load). On Home, a focused RW tile expands to show the
   screenshot below its title/link info (16:9-ish, center-crop);
   unfocused or no-shot tiles show nothing extra. Entry removal,
   Clear RW, and cap-12 eviction delete the screenshot file.
   **Honest limit:** if the last screen was fullscreen video, the
   capture may be black — Android does not composite
   protected/hardware video surfaces into view screenshots.
3. **Home edge overflow** — focus scale is now 1.03 max
   everywhere, page side padding is 40dp, applet/RW tiles have
   12dp gutters (`tileLp()`), and shelf rows have side padding
   with `clipToPadding=false`, so edge tiles grow inside the
   display instead of past it. Category grid: 12dp gaps between
   tiles and between rows (no more touching tiles).
4. **Browser Dark appearance (default)** — persisted
   "Browser appearance" setting (gear sheet + main Settings)
   restyles the full browser chrome without a reload: toolbar,
   pills, URL pill, progress tint, dialog theme, and the backdrop
   behind pages while loading. **Limit:** API-25 WebView has no
   force-dark (API 29+) — this themes the browser UI only,
   websites render as they design themselves; no CSS hacks.
5. **Toolbar clipping (device photo: Done + outlines cut at the
   right edge)** — toolbar side padding is 16dp, top/bottom 8dp,
   buttons are compact (~38dp, short labels — "Mouse"/"D-pad"),
   the URL pill keeps its flexible weight so it absorbs the
   squeeze, and toolbar buttons **no longer scale on focus**
   (blue ring only), so strokes can never be pushed outside the
   bar. Applies in both appearances.
6. **Inter app-wide** — Inter Regular + SemiBold + Bold
   (copied from the AirPlay TV project's local font assets; the
   closest open-source match to SF Pro) are bundled in
   `assets/fonts/` and applied through the text/button factory
   helpers to Home, categories, shelves, RW tiles, Settings, the
   browser toolbar, and dialogs. APK grows by the font files
   (~540 KB total build).

## v1.3 verification (Oct 26, 2026)

- APK: `build/manual/apk/HeckYeah.apk`, copied to
  `HeckYeah-1.3.apk` — **541,864 bytes**,
  sha256 `82fa880e4b276c7d2b447289eb9518092a021982a2db8a31d481d38168b6a71a`
- `aapt2 dump badging`: package `com.heckyeah.tv`
  **versionCode 4 / versionName 1.3**, sdk 25 / target 25,
  label "HeckYeah TV", launchable + leanback-launchable
  MainActivity, banner present.
- `apksigner verify`: **Verifies** (v2 + v3 true), 1 signer,
  cert SHA-256 `220529f9…b92fab5` (same keystore as v1.0–1.2,
  so it installs over them with `-r`).
- `unzip -t`: no errors. Zip contents include
  `assets/catalog.txt`, `assets/fonts/inter_regular.ttf`
  (324,820 B), `assets/fonts/inter_semibold.ttf` (326,048 B),
  `assets/fonts/inter_bold.ttf` (326,468 B), `classes.dex`.
- Verified by badging/apksigner/zip only — **NOT yet tested on
  the physical Stick.**

## v1.2 (Oct 26, 2026) — fullscreen + rendering fix release

Fixes from the v1.1 test on a Fire TV Stick:

1. **Fullscreen video now fills the display — root cause (v1.1):**
   the fullscreen container was a child of the browser pane's root
   layout (itself nested inside `contentFrame`/`windowRoot`), and
   `onShowCustomView` set no window flags, so the custom view never
   escaped the browser layout and kept its in-page size. **Fix:**
   the container is now attached directly to the window's
   **decorView** (top level, above the browser pane) and the custom
   view is added at MATCH_PARENT × MATCH_PARENT with
   `requestLayout()`; entering fullscreen saves the current system
   UI visibility + window flags, then sets `FLAG_FULLSCREEN` and
   `FLAG_KEEP_SCREEN_ON` and immersive system UI
   (`FULLSCREEN | HIDE_NAVIGATION | IMMERSIVE_STICKY |
   LAYOUT_FULLSCREEN | LAYOUT_STABLE`). Exiting (BACK /
   `onHideCustomView`) removes the container, restores the browser,
   restores the saved flags/visibility, and invokes the stored
   `CustomViewCallback`.
2. **D-pad + mouse work in fullscreen — root cause (v1.1):** the
   fullscreen branch inside `handleBrowserKey` ran before the
   mouse/D-pad handling and simply returned (only CENTER toggled
   play via JS), so the cursor never moved and arrows did nothing;
   the custom view could also take focus, and the cursor overlay
   sat inside `windowRoot`, below any window-level fullscreen
   view. **Fix:** the custom view and its container are made
   non-focusable; fullscreen keys are handled **first** in
   `dispatchKeyEvent` (dialogs exempt, and none can be open in
   fullscreen); the cursor overlay now lives at decor level and is
   brought to the front over the video. In mouse sub-mode the
   D-pad moves the cursor over the video and CENTER dispatches a
   simulated tap at the cursor position **directly to the
   fullscreen container** (players show controls / toggle play).
   In D-pad sub-mode CENTER taps screen center, arrows are
   consumed and tap to surface the player's controls,
   `MEDIA_PLAY_PAUSE` center-taps. Keys are still never forwarded
   into the WebView/custom view, so no accidental seeking; BACK
   exits fullscreen.
3. **Transparent page elements rendered white — root cause
   (v1.1):** the browser pane root was explicitly
   `setBackgroundColor(Color.WHITE)` and the WebView never got a
   background, leaving its default opaque white — transparent /
   rgba page elements composited over white. **Fix:**
   `webView.setBackgroundColor(0x00000000)` (transparent), the
   browser pane root and column are BLACK, the theme window
   background is `#000000`, and the fullscreen container + custom
   view are black so letterboxing is black, not white. (Audit: no
   injected CSS/JS in the app sets page background colors — the
   only JS is pause/seek — so there was nothing to remove there.)

## v1.2 verification (Oct 26, 2026)

- APK: `build/manual/apk/HeckYeah.apk`, copied to
  `HeckYeah-1.2.apk` — **66,502 bytes** (same byte count as v1.1
  by coincidence; contents differ — the new fullscreen methods
  are present in `classes.dex`),
  sha256 `315c4c0c2a723bf9e4220b97ece8b1f73943ecd02b404ee650300f0b62a27520`
- `aapt2 dump badging`: package `com.heckyeah.tv`
  **versionCode 3 / versionName 1.2**, sdk 25 / target 25,
  label "HeckYeah TV", launchable + leanback-launchable
  MainActivity, banner present.
- `apksigner verify`: **Verifies** (v2 + v3 true), 1 signer,
  cert SHA-256 `220529f9…b92fab5` (same keystore as v1.0/v1.1,
  so it installs over them with `-r`).
- `unzip -t`: no errors.
- Verified by badging/apksigner/zip only — **NOT yet tested on
  the physical Stick.**

## v1.1 (Oct 26, 2026) — on-device bug-fix release

Fixes from the v1.0 test on a Fire TV Stick:

1. **Video playback** — `setMediaPlaybackRequiresUserGesture(false)`,
   mixed content always allowed, third-party cookies accepted
   (players/logins need them), plugin state on, hardware
   acceleration on. **`WebChromeClient.onShowCustomView` /
   `onHideCustomView` implemented**: HTML5 fullscreen video now
   goes truly fullscreen (black fullscreen container, toolbar +
   page hidden, immersive system UI); Back exits fullscreen first.
2. **Scaling** — pages were rendering zoomed-in. Now
   `setUseWideViewPort(true)` + `setLoadWithOverviewMode(true)` +
   `TEXT_AUTOSIZING` so a page fits the TV width. New persisted
   **Browser scale** setting (50/75/100/125/150%, **default
   100%**) in Settings and the browser gear sheet: applied via
   `setInitialScale()` before every load and `setTextZoom()` live
   (with reload) when changed, shared by all browser sessions.
3. **Done pauses + suspends** — Done now runs JS pausing every
   `<video>`/`<audio>`, calls `WebView.onPause()` +
   `pauseTimers()`, and hides the browser **without destroying
   the WebView or blanking the page**, so nothing keeps playing
   or running in the background. App-backgrounding while browsing
   also pauses the WebView. The page you just left is saved
   (URL/title/applet/category, SharedPreferences), and Home,
   category, shelf and search screens show a prominent accent
   **"‹ Back to <title>"** button pinned at the very top that
   resumes the same session (media stays paused until you press
   play) — or reloads the saved URL fresh after an app restart.
4. **Padding + button polish** — ≥12dp gaps between all browser
   toolbar buttons, consistent ~40dp pill buttons with white
   backgrounds, `#0A84FF` focus ring + slight scale in D-pad
   mode; menu buttons got larger padding (16×9dp, min height
   40dp) and 10–12dp spacing in headers, setting rows and stacks.
5. **⚙ Browser Settings in the browser** — gear button in the
   toolbar opens a sheet with Scale, Ad blocking, JavaScript,
   Desktop user-agent, Cursor speed, Clear cache, Clear history.
   All persisted; JS/UA/scale changes reload the page.
6. **Navigation rework** — mouse cursor ONLY in the browser;
   Home/categories/search/settings/dialogs are pure D-pad.
   Browser D-pad mode: the URL is a **button** (one focus stop)
   that opens a full URL editor dialog — no more
   character-by-character traversal of an inline EditText.
7. **D-pad keys never reach the page** — while the browser is
   showing, `dispatchKeyEvent` consumes DPAD arrows, CENTER/
   ENTER, PAGE_UP/DOWN and media keys (FF/Rewind/Play-Pause) at
   the activity level: mouse mode drives the cursor, D-pad mode
   scrolls/taps/activates the toolbar. Site players can no
   longer read Left/Right as seek ±10s. This also holds during
   HTML5 fullscreen (Back still exits fullscreen). Browser
   dialogs (URL editor, gear sheet, link menu) are exempt so
   they keep native key handling.
8. **Recently Watched homepage bug — root cause (v1.0):** RW
   was written ONLY from `onPageFinished`, one entry per finished
   page, deduped by exact URL. Modern streaming aggregators are
   SPAs: navigating from the site's homepage to an episode page
   happens via `history.pushState`, which never fires
   `onPageFinished` — so the only URL ever recorded for those
   sites was the applet's homepage URL loaded by `openEntry`
   (plus root/trailing-slash finishes, which were inserted at
   the front and flooded the cap-12 list). Tiles faithfully
   opened the stored URL — the homepage. **Fix:** one RW entry
   per launch session, continuously updated via
   `doUpdateVisitedHistory` + `onPageFinished` +
   `onReceivedTitle`, with a root-URL guard (a `/` URL never
   overwrites a deeper same-host entry), a final
   `getUrl()`/`getTitle()` snapshot on Done, and best-effort
   video-position capture/restore (guarded JS, ignored on any
   failure).

## v1.1 verification (Oct 26, 2026)

- APK: `build/manual/apk/HeckYeah.apk`, copied to
  `HeckYeah-1.1.apk` — **66,502 bytes**,
  sha256 `3a846e116309e69419053e184e6885dab18fb5a50f348233cf9468cc1d138834`
- `aapt2 dump badging`: package `com.heckyeah.tv`
  **versionCode 2 / versionName 1.1**, sdk 25 / target 25,
  label "HeckYeah TV", launchable + leanback-launchable
  MainActivity, banner present.
- `apksigner verify`: **Verifies** (v2 + v3 true), 1 signer,
  cert SHA-256 `220529f9…b92fab5` (same keystore as v1.0).
- `unzip -t`: no errors.
- Verified by badging/apksigner/zip only — **NOT yet tested on
  the physical Stick.**

## v1.0 verification (Oct 26, 2026)

- APK: `build/manual/apk/HeckYeah.apk` — **58,310 bytes**,
  sha256 `8453350e789ad481b3bf81be75846ea6b91b0e26d5dd20c8d44580df7d332d7e`
- `aapt2 dump badging`: package `com.heckyeah.tv` 1.0 (1),
  sdk 25 / target 25, label "HeckYeah TV", launchable +
  leanback-launchable MainActivity, banner present, icons at all
  densities, only the two permissions above.
- `apksigner verify`: **Verifies** (v2 + v3 true), 1 signer,
  cert SHA-256 `220529f9…b92fab5`.
- `unzip -t`: no errors; APK contains `classes.dex` and
  `assets/catalog.txt` (194 lines = header + 193 entries).
- Verified by badging/apksigner/zip only — **NOT yet tested on
  the physical Stick.**

## Honest limits

- **Ad blocking is host-level only.** The API-25 WebView has no
  extension support, so this is not uBlock Origin: no cosmetic /
  element hiding, and first-party or same-host ads and some
  redirect tricks will still get through. Popup windows are
  blocked, which removes the worst streaming-site behavior.
- **The old WebView cannot do DRM.** Widevine-in-WebView playback
  on Fire OS 6 is not available; some modern/streaming sites will
  refuse the browser or fail to play video. Tiles open the sites —
  playback is each site's own player doing what the Stick's
  WebView allows.
- Live Refresh depends on the `fmhy/edit` repo layout
  (`docs/*.md` on `main`, verified Oct 26, 2026). If upstream
  renames files, refresh reports failure and keeps the last good
  catalog / bundled seed.
- Seed entries for gaming/audio/downloading/torrenting/
  educational/linux-macos/misc are starter lists (15–19 each);
  only video/ai/reading seeds were transcribed from full page
  renders. Refresh fills in the rest.
