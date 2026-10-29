package com.heckyeah.tv;

import android.animation.AnimatorListenerAdapter;
import android.animation.ValueAnimator;
import android.app.Activity;
import android.app.AlertDialog;
import android.app.Instrumentation;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.view.ContextThemeWrapper;
import android.content.DialogInterface;
import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.content.res.ColorStateList;
import android.content.SharedPreferences;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.PorterDuff;
import android.graphics.Rect;
import android.graphics.RectF;
import android.graphics.drawable.BitmapDrawable;
import android.graphics.drawable.Drawable;
import android.graphics.drawable.LayerDrawable;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.media.AudioManager;
import android.net.Uri;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.text.TextUtils;
import android.util.DisplayMetrics;
import android.util.Log;
import android.view.Gravity;
import android.view.InputDevice;
import android.view.KeyEvent;
import android.view.MotionEvent;
import android.view.SurfaceView;
import android.view.TextureView;
import android.view.View;
import android.view.ViewTreeObserver;
import android.view.ViewGroup;
import android.view.WindowManager;
import android.view.animation.DecelerateInterpolator;
import android.view.animation.LinearInterpolator;
import android.view.inputmethod.EditorInfo;
import android.webkit.CookieManager;
import android.webkit.ValueCallback;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import org.json.JSONObject;
import android.widget.Button;
import android.widget.ImageView;
import android.widget.EditText;
import android.widget.FrameLayout;
import android.widget.HorizontalScrollView;
import android.widget.LinearLayout;
import android.widget.ProgressBar;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;

import java.io.BufferedReader;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.net.HttpURLConnection;
import java.net.URL;
import java.net.URLEncoder;
import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.Date;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

import org.json.JSONArray;
import org.json.JSONObject;

/**
 * HeckYeah TV - FMHY (freemediaheckyeah) as a tvOS-style Fire TV app.
 * Framework APIs only, minSdk/target 25 (Fire OS 6 / Android 7.1.1).
 *
 * Core: 12 FMHY categories as large tiles; category pages scaffolded by
 * FMHY ##/### headers into shelves of small applet tiles; tiles open in
 * a Safari-styled in-app WebView browser with host-level ad blocking
 * and popup blocking.
 *
 * v1.1 navigation model: D-pad/native focus EVERYWHERE outside the
 * browser. The mouse cursor exists ONLY inside the browser, where two
 * sub-modes (mouse default, D-pad alternative) remain. In the browser,
 * all D-pad/media keys are intercepted in dispatchKeyEvent and never
 * reach the web page (video players would read Left/Right as seek).
 *
 * Extras: A Favorites, B Global Search, D Live Refresh (fetch
 * fmhy/edit docs/*.md from GitHub raw, parse, cache), E Mirror Picker.
 * RW Recently Watched: first shelf on Home; for video-category launches
 * only, the browser session's latest content page (URL + title, tracked
 * via doUpdateVisitedHistory so SPA navigation counts, snapshotted
 * again on Done) is stored so a tile reopens the exact episode page.
 */
public class MainActivity extends Activity {

    static final String PREFS = "heckyeah_prefs";
    private static final String KEY_MOUSE = "mouse_mode";       // browser sub-mode only (v1.1)
    private static final String KEY_ADBLOCK = "adblock";
    private static final String KEY_CURSOR_STEP = "cursor_step";
    private static final String KEY_FAVS = "favorites";
    private static final String KEY_RW = "recently_watched";
    private static final String KEY_UPDATED = "catalog_updated_at";
    private static final String KEY_SCALE = "browser_scale";    // percent, default 100
    private static final String KEY_JS = "browser_js";
    private static final String KEY_DESKTOP_UA = "desktop_ua";
    private static final String KEY_APPEARANCE = "browser_appearance"; // dark (default) | light
    private static final String KEY_VIDEO_LOCK = "video_scroll_lock"; // v1.13: default ON
    private static final String KEY_REMOTE = "remote_on"; // v1.19: phone remote server, default OFF
    private static final String KEY_RESUME = "resume_state";    // url SEP title SEP applet SEP cat SEP ts SEP seek
    private static final String SEP = "\u0001";
    private static final int RW_CAP = 12;
    private static final int[] SCALES = {50, 75, 100, 125, 150};
    private static final String DESKTOP_UA =
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
            + "(KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

    private static final int BG = Color.parseColor("#0B0B10");
    private static final int BG2 = Color.parseColor("#1C1C1E");
    private static final int CARD = Color.parseColor("#2C2C2E");
    private static final int TEXT = Color.parseColor("#F2F2F7");
    private static final int DIM = Color.parseColor("#AEAEB2");
    private static final int ACCENT = Color.parseColor("#0A84FF");
    private static final int DONE_RED = Color.parseColor("#FF453A"); // v1.27: toolbar Done
    private static final int SAFARI_BG = Color.parseColor("#F2F2F7");
    private static final int SAFARI_TEXT = Color.parseColor("#1C1C1E");
    private static final int PILL = Color.parseColor("#FFFFFF");
    private static final int PILL_BORDER = Color.parseColor("#D1D1D6");

    // id, title, description, docs file, glyph, gradient start, gradient end
    static final String[][] CATS = {
        {"video", "Streaming", "Stream, download, torrent and binge movies and shows", "video.md", "\u25B6", "#0A84FF", "#5E5CE6"},
        {"audio", "Listening", "Stream, download and torrent songs, podcasts and more", "audio.md", "\u266B", "#AF52DE", "#FF2D92"},
        {"gaming", "Gaming", "Download and play games or emulate old but gold ones", "gaming.md", "\u2726", "#30D158", "#64D2FF"},
        {"reading", "Reading", "Books, comics, manga and magazines for every reader", "reading.md", "\u2750", "#FF9F0A", "#FF375F"},
        {"ai", "Artificial Intelligence", "Explore the world of AI and machine learning", "ai.md", "\u273A", "#64D2FF", "#0A84FF"},
        {"downloading", "Downloading", "Download software, movies, shows, music, games and more", "downloading.md", "\u2193", "#FFD60A", "#FF9F0A"},
        {"torrenting", "Torrenting", "Download media using the BitTorrent protocol", "torrenting.md", "\u21C4", "#30D158", "#0A84FF"},
        {"educational", "Educational", "Educational content for all ages", "educational.md", "\u2727", "#5E5CE6", "#AF52DE"},
        {"mobile", "Android / iOS", "All forms of content for Android and iOS", "mobile.md", "\u25A3", "#FF375F", "#AF52DE"},
        {"linux-macos", "Linux / macOS", "The $HOME of Linux and macOS", "linux-macos.md", "\u2328", "#48484A", "#0A84FF"},
        {"non-english", "Non-English", "Content in languages other than English", "non-english.md", "\u2295", "#FF6482", "#FFD60A"},
        {"misc", "Miscellaneous", "Food, travel, news, shopping, fun sites and more", "misc.md", "\u2731", "#64D2FF", "#30D158"},
    };

    static String catTitle(String id) {
        for (String[] c : CATS) if (c[0].equals(id)) return c[1];
        return id;
    }

    static class Entry {
        String cat, shelf, name, url, desc, mirrors;
        boolean rec;
        Entry(String cat, String shelf, String name, String url, String desc, boolean rec, String mirrors) {
            this.cat = cat; this.shelf = shelf; this.name = name; this.url = url;
            this.desc = desc; this.rec = rec; this.mirrors = mirrors == null ? "" : mirrors;
        }
        List<String> mirrorList() {
            List<String> out = new ArrayList<>();
            if (!mirrors.isEmpty()) for (String m : mirrors.split(",")) if (!m.trim().isEmpty()) out.add(m.trim());
            return out;
        }
        String favKey() { return name + SEP + url + SEP + cat; }
    }

    static class RwItem {
        String title, url, source; long ts; double seek; // seek = video currentTime seconds (best-effort)
        String shot = ""; // v1.3: rw_shots/<hash>.jpg filename, "" = no screenshot
        RwItem(String t, String u, String s, long ts, double seek) {
            title = t; url = u; source = s; this.ts = ts; this.seek = seek;
        }
    }

    private SharedPreferences prefs;
    private final Handler ui = new Handler(Looper.getMainLooper());
    private final List<Entry> catalog = new ArrayList<>();
    private final List<RwItem> rw = new ArrayList<>();
    private final List<String> favKeys = new ArrayList<>(); // name SEP url SEP cat

    // v1.3: Inter typefaces (closest open-source match to SF Pro),
    // loaded once from assets/fonts/ in onCreate.
    private Typeface fontReg, fontSemi, fontBold;
    private Typeface tf(boolean bold) {
        Typeface t = bold ? (fontBold != null ? fontBold : fontSemi) : fontReg;
        return t != null ? t : (bold ? Typeface.DEFAULT_BOLD : Typeface.DEFAULT);
    }

    // v1.3: fullscreen cursor auto-fade (mouse sub-mode). v1.17:
    // generalized from real HTML5 fullscreen to "fullscreen-like"
    // state - v1.16's pseudo-fullscreen (a page overlay the app is
    // never handed as a custom view, so fsView stays null) never
    // armed the v1.3 fade and the cursor burned in over the video.
    private boolean cursorFaded = false;
    /** Fullscreen-like = a video is covering the screen, whether
     *  Chromium handed us the custom view (fsView) or the page
     *  faked it (pseudoFs). The raw pseudoFs field is safe here:
     *  while pseudo is active the ~750ms poll keeps it current,
     *  and every pseudo exit path (OFF transition, closeBrowser)
     *  cancels the fade explicitly - plus fsFadeRunnable re-checks
     *  this at fire time regardless. */
    private boolean fsLikeActive() { return fsView != null || pseudoFs; }
    private final Runnable fsFadeRunnable = new Runnable() {
        @Override public void run() {
            if (fsLikeActive() && browserMouse && cursorView != null) {
                cursorFaded = true;
                cursorView.animate().alpha(0f).setDuration(300).start();
            }
        }
    };
    private void pokeFsCursor() {
        if (cursorFaded && cursorView != null) {
            cursorView.animate().cancel(); cursorView.setAlpha(1f);
        }
        cursorFaded = false;
        ui.removeCallbacks(fsFadeRunnable);
        if (fsLikeActive() && browserMouse) ui.postDelayed(fsFadeRunnable, 10000);
    }
    private void cancelFsFade() {
        ui.removeCallbacks(fsFadeRunnable);
        cursorFaded = false;
        if (cursorView != null) { cursorView.animate().cancel(); cursorView.setAlpha(1f); }
    }

    // v1.3: RW screenshots
    private final Map<String, Bitmap> shotCache = new HashMap<>();
    private long lastShotAt = 0;
    private String lastShotUrl = "";
    private File rwShotsDir() { return new File(getFilesDir(), "rw_shots"); }
    private static String shotNameFor(String url) {
        return Long.toHexString(url == null ? 0 : (url.hashCode() & 0xffffffffL)) + ".jpg";
    }
    private void deleteShotFile(RwItem i) {
        if (i == null || i.shot == null || i.shot.isEmpty()) return;
        try { File f = new File(rwShotsDir(), i.shot); if (f.isFile()) f.delete(); }
        catch (Exception ignored) {}
    }

    // ------------------------------------------------- RW poster / ratings enrichment (v1.7, chain v2 in v1.8)
    //
    // RW screenshots come out black whenever the last screen was
    // protected video (Android does not composite those surfaces
    // into a view draw). So each RW entry is enriched from its page
    // title instead: clean the title, look the title up, download a
    // real poster, and (when an OMDb key is configured) store the
    // IMDb + Rotten Tomatoes scores. The focused RW tile shows the
    // poster + scores when a poster exists and falls back to the
    // existing screenshot when it does not.
    //
    // State lives in a SIDECAR - the RW record format is unchanged:
    // prefs keys enr_<urlhash>_{poster,imdb,rt,state,ts,blocked} and
    // poster bytes at files/posters/<urlhash>.jpg, where <urlhash>
    // is the same hex hash shotNameFor() uses for rw_shots filenames.
    //
    // v1.8 chain v2: v1.7's loose iTunes/TVmaze title scoring
    // produced false positives (the "Clayface" RW entry got artwork
    // from an animated Batman series). The chain is now
    // OMDb (key, exact) -> Wikipedia page image (exact article
    // title) -> DuckDuckGo image search (first verified PORTRAIT
    // poster) -> iTunes/TVmaze demoted to last resort with EXACT
    // normalized-title equality only. Posters fetched by the v1
    // matcher are invalidated once via the enr_ver migration.

    private static final String KEY_OMDB = "omdb_key";
    private static final String KEY_ENR_VER = "enr_ver";
    private static final int ENR_VER = 2; // v1.8 poster-chain schema
    private static final String KEY_SHOT_VER = "shot_ver";
    private static final int SHOT_VER = 2; // v1.12: strict-viewport screenshots (v1.10 capture)
    private static final long ENR_MISS_RETRY_MS = 24L * 60 * 60 * 1000;
    // (the DDG/Wikipedia steps reuse the existing DESKTOP_UA constant
    // declared with the browser settings keys - a desktop browser UA.)
    private final Set<String> enrInFlight = new HashSet<>();
    private final Map<String, Bitmap> posterCache = new HashMap<>();
    private Button omdbBtnSettings;

    private static String rwHash(String url) {
        return Long.toHexString(url == null ? 0 : (url.hashCode() & 0xffffffffL));
    }
    private File postersDir() { return new File(getFilesDir(), "posters"); }
    private File posterFile(String hash) { return new File(postersDir(), hash + ".jpg"); }
    private String enrGet(String hash, String field) {
        String v = prefs.getString("enr_" + hash + "_" + field, "");
        return v == null ? "" : v;
    }
    private void deleteEnrichmentForUrl(String url) {
        if (url == null) return;
        String hash = rwHash(url);
        try { File f = posterFile(hash); if (f.isFile()) f.delete(); }
        catch (Exception ignored) {}
        posterCache.remove(hash);
        prefs.edit().remove("enr_" + hash + "_poster").remove("enr_" + hash + "_imdb")
                .remove("enr_" + hash + "_rt").remove("enr_" + hash + "_state")
                .remove("enr_" + hash + "_ts").remove("enr_" + hash + "_blocked").apply();
    }

    /**
     * v1.8 migration: posters fetched under the v1 (loose) matcher
     * may be flat-out wrong (see the Clayface case in the chain
     * comment above). The first run under chain v2 clears every
     * stored poster file + lookup state so each current RW entry
     * is re-looked-up with the new chain. Ratings (imdb/rt) are
     * KEPT: they only ever came from OMDb exact lookups.
     */
    private void migrateEnrichmentIfNeeded() {
        if (prefs.getInt(KEY_ENR_VER, 0) >= ENR_VER) return;
        List<String> hashes = new ArrayList<>();
        for (String k : prefs.getAll().keySet()) {
            if (k.startsWith("enr_") && k.endsWith("_state")) {
                hashes.add(k.substring(4, k.length() - "_state".length()));
            }
        }
        SharedPreferences.Editor ed = prefs.edit();
        for (String h : hashes) {
            ed.remove("enr_" + h + "_poster").remove("enr_" + h + "_state")
              .remove("enr_" + h + "_ts");
            try { File f = posterFile(h); if (f.isFile()) f.delete(); }
            catch (Exception ignored) {}
            posterCache.remove(h);
        }
        ed.putInt(KEY_ENR_VER, ENR_VER);
        ed.apply();
    }
    private void deleteEnrichment(RwItem i) { if (i != null) deleteEnrichmentForUrl(i.url); }

    /**
     * v1.12 migration: screenshots captured before v1.10 used the
     * pre-scaled-canvas draw (see captureViewportBitmap) and came
     * out as squeezed re-renders of the page instead of the visible
     * viewport. Those files persist across updates, so without this
     * one-time purge users would keep SEEING the old bad shots on
     * RW tiles / the resume preview even though every new capture
     * is correct. Purge = every file in rw_shots/ + resume_shot.jpg
     * + both in-memory decoded-shot caches, then stamp shot_ver.
     * RW records keep their shot filename field - a missing file
     * is simply "no screenshot" for both loaders (RW tile:
     * decodeFile returns null and the shot view stays GONE; resume
     * preview: resumeShotBitmap() gates on isFile()), and fresh
     * shots re-capture through the normal Done / debounced
     * triggers. Posters and enrichment are NOT touched here -
     * that is enr_ver's separate migration.
     */
    private void migrateShotsIfNeeded() {
        if (prefs.getInt(KEY_SHOT_VER, 0) >= SHOT_VER) return;
        try {
            File[] files = rwShotsDir().listFiles();
            if (files != null) for (File f : files) f.delete();
        } catch (Exception ignored) {}
        deleteResumeShot(); // resume_shot.jpg + resumeShotBm/resumeShotStamp reset
        shotCache.clear();
        prefs.edit().putInt(KEY_SHOT_VER, SHOT_VER).apply();
    }

    /**
     * v1.9: the ONE Recently Watched wipe - entries, their
     * rw_shots files, poster files and enrichment sidecars - shared
     * by Settings -> "Clear Recently Watched" and the Home RW
     * shelf's Clear card, so the two can never drift apart.
     */
    private void clearAllRw() {
        for (RwItem i : rw) { deleteShotFile(i); deleteEnrichment(i); }
        shotCache.clear();
        rw.clear();
        sessItem = null;
        saveRw();
    }

    /** Clears every lookup_state=miss sidecar (Settings: key saved). */
    private void clearEnrMisses() {
        List<String> hashes = new ArrayList<>();
        for (String k : prefs.getAll().keySet()) {
            if (k.startsWith("enr_") && k.endsWith("_state")
                    && "miss".equals(prefs.getString(k, ""))) {
                hashes.add(k.substring(4, k.length() - "_state".length()));
            }
        }
        SharedPreferences.Editor ed = prefs.edit();
        for (String h : hashes) {
            ed.remove("enr_" + h + "_poster").remove("enr_" + h + "_imdb")
              .remove("enr_" + h + "_rt").remove("enr_" + h + "_state")
              .remove("enr_" + h + "_ts");
            try { File f = posterFile(h); if (f.isFile()) f.delete(); }
            catch (Exception ignored) {}
            posterCache.remove(h);
        }
        ed.apply();
    }

    // ---- title cleaning ----

    static class CleanedTitle { String title; Integer year; }

    /**
     * Turns a streaming-site page title into a lookup title:
     * "Watch Project Hail Mary (2026) Full Movie Online Free | X"
     * -> {title "Project Hail Mary", year 2026}. Returns null when
     * nothing usable remains.
     */
    static CleanedTitle cleanTitle(String pageTitle, String url) {
        if (pageTitle == null) return null;
        String t = pageTitle;
        // Cut at the first site-name separator; keep the head.
        int cut = -1;
        for (String sep : new String[]{" | ", " \u2013 ", " \u2014 ", " - "}) {
            int i = t.indexOf(sep);
            if (i >= 0 && (cut < 0 || i < cut)) cut = i;
        }
        if (cut >= 0) t = t.substring(0, cut);
        // Leading junk words, repeatedly ("Watch Free X" -> "X").
        while (true) {
            String low = t.trim().toLowerCase(Locale.US);
            boolean stripped = false;
            for (String w : new String[]{"watch free ", "watch online ", "watch ", "stream "}) {
                if (low.startsWith(w)) { t = t.trim().substring(w.length()); stripped = true; break; }
            }
            if (!stripped) break;
        }
        // Trailing / embedded junk phrases.
        for (String junk : new String[]{"Full Movie", "Full Episode", "Online Free",
                "Watch Online", "Free Online", "Movie Online"}) {
            t = t.replaceAll("(?i)" + Pattern.quote(junk), " ");
        }
        t = t.replaceAll("(?i)\\bHD\\b", " ");
        // Year: first (…) group holding a 4-digit year, else the
        // first bare 4-digit year token. Extracted AND removed.
        Integer year = null;
        Matcher ym = Pattern.compile("\\(((?:19|20)\\d{2})\\)").matcher(t);
        if (ym.find()) {
            year = Integer.valueOf(ym.group(1));
            t = t.replace(ym.group(0), " ");
        } else {
            Matcher bm = Pattern.compile("\\b((?:19|20)\\d{2})\\b").matcher(t);
            if (bm.find()) {
                year = Integer.valueOf(bm.group(1));
                t = t.replace(bm.group(0), " ");
            }
        }
        // Episode markers: everything from the marker on is junk.
        int ecut = -1;
        for (String pat : new String[]{"(?i)\\bseason\\s*\\d+",
                "(?i)\\bS\\d{1,2}E\\d{1,2}\\b", "(?i)\\bepisode\\s*\\d+"}) {
            Matcher m = Pattern.compile(pat).matcher(t);
            if (m.find() && (ecut < 0 || m.start() < ecut)) ecut = m.start();
        }
        if (ecut >= 0) t = t.substring(0, ecut);
        t = t.replaceAll("\\s+", " ").trim();
        t = t.replaceAll("^[^A-Za-z0-9]+|[^A-Za-z0-9]+$", "").trim();
        if (t.length() < 2) return null;
        CleanedTitle out = new CleanedTitle();
        out.title = t; out.year = year;
        return out;
    }

    /** Normalized comparison form: lowercase, alphanumerics only. */
    private static String normTitle(String s) {
        return s == null ? "" : s.toLowerCase(Locale.US).replaceAll("[^a-z0-9]", "");
    }
    /** exact 3 > startsWith 2 > contains 1 > no match 0. */
    private static int titleScore(String candidate, String cleaned) {
        String a = normTitle(candidate), b = normTitle(cleaned);
        if (a.isEmpty() || b.isEmpty()) return 0;
        if (a.equals(b)) return 3;
        if (a.startsWith(b) || b.startsWith(a)) return 2;
        if (a.contains(b) || b.contains(a)) return 1;
        return 0;
    }

    // ---- HTTP (8s timeouts; every failure degrades silently) ----

    private static byte[] httpGetBytes(String urlStr) {
        return httpGetBytes(urlStr, "HeckYeahTV/1.8");
    }
    private static byte[] httpGetBytes(String urlStr, String userAgent) {
        HttpURLConnection conn = null;
        try {
            conn = (HttpURLConnection) new URL(urlStr).openConnection();
            conn.setConnectTimeout(8000); conn.setReadTimeout(8000);
            conn.setRequestProperty("User-Agent", userAgent);
            if (conn.getResponseCode() != 200) return null;
            InputStream in = conn.getInputStream();
            ByteArrayOutputStream bos = new ByteArrayOutputStream();
            byte[] buf = new byte[8192]; int n;
            while ((n = in.read(buf)) > 0) bos.write(buf, 0, n);
            in.close();
            return bos.toByteArray();
        } catch (Exception e) { return null; }
        finally { if (conn != null) conn.disconnect(); }
    }
    private static String httpGetString(String urlStr) {
        return httpGetString(urlStr, null);
    }
    private static String httpGetString(String urlStr, String userAgent) {
        byte[] b = userAgent == null ? httpGetBytes(urlStr) : httpGetBytes(urlStr, userAgent);
        if (b == null) return null;
        try { return new String(b, "UTF-8"); } catch (Exception e) { return null; }
    }
    /** Writes poster bytes to dest; the bytes must decode as a real image. */
    private boolean downloadPosterBytes(byte[] b, File dest) {
        if (b == null || b.length < 1024) return false;
        try {
            File dir = dest.getParentFile();
            if (dir != null && !dir.exists()) dir.mkdirs();
            FileOutputStream fos = new FileOutputStream(dest);
            fos.write(b); fos.flush(); fos.close();
        } catch (Exception e) { return false; }
        if (BitmapFactory.decodeFile(dest.getAbsolutePath()) == null) {
            try { dest.delete(); } catch (Exception ignored) {}
            return false;
        }
        return true;
    }
    /** Downloads a candidate poster; must decode as a real image. */
    private boolean downloadPoster(String urlStr, File dest) {
        if (urlStr == null || urlStr.isEmpty() || "N/A".equals(urlStr)) return false;
        return downloadPosterBytes(httpGetBytes(urlStr), dest);
    }
    /** Decoded pixel dimensions of image bytes, or null when not an image. */
    private static int[] imageDims(byte[] b) {
        if (b == null) return null;
        BitmapFactory.Options o = new BitmapFactory.Options();
        o.inJustDecodeBounds = true;
        BitmapFactory.decodeByteArray(b, 0, b.length, o);
        return o.outWidth > 0 && o.outHeight > 0 ? new int[]{o.outWidth, o.outHeight} : null;
    }
    /** Poster aspect test: portrait with width/height in [0.55, 0.85] (2:3 ~= 0.67). */
    private static boolean portraitDims(int w, int h) {
        if (w <= 0 || h <= 0) return false;
        double r = (double) w / (double) h;
        return r >= 0.55 && r <= 0.85;
    }

    // ---- lookup chain step (a): OMDb (needs the configured key) ----

    private static class OmdbHit { String poster, imdb, rt; }

    private OmdbHit omdbHitFrom(JSONObject o) {
        OmdbHit h = new OmdbHit();
        String p = o.optString("Poster", "");
        h.poster = (p == null || p.isEmpty() || "N/A".equals(p)) ? null : p;
        String r = o.optString("imdbRating", "");
        h.imdb = (r == null || r.isEmpty() || "N/A".equals(r)) ? null : r;
        JSONArray ratings = o.optJSONArray("Ratings");
        if (ratings != null) {
            for (int i = 0; i < ratings.length(); i++) {
                JSONObject e = ratings.optJSONObject(i);
                if (e != null && "Rotten Tomatoes".equals(e.optString("Source"))) {
                    String v = e.optString("Value", "");
                    h.rt = (v == null || v.isEmpty() || "N/A".equals(v)) ? null : v;
                }
            }
        }
        return h;
    }
    private OmdbHit omdbByTitle(String key, CleanedTitle ct, boolean withYear) {
        try {
            String u = "https://www.omdbapi.com/?apikey=" + URLEncoder.encode(key, "UTF-8")
                    + "&t=" + URLEncoder.encode(ct.title, "UTF-8");
            if (withYear && ct.year != null) u += "&y=" + ct.year;
            String body = httpGetString(u);
            if (body == null) return null;
            JSONObject o = new JSONObject(body);
            if (!"True".equals(o.optString("Response"))) return null;
            return omdbHitFrom(o);
        } catch (Exception e) { return null; }
    }
    /** &s= search fallback: closest normalized Title wins, then the
     * full record is fetched by imdbID (search hits carry no ratings). */
    private OmdbHit omdbSearch(String key, CleanedTitle ct) {
        try {
            String u = "https://www.omdbapi.com/?apikey=" + URLEncoder.encode(key, "UTF-8")
                    + "&s=" + URLEncoder.encode(ct.title, "UTF-8");
            String body = httpGetString(u);
            if (body == null) return null;
            JSONObject o = new JSONObject(body);
            if (!"True".equals(o.optString("Response"))) return null;
            JSONArray arr = o.optJSONArray("Search");
            if (arr == null) return null;
            JSONObject best = null; int bestScore = 0;
            for (int i = 0; i < arr.length(); i++) {
                JSONObject e = arr.optJSONObject(i);
                if (e == null) continue;
                int sc = titleScore(e.optString("Title"), ct.title);
                if (sc > bestScore) { bestScore = sc; best = e; }
            }
            if (best == null || bestScore == 0) return null;
            String id = best.optString("imdbID", "");
            if (id == null || id.isEmpty()) return null;
            String dbody = httpGetString("https://www.omdbapi.com/?apikey="
                    + URLEncoder.encode(key, "UTF-8") + "&i=" + URLEncoder.encode(id, "UTF-8"));
            if (dbody == null) return null;
            JSONObject d = new JSONObject(dbody);
            if (!"True".equals(d.optString("Response"))) return null;
            return omdbHitFrom(d);
        } catch (Exception e) { return null; }
    }

    // ---- lookup chain step (b): Wikipedia page image (no key) ----
    //
    // The English Wikipedia film/TV article for a title carries the
    // official poster as its page image in the overwhelming majority
    // of cases, so an article-title match is nearly as reliable as
    // OMDb. A page is accepted ONLY when its title is exactly the
    // cleaned title, "<T> (film)", "<T> (<YEAR> film)", "<T> (TV
    // series)" or "<T> (<...> TV series)" - loose search hits whose
    // titles merely mention the name never qualify.

    /** Wikipedia acceptance rule for a page title (see above). */
    private static boolean wikiTitleOk(String pageTitle, CleanedTitle ct) {
        if (pageTitle == null) return false;
        String lp = pageTitle.trim().toLowerCase(Locale.US);
        String lt = ct.title.trim().toLowerCase(Locale.US);
        if (lp.equals(lt)) return true;
        if (!lp.startsWith(lt)) return false;
        String rest = lp.substring(lt.length()).trim();
        return rest.matches("^\\((\\d{4} )?film\\)$")
                || rest.matches("^\\((.+ )?tv series\\)$");
    }

    /** One Wikipedia generator=search query; returns the image URL
     * of the first (by search rank) accepted page that has one. */
    private String wikiSearchPoster(String gsrsearch, CleanedTitle ct) {
        try {
            String u = "https://en.wikipedia.org/w/api.php?action=query&generator=search"
                    + "&gsrnamespace=0&gsrlimit=6&prop=pageimages"
                    + "&piprop=original%7Cthumbnail&pithumbsize=600&format=json"
                    + "&gsrsearch=" + URLEncoder.encode(gsrsearch, "UTF-8");
            String body = httpGetString(u);
            if (body == null) return null;
            JSONObject pages = new JSONObject(body).optJSONObject("query");
            pages = pages == null ? null : pages.optJSONObject("pages");
            if (pages == null) return null;
            // Pages arrive as an object keyed by pageid; order them
            // by the search "index" rank before judging.
            List<JSONObject> ordered = new ArrayList<>();
            java.util.Iterator<String> keys = pages.keys();
            while (keys.hasNext()) {
                JSONObject p = pages.optJSONObject(keys.next());
                if (p != null) ordered.add(p);
            }
            ordered.sort((a, b) -> Integer.compare(a.optInt("index", 99), b.optInt("index", 99)));
            for (JSONObject p : ordered) {
                if (!wikiTitleOk(p.optString("title", ""), ct)) continue;
                JSONObject orig = p.optJSONObject("original");
                String src = orig == null ? "" : orig.optString("source", "");
                if (src == null || src.isEmpty()) {
                    JSONObject th = p.optJSONObject("thumbnail");
                    src = th == null ? "" : th.optString("source", "");
                }
                if (src != null && !src.isEmpty()) return src;
            }
            return null;
        } catch (Exception e) { return null; }
    }

    private String wikipediaPoster(CleanedTitle ct) {
        // Primary: intitle-anchored query. Fallbacks: plain film /
        // TV-series phrasings (generator search syntax can vary).
        String q = "intitle:\"" + ct.title + "\" (film|series)"
                + (ct.year != null ? " " + ct.year : "");
        String u = wikiSearchPoster(q, ct);
        if (u != null) return u;
        u = wikiSearchPoster(ct.title + " film" + (ct.year != null ? " " + ct.year : ""), ct);
        if (u != null) return u;
        return wikiSearchPoster(ct.title + " TV series", ct);
    }

    // ---- lookup chain step (c): DuckDuckGo image search (no key) ----
    //
    // Requested approach: image-search "<title> movie
    // poster" and take the first PORTRAIT poster. DDG images use an
    // unofficial two-step token flow: the html page carries a vqd
    // token, then i.duckduckgo.com/i.js returns JSON results. All
    // requests use a desktop browser UA. Best-effort: any failure
    // (token missing, bot challenge, bad JSON) degrades silently to
    // the next chain step. A candidate only wins after its bytes
    // are downloaded and its ACTUAL decoded dimensions are
    // portrait (w/h 0.55-0.85) and >=250px wide - declared result
    // dimensions are used only as a cheap pre-filter.

    private String ddgVqd(String query) {
        try {
            String html = httpGetString("https://duckduckgo.com/?q="
                    + URLEncoder.encode(query, "UTF-8") + "&iax=images&ia=images", DESKTOP_UA);
            if (html == null) return null;
            Matcher m = Pattern.compile("vqd[=\"':]+([0-9][0-9-]*)").matcher(html);
            return m.find() ? m.group(1) : null;
        } catch (Exception e) { return null; }
    }

    /** Tries the DDG poster queries in order; downloads candidates
     * (max 6 across all queries) and returns true when one passes
     * the real-dimension portrait test and is saved to dest. */
    private boolean ddgPoster(CleanedTitle ct, File dest) {
        List<String> queries = new ArrayList<>();
        String y = ct.year != null ? " " + ct.year : "";
        queries.add("\"" + ct.title + "\"" + y + " movie poster");
        if (ct.year != null) queries.add(ct.title + y + " movie poster");
        queries.add(ct.title + " official poster");
        queries.add(ct.title + " poster");
        String want = normTitle(ct.title);
        int downloads = 0;
        for (String q : queries) {
            if (downloads >= 6) break;
            String vqd = ddgVqd(q);
            if (vqd == null) continue;
            String body;
            try {
                body = httpGetString("https://i.duckduckgo.com/i.js?l=us-en&o=json&q="
                        + URLEncoder.encode(q, "UTF-8") + "&vqd=" + vqd + "&f=,,,&p=1", DESKTOP_UA);
            } catch (Exception e) { body = null; }
            if (body == null) continue;
            JSONArray results;
            try { results = new JSONObject(body).optJSONArray("results"); }
            catch (Exception e) { continue; }
            if (results == null) continue;
            for (int i = 0; i < results.length() && downloads < 6; i++) {
                JSONObject r = results.optJSONObject(i);
                if (r == null) continue;
                String img = r.optString("image", "");
                if (img == null || img.isEmpty()) continue;
                // Cheap pre-filter on the DECLARED dimensions.
                int dw = r.optInt("width", 0), dh = r.optInt("height", 0);
                if (dw < 250 || dh <= dw || !portraitDims(dw, dh)) continue;
                // The result title must name the title (the query
                // itself quoted the title + "poster", so a contains
                // test is meaningful here and only here).
                if (!normTitle(r.optString("title", "")).contains(want)) continue;
                downloads++;
                byte[] b = httpGetBytes(img, DESKTOP_UA);
                int[] dims = imageDims(b);
                if (dims == null || dims[0] < 250 || !portraitDims(dims[0], dims[1])) continue;
                if (downloadPosterBytes(b, dest)) return true;
            }
        }
        return false;
    }

    // ---- lookup chain step (d): iTunes + TVmaze, STRICT (v1.8) ----
    //
    // Demoted to last resort and tightened after the v1.7 false
    // positive ("Clayface" got animated-Batman artwork): a candidate
    // now wins ONLY on exact normalized-title equality. Contains /
    // startsWith matches are rejects, a known year off by more than
    // 1 is a reject, and iTunes TV episodes are rejects.

    private String itunesPoster(CleanedTitle ct, String media) {
        try {
            String u = "https://itunes.apple.com/search?term=" + URLEncoder.encode(ct.title, "UTF-8")
                    + "&media=" + media + "&limit=8";
            String body = httpGetString(u);
            if (body == null) return null;
            JSONArray arr = new JSONObject(body).optJSONArray("results");
            if (arr == null) return null;
            for (int i = 0; i < arr.length(); i++) {
                JSONObject e = arr.optJSONObject(i);
                if (e == null) continue;
                if ("tv-episode".equals(e.optString("kind", ""))) continue;
                String name = e.optString("trackName", "");
                if (name == null || name.isEmpty()) name = e.optString("collectionName", "");
                if (!normTitle(name).equals(normTitle(ct.title))) continue; // EXACT only
                if (ct.year != null) {
                    String rd = e.optString("releaseDate", "");
                    if (rd != null && rd.length() >= 4) {
                        try {
                            int yr = Integer.parseInt(rd.substring(0, 4));
                            if (Math.abs(yr - ct.year) > 1) continue; // known year, too far off
                        } catch (Exception ignored) {}
                    }
                }
                String art = e.optString("artworkUrl100", "");
                if (art == null || art.isEmpty()) continue;
                String big = art.replace("100x100bb.jpg", "600x600bb.jpg");
                if (big.equals(art)) big = art.replace("100x100bb", "600x600");
                return big;
            }
            return null;
        } catch (Exception e) { return null; }
    }

    private String tvmazePoster(CleanedTitle ct) {
        try {
            String u = "https://api.tvmaze.com/search/shows?q=" + URLEncoder.encode(ct.title, "UTF-8");
            String body = httpGetString(u);
            if (body == null) return null;
            JSONArray arr = new JSONArray(body);
            for (int i = 0; i < arr.length(); i++) {
                JSONObject e = arr.optJSONObject(i);
                if (e == null) continue;
                JSONObject show = e.optJSONObject("show");
                if (show == null) continue;
                JSONObject img = show.optJSONObject("image");
                if (img == null) continue;
                if (!normTitle(show.optString("name")).equals(normTitle(ct.title))) continue; // EXACT only
                String url = img.optString("original", "");
                if (url == null || url.isEmpty()) url = img.optString("medium", "");
                if (url != null && !url.isEmpty()) return url;
            }
            return null;
        } catch (Exception e) { return null; }
    }

    // ---- chain driver ----

    /** Runs the full chain on a background thread. Writes the sidecar. */
    private void runEnrichment(String pageTitle, String pageUrl, String hash) {
        CleanedTitle ct = cleanTitle(pageTitle, pageUrl);
        File dest = posterFile(hash);
        boolean havePoster = false;
        String imdb = null, rt = null;
        if (ct != null) {
            String key = prefs.getString(KEY_OMDB, "");
            if (key == null) key = "";
            key = key.trim();
            if (!key.isEmpty()) {
                OmdbHit h = omdbByTitle(key, ct, true);
                if (h == null && ct.year != null) h = omdbByTitle(key, ct, false);
                if (h == null) h = omdbSearch(key, ct);
                if (h != null) {
                    imdb = h.imdb; rt = h.rt;
                    if (h.poster != null) havePoster = downloadPoster(h.poster, dest);
                }
            }
            // v1.8 chain v2: (b) Wikipedia page image, then
            // (c) DuckDuckGo image search (verified portrait),
            // then (d) strict iTunes/TVmaze as last resort.
            if (!havePoster) {
                String p = wikipediaPoster(ct);
                if (p != null) havePoster = downloadPoster(p, dest);
            }
            if (!havePoster) havePoster = ddgPoster(ct, dest);
            if (!havePoster) {
                String p = itunesPoster(ct, "movie");
                if (p != null) havePoster = downloadPoster(p, dest);
            }
            if (!havePoster) {
                String p = itunesPoster(ct, "tvShow");
                if (p != null) havePoster = downloadPoster(p, dest);
            }
            if (!havePoster) {
                String p = tvmazePoster(ct);
                if (p != null) havePoster = downloadPoster(p, dest);
            }
        }
        SharedPreferences.Editor ed = prefs.edit();
        String pfx = "enr_" + hash + "_";
        if (havePoster) ed.putString(pfx + "poster", dest.getName()); else ed.remove(pfx + "poster");
        if (imdb != null) ed.putString(pfx + "imdb", imdb); else ed.remove(pfx + "imdb");
        if (rt != null) ed.putString(pfx + "rt", rt); else ed.remove(pfx + "rt");
        // Ratings alone still count as a hit - do not refetch on
        // every Home build just because no poster exists anywhere.
        ed.putString(pfx + "state", (havePoster || imdb != null || rt != null) ? "ok" : "miss");
        ed.putLong(pfx + "ts", System.currentTimeMillis());
        ed.apply();
        if (havePoster) posterCache.remove(hash); // force a fresh decode on next focus
    }

    /**
     * Kicks a lookup for one RW entry unless one is already running
     * or the entry is settled. Non-forced: state "ok" never re-runs,
     * "miss" retries only after 24h. Forced (OMDb key just saved):
     * re-runs unless the entry already has a poster AND (no key set
     * OR an IMDb score already stored).
     */
    private void kickEnrichment(final RwItem item, boolean force) {
        if (item == null || item.url == null) return;
        final String hash = rwHash(item.url);
        // v1.8: "Remove poster art" sets poster_blocked; auto-lookup
        // (including the OMDb-key-saved forced pass) skips blocked
        // entries until an explicit Re-fetch deletes the sidecar
        // (flag included) and kicks again.
        if ("1".equals(enrGet(hash, "blocked"))) return;
        String state = enrGet(hash, "state");
        if (!force) {
            if ("ok".equals(state)) return;
            if ("miss".equals(state) && System.currentTimeMillis()
                    - prefs.getLong("enr_" + hash + "_ts", 0) < ENR_MISS_RETRY_MS) return;
        } else if ("ok".equals(state) && posterFile(hash).isFile()) {
            boolean keySet = !prefs.getString(KEY_OMDB, "").trim().isEmpty();
            if (!keySet || !enrGet(hash, "imdb").isEmpty()) return;
        }
        synchronized (enrInFlight) { if (!enrInFlight.add(hash)) return; }
        final String title = item.title, url = item.url;
        new Thread(() -> {
            try { runEnrichment(title, url, hash); }
            catch (Exception ignored) {}
            synchronized (enrInFlight) { enrInFlight.remove(hash); }
            ui.post(() -> {
                // Refresh the shelf with the new poster/scores, but
                // never yank Home out from under a user who is
                // interacting with an RW tile right now - that tile
                // picks the poster up on its next focus gain, and
                // the next Home build shows it everywhere.
                if (currentScreen == homeScroll && browserDialogDepth == 0
                        && !focusInsideRwShelf()) buildHome();
            });
        }).start();
    }

    private boolean focusInsideRwShelf() {
        View f = getCurrentFocus();
        while (f != null) {
            if ("rwTile".equals(f.getTag())) return true;
            Object p = f.getParent();
            f = p instanceof View ? (View) p : null;
        }
        return false;
    }

    // v1.3: browser appearance (Dark default / Light) - chrome colors
    private FrameLayout browserBar;
    private Button appearanceBtnSettings;
    private boolean browserDark() {
        return !"light".equals(prefs.getString(KEY_APPEARANCE, "dark"));
    }
    private int tbText() { return browserDark() ? Color.parseColor("#F5F5F7") : Color.parseColor("#1D1D1F"); }
    private int tbBorder() { return browserDark() ? Color.parseColor("#48484A") : PILL_BORDER; }
    // v1.29: floating-glass nav bar palette. Dark = graphite glass,
    // light = porcelain glass; both translucent so the bar floats
    // over video/pages instead of sitting on an opaque slab. The
    // gradient runs a touch lighter at the top (the "sheen" a real
    // glass material gets from its top highlight).
    private int tbSecondary() { return browserDark() ? Color.parseColor("#A1A1A6") : Color.parseColor("#6E6E73"); }
    private int glassTop() { return browserDark() ? Color.argb(224, 34, 35, 41) : Color.argb(242, 255, 255, 255); }
    private int glassBot() { return browserDark() ? Color.argb(224, 20, 21, 25) : Color.argb(235, 242, 242, 247); }
    private int glassBorder() { return browserDark() ? Color.argb(36, 255, 255, 255) : Color.argb(20, 0, 0, 0); }
    private int glassDivider() { return browserDark() ? Color.argb(36, 255, 255, 255) : Color.argb(26, 0, 0, 0); }
    private int focusFill() { return browserDark() ? Color.argb(36, 255, 255, 255) : Color.argb(18, 0, 0, 0); }
    private int capsuleInset() { return browserDark() ? Color.argb(14, 255, 255, 255) : Color.argb(10, 0, 0, 0); }
    private Context dialogContext() {
        return browserDark() ? (Context) this
                : new ContextThemeWrapper(this, android.R.style.Theme_Material_Light_Dialog);
    }

    private FrameLayout windowRoot, contentFrame;
    private ScrollView homeScroll, catScroll, shelfScroll;
    private LinearLayout homeContent, catContent, shelfContent;
    private ScrollView settingsScroll; // v1.8: Settings page scrolls (was a fixed pane)
    private LinearLayout searchPane, settingsPane;
    private FrameLayout browserPane;
    private LinearLayout searchResumeSlot;
    private EditText searchInput;
    private LinearLayout searchResults;
    private TextView settingsStatus, browserTitle;
    private Button browserModeBtnSettings, adblockBtnSettings, cursorBtnSettings, scaleBtnSettings;
    private Button videoLockBtnSettings; // v1.13
    private Button remoteBtnSettings; // v1.19

    private WebView webView;
    private LinearLayout browserCol;
    // v1.29: floating glass bar pieces - the two segmented clusters,
    // the address capsule (a toolbar stop in place of the bare pill),
    // and the hairline dividers inside the clusters (recolored with
    // the appearance, so they are tracked).
    private LinearLayout navCluster, rightCluster;
    private FrameLayout addrCapsule;
    private final List<View> tbDividers = new ArrayList<>();
    // v1.4: auto-hiding header block (toolbar + title strip + progress)
    private LinearLayout headerBlock;
    private boolean headerHidden = false;
    private int scrollAccum = 0;   // signed accumulated scroll delta (direction-aware)
    // v1.4: D-pad CENTER hold detection (page block focus only)
    private boolean dpadCenterDown = false;
    private boolean dpadCenterHoldFired = false;
    private final Runnable dpadHoldRunnable = new Runnable() {
        @Override public void run() {
            if (dpadCenterDown && !toolbarFocus && fsView == null
                    && currentScreen == browserPane && browserDialogDepth == 0) {
                dpadCenterHoldFired = true;
                openBrowserNavMenu();
            }
        }
    };
    private FrameLayout fsContainer;
    private TextView urlPill;
    private ProgressBar pageProgress;
    private FrameLayout tbBack, tbFwd, tbReload, tbMode, tbPhone, tbGear, tbDone;
    private Button homePhoneBtn; // v1.22: Home header Phone button (rebuilt with Home)
    // v1.25: Mode/Phone are icon buttons in the browser toolbar. The
    // icons are drawn on a Canvas at build time (font glyphs for a
    // mouse / phone are not trustworthy on Fire OS) and tinted via
    // color filter, so dark/light chrome and the paired accent are
    // just re-tints of the same bitmaps.
    private BitmapDrawable modeMouseIcon, modeDpadIcon, phoneIconDrawable;
    // v1.27: the rest of the toolbar is canvas-drawn icons too
    // (Back / Forward / Reload / Settings / Done), each its own
    // drawable so Back/Forward can dim independently.
    private BitmapDrawable backIcon, fwdIcon, reloadIcon, gearIcon, doneIcon;
    // v1.25: one shared name bubble for toolbar elements, shown on
    // mouse hover (350ms dwell) and on D-pad toolbar focus (150ms).
    private TextView toolbarTipView;
    private View toolbarTipTarget;   // element the visible bubble names
    private View pendingTipTarget;   // element waiting out its dwell
    private final Runnable toolbarTipShowRunnable = new Runnable() {
        @Override public void run() {
            View v = pendingTipTarget;
            pendingTipTarget = null;
            if (v != null) showToolbarTip(v);
        }
    };
    private final List<View> toolbarViews = new ArrayList<>();
    private int toolbarIndex = 2;       // default: URL pill
    private boolean toolbarFocus = false;

    private View fsView;
    private WebChromeClient.CustomViewCallback fsCallback;
    private int fsPrevUiVisibility = View.SYSTEM_UI_FLAG_VISIBLE;
    private int fsPrevWindowFlags = 0;
    // v1.8: fullscreen geometry guard state. v1.4's one-shot
    // centering pass reduced the off-center shifts but did not
    // eliminate them: Chromium (or the player page) can re-lay-out
    // the custom view AFTER the pass runs, and nothing corrected it
    // afterward. The guard enforces geometry continuously while
    // fullscreen is active - see enforceFullscreenGeometry().
    private int fsPrevOrientation = ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED;
    private boolean fsEnforcing = false;
    private View fsFixedSurface; private int fsFixedW = -1, fsFixedH = -1;
    // v1.14 (diagnostic): per-pass correction counter for the
    // HeckYeahFS summary line, and the last CLEAN "global"-trigger
    // summary, used to suppress identical repeat clean summaries
    // from the (very chatty) global-layout listener only.
    private int fsPassFixCount = 0;
    private String fsLastGlobalCleanSummary = null;
    // v1.15: the WebView's scroll position snapshotted at FS
    // entry. Fullscreen must not move the page underneath - if
    // anything (player JS, a stray scroll) drifts it while the
    // video plays, the enforcer restores these exact values.
    private int fsSavedScrollX = 0, fsSavedScrollY = 0;
    private final ViewTreeObserver.OnGlobalLayoutListener fsGlobalLayoutListener =
            new ViewTreeObserver.OnGlobalLayoutListener() {
                @Override public void onGlobalLayout() { enforceFullscreenGeometry("global"); }
            };
    private final View.OnLayoutChangeListener fsLayoutChangeListener =
            new View.OnLayoutChangeListener() {
                @Override public void onLayoutChange(View v, int l, int t, int r, int b,
                        int ol, int ot, int or, int ob) { enforceFullscreenGeometry("layout"); }
            };
    private final Runnable fsDelay150 = new Runnable() {
        @Override public void run() { enforceFullscreenGeometry("delay150"); } };
    private final Runnable fsDelay400 = new Runnable() {
        @Override public void run() { enforceFullscreenGeometry("delay400"); fsJsProbe("delay400"); } };
    private final Runnable fsDelay900 = new Runnable() {
        @Override public void run() { enforceFullscreenGeometry("delay900"); fsJsProbe("delay900"); } };

    private CursorView cursorView;
    private boolean browserMouse = true;    // browser sub-mode (mouse default)
    private float cursorX = 480f, cursorY = 270f;
    private long centerDownAt = 0;
    private long lastMenuShownAt = 0;
    private int browserDialogDepth = 0;
    // v1.22: the currently-showing tracked dialog, so phone D-pad
    // keys can be delivered to ITS window (physical remote keys go
    // to the foreground window, not the activity's).
    private AlertDialog openDialog;

    // v1.15: VIDEO-BOUNDED SCROLL AREA (replaces v1.13's binary
    // lock; field report, Oct 28: "The browser when video is playing
    // needs to better determine the max scrollable area"). On
    // this device every page scroll is app-driven (remote only -
    // no touch, no wheel), and v1.13's poll only watched the
    // TOP document for a playing <video>, so players inside
    // same-origin iframes were never detected and the page
    // stayed fully scrollable with the video in it. Now a
    // ~750ms JS poll measures the page: the largest playing
    // <video> across the top document AND same-origin iframes
    // (rects composed into top-page CSS px), plus the CSS
    // viewport size, the document's scroll size, and the
    // current scroll position. EVERY app-initiated scroll goes
    // through doScrollBy(), which clamps the target position:
    // always to the true document extent, and - while a video
    // plays and the setting is ON - to the range that keeps
    // the video fully in view (or covering the viewport, when
    // it is larger than the viewport). Cursor MOVEMENT is never
    // clamped, and nothing scrolls in HTML5 fullscreen.
    private boolean videoFound = false;
    private float vidL, vidT, vidR, vidB; // video rect, CSS PAGE coords
    private float vidViewL, vidViewT, vidViewR, vidViewB; // same rect, CSS VIEWPORT coords
    private float vidInnerW, vidInnerH;   // CSS viewport size
    private float vidDocW, vidDocH;       // CSS document scroll size
    private long videoDataAt = 0;         // timestamp of the last good poll
    private boolean videoKickInFlight = false; // v1.21: 1px layout nudge in progress
    // v1.16: PSEUDO-fullscreen detection. Some players never
    // hand a custom view to the app (no onShowCustomView, so
    // no FS handler/guard/logs at all): the page just makes
    // its own container cover the WebView viewport. The poll
    // flags that state when the largest PLAYING video's
    // VIEWPORT rect covers >=90% of the viewport in both
    // dimensions (Java also requires fsView == null, i.e. the
    // app is NOT in a real fullscreen session). pseudoL/T/R/B
    // are CSS VIEWPORT coords (before page-offset conversion).
    private boolean pseudoFs = false;
    private long pseudoFsAt = 0;          // last poll that saw pseudo-FS
    private float pseudoL, pseudoT, pseudoR, pseudoB;
    private String pageFsElement = "";    // document.fullscreenElement tag (top doc)
    private boolean inPageFsLogged = false; // "inpage-fs" logs once per page
    private boolean videoHintShown = false; // once per navigation
    private boolean videoPollRunning = false;
    // v1.26: center-screen action feedback. One decor-level
    // ImageView flashes a big glyph (play / pause / seek -10 /
    // seek +10) YouTube-style whenever a seek or a play/pause
    // toggle lands, from EITHER remote. Glyphs are drawn once
    // into bitmaps (font glyphs are untrustworthy on Fire OS).
    // feedbackGen invalidates pending fade-outs on retrigger.
    private ImageView feedbackView;
    private Bitmap fbPlayBmp, fbPauseBmp, fbSeekBackBmp, fbSeekFwdBmp;
    private int feedbackGen = 0;

    /** Lazily drawn feedback glyphs: "play" | "pause" | "back" | "fwd". */
    private Bitmap fbBitmap(String kind) {
        switch (kind) {
            case "play":  if (fbPlayBmp == null) fbPlayBmp = drawFeedbackGlyph(kind); return fbPlayBmp;
            case "pause": if (fbPauseBmp == null) fbPauseBmp = drawFeedbackGlyph(kind); return fbPauseBmp;
            case "back":  if (fbSeekBackBmp == null) fbSeekBackBmp = drawFeedbackGlyph(kind); return fbSeekBackBmp;
            default:      if (fbSeekFwdBmp == null) fbSeekFwdBmp = drawFeedbackGlyph(kind); return fbSeekFwdBmp;
        }
    }

    private Bitmap drawFeedbackGlyph(String kind) {
        int s = dp(92);
        Bitmap bmp = Bitmap.createBitmap(s, s, Bitmap.Config.ARGB_8888);
        Canvas c = new Canvas(bmp);
        float cx = s / 2f, cy = s / 2f;
        Paint bg = new Paint(Paint.ANTI_ALIAS_FLAG);
        bg.setStyle(Paint.Style.FILL);
        bg.setColor(Color.argb(140, 0, 0, 0)); // black at ~55% alpha
        c.drawCircle(cx, cy, s / 2f, bg);
        Paint w = new Paint(Paint.ANTI_ALIAS_FLAG);
        w.setColor(Color.WHITE);
        if ("play".equals(kind)) {
            w.setStyle(Paint.Style.FILL);
            Path tri = new Path();
            tri.moveTo(cx - s * 0.15f, cy - s * 0.21f);
            tri.lineTo(cx - s * 0.15f, cy + s * 0.21f);
            tri.lineTo(cx + s * 0.25f, cy);
            tri.close();
            c.drawPath(tri, w);
        } else if ("pause".equals(kind)) {
            w.setStyle(Paint.Style.FILL);
            float bw = s * 0.13f, bh = s * 0.21f, off = s * 0.115f, rad = s * 0.03f;
            c.drawRoundRect(new RectF(cx - off - bw / 2, cy - bh, cx - off + bw / 2, cy + bh), rad, rad, w);
            c.drawRoundRect(new RectF(cx + off - bw / 2, cy - bh, cx + off + bw / 2, cy + bh), rad, rad, w);
        } else {
            // Seek: circular arc arrow + number. v1.30: rebuilt on
            // the same render-verified construction as the toolbar
            // reload - the arc stops at the head's BASE CENTER and
            // the filled head grows out of it: tip on the circle,
            // base center 0.44 rad back along the travel direction,
            // base corners radial at +-0.24r, butt cap. (v1.26 put
            // the tip 0.105s BEYOND the arc end along the tangent
            // with the base straddling the circle - a kite.)
            // Android angles: 0 deg = 3 o'clock, positive sweep =
            // clockwise (y is down). Forward (clockwise): tip at
            // 305 deg, tail at 25 deg. Back is the exact mirror:
            // tip at 235 deg, tail at 155 deg, negative sweep.
            boolean fwd = "fwd".equals(kind);
            float r = s * 0.205f;
            RectF oval = new RectF(cx - r, cy - r, cx + r, cy + r);
            w.setStyle(Paint.Style.STROKE);
            w.setStrokeWidth(s * 0.05f);
            w.setStrokeCap(Paint.Cap.BUTT);
            double tipDeg = fwd ? 305 : 235;
            double headDeg = Math.toDegrees(0.44); // head length 0.44 * r of arc
            double baseDeg = fwd ? tipDeg - headDeg : tipDeg + headDeg;
            double sweepEnd = fwd ? baseDeg : baseDeg - 360; // unwrap back's negative sweep
            if (fwd) c.drawArc(oval, 25f, (float) (sweepEnd - 25.0), false, w);
            else c.drawArc(oval, 155f, (float) (sweepEnd - 155.0), false, w);
            double tr = Math.toRadians(tipDeg), br = Math.toRadians(baseDeg);
            float px = cx + (float) (r * Math.cos(tr)), py = cy + (float) (r * Math.sin(tr));
            float bx = cx + (float) (r * Math.cos(br)), by = cy + (float) (r * Math.sin(br));
            float hb = r * 0.24f; // half the base width
            float nx = (float) Math.cos(br), ny = (float) Math.sin(br);
            Path head = new Path();
            head.moveTo(px, py);
            head.lineTo(bx + nx * hb, by + ny * hb);
            head.lineTo(bx - nx * hb, by - ny * hb);
            head.close();
            w.setStyle(Paint.Style.FILL);
            c.drawPath(head, w);
            Paint t = new Paint(Paint.ANTI_ALIAS_FLAG);
            t.setColor(Color.WHITE);
            t.setTextSize(s * 0.21f);
            t.setFakeBoldText(true);
            t.setTextAlign(Paint.Align.CENTER);
            Paint.FontMetrics fm = t.getFontMetrics();
            c.drawText(fwd ? "+10" : "\u221210", cx, cy - (fm.ascent + fm.descent) / 2, t);
        }
        return bmp;
    }

    /**
     * v1.26: flash a feedback glyph center-screen: fade/scale in
     * (~120ms), hold (~350ms), fade out (~300ms). Retriggering
     * swaps the bitmap and restarts via the generation counter.
     * Browser-gated; callers are the shared seek + toggle paths.
     */
    private void showActionFeedback(Bitmap bm) {
        if (bm == null || feedbackView == null) return;
        if (currentScreen != browserPane) return;
        final int gen = ++feedbackGen;
        feedbackView.animate().cancel();
        feedbackView.setImageBitmap(bm);
        feedbackView.setAlpha(0f);
        feedbackView.setScaleX(0.7f);
        feedbackView.setScaleY(0.7f);
        feedbackView.setVisibility(View.VISIBLE);
        feedbackView.animate().alpha(1f).scaleX(1f).scaleY(1f).setDuration(120).start();
        ui.postDelayed(() -> {
            if (gen != feedbackGen) return;
            feedbackView.animate().alpha(0f).scaleX(1.06f).scaleY(1.06f)
                    .setDuration(300)
                    .withEndAction(() -> {
                        if (gen == feedbackGen) feedbackView.setVisibility(View.GONE);
                    })
                    .start();
        }, 470); // 120 in + 350 hold
    }

    /** v1.26: kill any in-flight feedback (leaving the browser). */
    private void cancelActionFeedback() {
        feedbackGen++;
        if (feedbackView != null) {
            feedbackView.animate().cancel();
            feedbackView.setVisibility(View.GONE);
        }
    }
    private static final String VIDEO_POLL_JS =
            "(function(){try{"
            + "var IW=window.innerWidth,IH=window.innerHeight;"
            + "var best=null,bestArea=0;"
            + "function scan(doc,ox,oy){"
            + "var vs;try{vs=doc.querySelectorAll('video');}catch(e){return;}"
            + "for(var i=0;i<vs.length;i++){var v=vs[i];"
            + "try{"
            + "if(v.paused||v.ended)continue;"
            + "if(!(v.readyState>=2||v.currentTime>0))continue;"
            + "var r=v.getBoundingClientRect();"
            + "if(r.width<160||r.height<90)continue;"
            + "var L=r.left+ox,T=r.top+oy,R=r.right+ox,B=r.bottom+oy;"
            + "var ix=Math.max(0,Math.min(R,IW)-Math.max(L,0));"
            + "var iy=Math.max(0,Math.min(B,IH)-Math.max(T,0));"
            + "var vis=ix*iy,area=r.width*r.height;"
            + "if(area<=0)continue;"
            + "if(vis<0.5*area&&vis<0.5*IW*IH)continue;"
            + "if(area>bestArea){bestArea=area;best={l:L,t:T,r:R,b:B};}"
            + "}catch(e){}}"
            + "var fs;try{fs=doc.querySelectorAll('iframe');}catch(e){return;}"
            + "for(var j=0;j<fs.length;j++){var f=fs[j],cd=null;"
            + "try{cd=f.contentDocument;}catch(e){}"
            + "if(!cd)continue;"
            + "var fr;try{fr=f.getBoundingClientRect();}catch(e){continue;}"
            + "scan(cd,ox+fr.left,oy+fr.top);}"
            + "}"
            + "scan(document,0,0);"
            + "var de=document.documentElement,se=document.scrollingElement||de;"
            + "var dw=Math.max(de?de.scrollWidth:0,se?se.scrollWidth:0);"
            + "var dh=Math.max(de?de.scrollHeight:0,se?se.scrollHeight:0);"
            + "var sx=window.scrollX||window.pageXOffset||0;"
            + "var sy=window.scrollY||window.pageYOffset||0;"
            + "var fe='';try{var fsel=document.fullscreenElement||document.webkitFullscreenElement;"
            + "if(fsel)fe=fsel.tagName||'';}catch(e){}"
            + "var o={f:best?1:0,iw:IW,ih:IH,dw:dw,dh:dh,sx:sx,sy:sy,fe:fe,ps:0};"
            + "if(best){o.l=best.l+sx;o.t=best.t+sy;o.r=best.r+sx;o.b=best.b+sy;"
            + "o.vl=best.l;o.vt=best.t;o.vr=best.r;o.vb=best.b;"
            + "if((best.r-best.l)>=0.9*IW&&(best.b-best.t)>=0.9*IH)o.ps=1;}"
            + "return JSON.stringify(o);"
            + "}catch(e){return '';}})()";
    private final Runnable videoPollRunnable = new Runnable() {
        @Override public void run() {
            // Self-terminating: the loop lives only while the
            // browser is the current screen (closeBrowser lands on
            // another screen, which stops it at the next cycle).
            if (currentScreen != browserPane) { videoPollRunning = false; return; }
            if (fsView != null) clearVideoData(); // detection is inline-only
            else if (browserDialogDepth == 0 && !browserSuspended) pollVideoPlaying();
            ui.postDelayed(this, 750);
        }
    };
    private void kickVideoPoll() {
        if (videoPollRunning) return;
        videoPollRunning = true;
        ui.post(videoPollRunnable);
    }
    /** Drop all poll data (navigation, fullscreen, Done): the
     *  clamp falls back to unclamped scrolling until the next
     *  good poll lands. */
    private void clearVideoData() {
        videoFound = false;
        vidViewL = vidViewT = vidViewR = vidViewB = 0;
        videoDataAt = 0;
        pseudoFs = false; // v1.16: silent reset (navigation / real FS / Done) - no OFF log
        pseudoFsAt = 0;
    }
    private void pollVideoPlaying() {
        try {
            webView.evaluateJavascript(VIDEO_POLL_JS, value -> {
                try {
                    String s = unquoteJs(value);
                    if (s == null || s.isEmpty() || "null".equals(s)) return;
                    JSONObject o = new JSONObject(s);
                    boolean was = videoFound;
                    boolean wasPseudo = pseudoFs;
                    videoFound = o.optInt("f", 0) == 1;
                    vidInnerW = (float) o.optDouble("iw", 0);
                    vidInnerH = (float) o.optDouble("ih", 0);
                    vidDocW = (float) o.optDouble("dw", 0);
                    vidDocH = (float) o.optDouble("dh", 0);
                    if (videoFound) {
                        vidL = (float) o.optDouble("l", 0);
                        vidT = (float) o.optDouble("t", 0);
                        vidR = (float) o.optDouble("r", 0);
                        vidB = (float) o.optDouble("b", 0);
                        vidViewL = (float) o.optDouble("vl", 0);
                        vidViewT = (float) o.optDouble("vt", 0);
                        vidViewR = (float) o.optDouble("vr", 0);
                        vidViewB = (float) o.optDouble("vb", 0);
                    }
                    // v1.16: pseudo-FS state from this poll. The
                    // JS "ps" flag only measures viewport coverage;
                    // the fsView == null half of the definition
                    // lives here (the poll never runs during a real
                    // FS session anyway - the runnable clears data
                    // instead - but the check keeps the invariant
                    // explicit). Rect is CSS VIEWPORT coords.
                    pageFsElement = o.optString("fe", "");
                    boolean nowPseudo = videoFound && fsView == null
                            && o.optInt("ps", 0) == 1;
                    if (nowPseudo) {
                        pseudoL = (float) o.optDouble("vl", 0);
                        pseudoT = (float) o.optDouble("vt", 0);
                        pseudoR = (float) o.optDouble("vr", 0);
                        pseudoB = (float) o.optDouble("vb", 0);
                        pseudoFsAt = System.currentTimeMillis();
                    }
                    pseudoFs = nowPseudo;
                    videoDataAt = System.currentTimeMillis();
                    // v1.15 (provable logging): detection flips are
                    // logged under HeckYeahFS, so an EMPTY dump from
                    // now on unambiguously means this build is not
                    // the one running - a live browser session with
                    // a playing video always produces "scrolllock ON".
                    if (videoFound && !was) {
                        Log.d("HeckYeahFS", "scrolllock ON rect=["
                                + Math.round(vidL) + "," + Math.round(vidT) + ","
                                + Math.round(vidR) + "," + Math.round(vidB)
                                + "] viewport=" + Math.round(vidInnerW) + "x" + Math.round(vidInnerH)
                                + " doc=" + Math.round(vidDocW) + "x" + Math.round(vidDocH));
                    } else if (!videoFound && was) {
                        Log.d("HeckYeahFS", "scrolllock OFF");
                    }
                    // v1.16: a fullscreenElement the app was
                    // never handed (no onShowCustomView) is the
                    // smoking gun for an in-page fullscreen -
                    // log it once per page.
                    if (fsView == null && pageFsElement != null && !pageFsElement.isEmpty()
                            && !inPageFsLogged) {
                        inPageFsLogged = true;
                        Log.d("HeckYeahFS", "inpage-fs fe=" + pageFsElement
                                + " (no custom view handed to app)");
                    }
                    // v1.16: pseudo-FS transitions. On ON, force
                    // the browser header out of the way through
                    // the existing header-hide path (hideHeader)
                    // so the WebView - and therefore the page's
                    // own fullscreen overlay, which can only fill
                    // the WebView viewport - gets the full window
                    // height. (showHeader(false) in this codebase
                    // RESTORES the bar; hideHeader is the hide.)
                    // On OFF nothing special: the v1.4 auto-hide
                    // logic resumes on its own.
                    if (pseudoFs && !wasPseudo) {
                        Log.d("HeckYeahFS", "pseudofs ON rect=["
                                + Math.round(pseudoL) + "," + Math.round(pseudoT) + ","
                                + Math.round(pseudoR) + "," + Math.round(pseudoB)
                                + "] fe=" + (pageFsElement == null || pageFsElement.isEmpty()
                                        ? "none" : pageFsElement));
                        hideHeader();
                        pokeFsCursor(); // v1.17: arm the cursor fade from pseudo-FS entry
                    } else if (!pseudoFs && wasPseudo) {
                        Log.d("HeckYeahFS", "pseudofs OFF");
                        cancelFsFade(); // v1.17: restore a faded cursor, kill the timer
                    }
                    if (pseudoFs != wasPseudo) pushRemoteState(); // v1.19
                } catch (Exception ignored) {}
            });
        } catch (Exception ignored) {}
    }
    /** evaluateJavascript hands string results back as a quoted,
     *  escaped JSON string; unwrap one level so JSONObject can
     *  parse the VIDEO_POLL_JS payload. */
    private static String unquoteJs(String value) {
        if (value == null) return null;
        String s = value.trim();
        if (s.length() >= 2 && s.startsWith("\"") && s.endsWith("\"")) {
            s = s.substring(1, s.length() - 1);
            s = s.replace("\\\"", "\"").replace("\\/", "/").replace("\\\\", "\\");
        }
        return s;
    }
    private static float clampF(float v, float lo, float hi) {
        return Math.max(lo, Math.min(hi, v));
    }
    /**
     * v1.15: THE app scroll path - every app-initiated page
     * scroll (mouse edge-push, D-pad, PgUp/PgDn, FF/Rew) calls
     * this instead of touching the WebView directly. Deltas are
     * physical px; the clamp math runs in CSS px via
     * webView.getScale() against the freshest poll data:
     *
     *  - Fullscreen: no-op. The page under a fullscreen video
     *    never scrolls (the enforcer also restores the scroll
     *    position snapshotted at FS entry).
     *  - Document clamp (whenever poll data is <2s old): the
     *    target stays inside [0, (docW-innerW)*scale] x
     *    [0, (docH-innerH)*scale] - the page's true max
     *    scrollable area, instead of WebView overscroll slack.
     *  - Video clamp (additionally, while a video plays and
     *    the setting is ON): the target CSS scroll position
     *    must stay inside [vidR-innerW, vidL] x [vidB-innerH,
     *    vidT] (endpoints normalized) - for a video smaller
     *    than the viewport that is exactly the range where it
     *    stays fully visible; for one larger than the viewport
     *    it is the range where it still covers the viewport.
     *    The first truncation per page shows the hint once.
     *  - Stale or absent poll data (detection failed, page
     *    still loading): unclamped scrollBy, exactly the
     *    pre-v1.15 behavior - browsing never freezes because
     *    the poll could not see the page.
     */
    private void doScrollBy(int dxPx, int dyPx) {
        if (webView == null) return;
        if (fsView != null) return; // v1.15: fullscreen never scrolls the page
        // v1.16: pseudo-fullscreen gets the same hard freeze
        // while it is fresh - the playing video already covers
        // the viewport, so any page scroll can only shift the
        // page under/around it.
        if (pseudoFs && pseudoFsAt > 0
                && System.currentTimeMillis() - pseudoFsAt < 2000) return;
        boolean fresh = videoDataAt > 0
                && System.currentTimeMillis() - videoDataAt < 2000
                && vidDocW > 0 && vidDocH > 0 && vidInnerW > 0 && vidInnerH > 0;
        float scale = fresh ? webView.getScale() : 0f;
        if (!fresh || scale <= 0f) {
            if (dxPx != 0 || dyPx != 0) webView.scrollBy(dxPx, dyPx);
            return;
        }
        int curX = webView.getScrollX(), curY = webView.getScrollY();
        float curCssX = curX / scale, curCssY = curY / scale;
        float maxCssX = Math.max(0f, vidDocW - vidInnerW);
        float maxCssY = Math.max(0f, vidDocH - vidInnerH);
        // Document clamp.
        float tX = clampF(curCssX + dxPx / scale, 0f, maxCssX);
        float tY = clampF(curCssY + dyPx / scale, 0f, maxCssY);
        // Video clamp.
        if (videoFound && prefs.getBoolean(KEY_VIDEO_LOCK, true)) {
            float loX = Math.min(vidR - vidInnerW, vidL), hiX = Math.max(vidR - vidInnerW, vidL);
            float loY = Math.min(vidB - vidInnerH, vidT), hiY = Math.max(vidB - vidInnerH, vidT);
            float vX = clampF(tX, loX, hiX), vY = clampF(tY, loY, hiY);
            if ((vX != tX || vY != tY) && !videoHintShown) {
                videoHintShown = true;
                Toast.makeText(this, "Video playing \u2014 scrolling locked (pause to scroll)",
                        Toast.LENGTH_SHORT).show();
            }
            // Intersect with the document range (the video
            // interval can reach past it when the video sits at
            // a page edge).
            tX = clampF(vX, 0f, maxCssX);
            tY = clampF(vY, 0f, maxCssY);
        }
        int ndx = Math.round((tX - curCssX) * scale);
        int ndy = Math.round((tY - curCssY) * scale);
        if (ndx != 0 || ndy != 0) webView.scrollBy(ndx, ndy);
    }
    /** One page of scrolling for PgUp/PgDn/FF/Rew, minus a
     *  small overlap so the reader keeps their place. */
    private int pageScrollStep() {
        return Math.max(dp(48), webView.getHeight() - dp(48));
    }

    /**
     * v1.21: revive a black inline video after an overlaying
     * window closes. On this API-25 Chromium the playing video
     * is a hardware surface visible through a compositor hole
     * in the page; an AlertDialog opening/closing over the
     * activity can desync that hole, and the surface stays
     * black until the WebView changes size (which is why
     * revealing the browser bar - a WebView resize - revived
     * it). This reproduces that resize harmlessly: browserCol
     * (the WebView's direct parent) gets 1px of bottom padding
     * for ~50ms, shrinking the WebView's height by 1px, then
     * the captured padding is restored exactly. No header or
     * visibility toggling, so nothing visible changes.
     *
     * Gating: browser current, no real fullscreen, views live,
     * and the freshest poll (<2s) reporting a playing video.
     * The one exception is why="pair": a first phone pairing
     * typically happens while the pairing dialog has starved
     * the poll (the poll skips while a dialog is open), so a
     * pair kick also fires with NO poll data at all
     * (videoDataAt == 0). Stale poll data never kicks.
     */
    private void kickVideoLayer(String why) {
        if (videoKickInFlight) return;
        if (currentScreen != browserPane || fsView != null
                || webView == null || browserCol == null) return;
        long now = System.currentTimeMillis();
        boolean freshPlaying = videoFound && videoDataAt > 0
                && now - videoDataAt < 2000;
        boolean pairWithoutPoll = "pair".equals(why) && videoDataAt == 0;
        if (!freshPlaying && !pairWithoutPoll) return;
        videoKickInFlight = true;
        final int prevLeft = browserCol.getPaddingLeft();
        final int prevTop = browserCol.getPaddingTop();
        final int prevRight = browserCol.getPaddingRight();
        final int prevBottom = browserCol.getPaddingBottom();
        try {
            browserCol.setPadding(prevLeft, prevTop, prevRight, 1);
            browserCol.requestLayout();
            webView.requestLayout();
            webView.invalidate();
            Log.d("HeckYeahFS", "videokick " + why);
            browserCol.postDelayed(() -> {
                try {
                    browserCol.setPadding(prevLeft, prevTop, prevRight, prevBottom);
                    browserCol.requestLayout();
                    webView.requestLayout();
                    webView.invalidate();
                } catch (Exception ignored) {
                } finally {
                    videoKickInFlight = false;
                }
            }, 50);
        } catch (Exception ignored) {
            videoKickInFlight = false;
        }
    }
    private ScrollView currentScroll;

    private String currentCat, expandedShelfBackCat;
    private Entry currentEntry;
    private View currentScreen;

    // Browser session / Done-resume state
    // v1.4 resume fix: lastNavUrl tracks the latest navigated URL from
    // the WebView callbacks (all categories), independent of RW state.
    private String lastNavUrl = "";
    private boolean browserSuspended = false;
    private String originKind = "home";     // home | cat | search
    private String originCat = null;
    private String resumeUrl = "", resumeTitle = "", resumeApplet = "", resumeCat = "";
    private long resumeTs = 0;
    private double resumeSeek = 0;

    // RW session tracking (video launches only)
    private boolean rwActive = false;
    private String rwSource = "";
    private String sessUrl = "", sessTitle = "";
    private RwItem sessItem = null;
    private double pendingSeek = -1;

    private int dp(int v) { return Math.round(v * getResources().getDisplayMetrics().density); }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        // v1.16: app-start proof of life - every launch of
        // THIS build leaves exactly one unmistakable line, so
        // an empty HeckYeahFS dump can never again be confused
        // with "the guard saw nothing to fix".
        Log.d("HeckYeahFS", "alive version=1.32 code=33");
        prefs = getSharedPreferences(PREFS, MODE_PRIVATE);
        browserMouse = prefs.getBoolean(KEY_MOUSE, true);
        AdBlock.enabled = prefs.getBoolean(KEY_ADBLOCK, true);
        loadFonts();
        loadCatalog();
        loadRw();
        migrateEnrichmentIfNeeded(); // v1.8: drop v1-matcher posters once (chain v2)
        migrateShotsIfNeeded(); // v1.12: purge pre-v1.10 (squeezed-capture) screenshots once
        loadFavs();
        loadResume();
        buildUi();
        setupWebView();
        if (prefs.getBoolean(KEY_REMOTE, false)) startRemoteServer(); // v1.19
        showHome();
    }

    private void loadFonts() {
        try { fontReg = Typeface.createFromAsset(getAssets(), "fonts/inter_regular.ttf"); }
        catch (Exception e) { fontReg = null; }
        try { fontSemi = Typeface.createFromAsset(getAssets(), "fonts/inter_semibold.ttf"); }
        catch (Exception e) { fontSemi = null; }
        try { fontBold = Typeface.createFromAsset(getAssets(), "fonts/inter_bold.ttf"); }
        catch (Exception e) { fontBold = null; }
        if (fontBold == null) fontBold = fontSemi;
    }

    @Override
    protected void onPause() {
        super.onPause();
        // App backgrounded while browsing: don't let media keep playing.
        if (webView != null && currentScreen == browserPane && !browserSuspended) {
            try { webView.onPause(); } catch (Exception ignored) {}
        }
    }

    @Override
    protected void onResume() {
        super.onResume();
        if (webView != null && currentScreen == browserPane && !browserSuspended) {
            try { webView.onResume(); webView.resumeTimers(); } catch (Exception ignored) {}
        }
    }

    @Override
    protected void onDestroy() {
        stopRemoteServer(); // v1.19
        super.onDestroy();
    }

    @Override
    public void onBackPressed() {
        if (fsView != null) { hideFullscreen(); return; }
        if (currentScreen == browserPane) {
            if (webView != null && webView.canGoBack()) { webView.goBack(); return; }
            closeBrowser(); return;
        }
        if (currentScreen == shelfScroll) { showCategory(expandedShelfBackCat, null); return; }
        if (currentScreen == catScroll || currentScreen == searchPane || currentScreen == settingsScroll) {
            showHome(); return;
        }
        super.onBackPressed();
    }

    // ---------------------------------------------------------- UI skeleton

    private GradientDrawable tileBg(int c1, int c2, boolean focused) {
        GradientDrawable g = new GradientDrawable(GradientDrawable.Orientation.TL_BR, new int[]{c1, c2});
        g.setCornerRadius(dp(14));
        g.setStroke(focused ? dp(3) : 0, Color.WHITE);
        return g;
    }
    private GradientDrawable solidBg(int color, boolean focused) {
        GradientDrawable g = new GradientDrawable();
        g.setColor(color); g.setCornerRadius(dp(16));
        g.setStroke(focused ? dp(3) : dp(1), focused ? Color.WHITE : Color.parseColor("#48484A"));
        return g;
    }
    /** v1.3: restyle the whole browser chrome for Dark/Light, no reload. */
    private void applyBrowserAppearance() {
        // v1.29: bar + title sit on the floating glass now - the row
        // and the title line have no slab behind them.
        if (browserBar != null) browserBar.setBackgroundColor(Color.TRANSPARENT);
        if (browserTitle != null) {
            browserTitle.setBackgroundColor(Color.TRANSPARENT);
            browserTitle.setTextColor(tbSecondary());
        }
        if (urlPill != null) urlPill.setTextColor(tbText());
        if (navCluster != null) navCluster.setBackground(glassBg(20, false));
        if (rightCluster != null) rightCluster.setBackground(glassBg(20, false));
        for (View dv : tbDividers) dv.setBackgroundColor(glassDivider());
        if (browserCol != null)
            browserCol.setBackgroundColor(browserDark() ? Color.BLACK : SAFARI_BG);
        for (View v : toolbarViews) {
            if (v instanceof TextView) ((TextView) v).setTextColor(tbText());
            v.setBackground(toolbarBg(v, false)); // v1.29: capsule / segment / ghost / Done
        }
        retintToolbarIcons(); // v1.27: Back/Fwd/Reload/Gear follow the appearance; Done stays red
        updateModeBtn();   // v1.25: re-tint the Mode icon for the new appearance
        refreshPhoneBtn(); // v1.25: same for the Phone icon (keeps any paired accent)
        applyPairCardTheme(); // v1.28: the pairing card follows the chrome too
        styleToolbarFocus();
    }

    /**
     * v1.4 clip fix: Android parents clip scaled children by default,
     * so a focused tile's white border was sliced wherever it touched
     * its row/shelf/ScrollView edge (device photo of Home). Every menu
     * container now sets clipChildren=false + clipToPadding=false
     * (see unclip), shelves carry >=10dp padding (>= the ~8dp max
     * scale growth), and a focused tile is raised via translationZ
     * (v1.5; was bringToFront in v1.4 - see focusIntoView) and
     * scrolled fully into view including its growth margin. Page
     * side padding is 40dp, far beyond the growth, so no tile sits
     * flush against the physical screen edge and the scale can stay.
     */
    static void unclip(ViewGroup g) {
        if (g == null) return;
        g.setClipChildren(false);
        g.setClipToPadding(false);
    }

    private void focusIntoView(View view) {
        // v1.5 REGRESSION FIX: no bringToFront() here. In a
        // LinearLayout, bringToFront() physically REORDERS the
        // child to the end of the parent's child list, so every
        // focus gain moved the focused tile to the end of its row
        // (Field report on v1.4: pressing LEFT swapped tile positions and
        // focus stayed pinned on the right). translationZ raises
        // the tile in DRAW order only - layout order and focus
        // order are never touched. Callers reset it to 0 on
        // focus loss.
        view.setTranslationZ(dp(8));
        int g = dp(10);
        Rect r = new Rect(-g, -g,
                view.getWidth() + g, view.getHeight() + g);
        view.requestRectangleOnScreen(r, false);
    }

    private void styleFocusable(final View v, final int c1, final int c2, final boolean gradient) {
        v.setFocusable(true); v.setFocusableInTouchMode(false);
        v.setOnFocusChangeListener((view, has) -> {
            view.setBackground(gradient ? tileBg(c1, c2, has) : solidBg(c1, has));
            float s = has ? 1.03f : 1f;
            view.animate().scaleX(s).scaleY(s).setDuration(120).start();
            if (has) focusIntoView(view);
            else view.setTranslationZ(0); // v1.5: undo the draw-order raise
        });
    }

    private TextView tv(String text, float size, int color, boolean bold) {
        TextView t = new TextView(this);
        t.setText(text); t.setTextSize(size); t.setTextColor(color);
        t.setTypeface(tf(bold));
        return t;
    }

    /** Menu button: rounded pill, generous padding (~40dp tall). */
    private Button safariBtn(String label, boolean dark) {
        Button b = new Button(this);
        b.setText(label); b.setAllCaps(false); b.setTextSize(15);
        b.setTypeface(tf(false));
        b.setTextColor(dark ? SAFARI_TEXT : TEXT);
        b.setPadding(dp(16), dp(9), dp(16), dp(9));
        b.setMinHeight(dp(40));
        b.setBackground(solidBg(dark ? Color.parseColor("#E5E5EA") : CARD, false));
        styleFocusable(b, dark ? Color.parseColor("#E5E5EA") : CARD, 0, false);
        return b;
    }

    private LinearLayout.LayoutParams gapLp(boolean first) {
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        lp.setMargins(first ? 0 : dp(12), 0, 0, 0);
        return lp;
    }
    private LinearLayout.LayoutParams stackLp() {
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        lp.setMargins(0, dp(10), 0, 0);
        return lp;
    }

    private void buildUi() {
        windowRoot = new FrameLayout(this);
        contentFrame = new FrameLayout(this);
        contentFrame.setBackgroundColor(BG);
        unclip(windowRoot); unclip(contentFrame); // v1.4: focus borders must survive to the root
        windowRoot.addView(contentFrame, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));

        homeScroll = new ScrollView(this); homeContent = pageColumn();
        unclip(homeScroll); homeScroll.addView(homeContent); contentFrame.addView(homeScroll);
        catScroll = new ScrollView(this); catContent = pageColumn();
        unclip(catScroll); catScroll.addView(catContent); contentFrame.addView(catScroll);
        shelfScroll = new ScrollView(this); shelfContent = pageColumn();
        unclip(shelfScroll); shelfScroll.addView(shelfContent); contentFrame.addView(shelfScroll);
        searchPane = buildSearchPane(); contentFrame.addView(searchPane);
        // v1.8: the Settings column lives inside a ScrollView, built
        // exactly like the other menu screens (homeScroll & friends:
        // unclip'ed ScrollView wrapping a pageColumn()), because the
        // rows now run past the bottom of the TV - in v1.7 the fixed
        // pane left the lower rows unreachable and unselectable.
        settingsScroll = new ScrollView(this); settingsPane = buildSettingsPane();
        unclip(settingsScroll); settingsScroll.addView(settingsPane); contentFrame.addView(settingsScroll);
        browserPane = buildBrowserPane(); contentFrame.addView(browserPane);

        setContentView(windowRoot);

        // Fullscreen video + cursor live at the DECOR level (v1.2). The
        // HTML5 custom view must cover the whole window - in v1.1 the
        // container was a child of the browser pane, so the video kept
        // its in-page size - and the cursor must be able to sit ABOVE
        // the video. fsContainer first, cursor last (topmost).
        ViewGroup decor = (ViewGroup) getWindow().getDecorView();
        fsContainer = new FrameLayout(this);
        fsContainer.setBackgroundColor(Color.BLACK);
        fsContainer.setFocusable(false);
        fsContainer.setFocusableInTouchMode(false);
        fsContainer.setVisibility(View.GONE);
        FrameLayout.LayoutParams fsInitLp = new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT);
        fsInitLp.gravity = Gravity.FILL; // v1.15: same anchor the enforcer asserts
        decor.addView(fsContainer, fsInitLp);

        cursorView = new CursorView(this);
        cursorView.setVisibility(View.GONE);
        decor.addView(cursorView, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));

        // v1.26: center action-feedback flash (seek / play / pause),
        // also at the DECOR level so it renders identically over the
        // inline browser, real fullscreen and pseudo-fullscreen.
        // Added after the cursor so it sits topmost; it never takes
        // touches or focus.
        feedbackView = new ImageView(this);
        feedbackView.setVisibility(View.GONE);
        feedbackView.setClickable(false);
        feedbackView.setFocusable(false);
        FrameLayout.LayoutParams fbLp = new FrameLayout.LayoutParams(dp(92), dp(92));
        fbLp.gravity = Gravity.CENTER;
        decor.addView(feedbackView, fbLp);
    }

    private LinearLayout pageColumn() {
        LinearLayout l = new LinearLayout(this);
        l.setOrientation(LinearLayout.VERTICAL);
        l.setPadding(dp(40), dp(18), dp(40), dp(28)); // >=40dp sides: focused tiles can scale without leaving the screen
        unclip(l); // v1.4: don't clip focused tiles at the column's bounds
        return l;
    }

    private void showScreen(View v) {
        View[] all = {homeScroll, catScroll, shelfScroll, searchPane, settingsScroll, browserPane};
        for (View x : all) x.setVisibility(x == v ? View.VISIBLE : View.GONE);
        currentScreen = v;
        currentScroll = v instanceof ScrollView ? (ScrollView) v : null;
        if (v == browserPane) {
            kickVideoPoll(); // v1.13: (re)start the video poll; it self-stops off-browser
            refreshPhoneBtn(); // v1.20: pairing tint can't go stale across screens
            maybeAutoPairPopup(); // auto pairing card, once per browser session (v1.27 popup; v1.28 card)
        } else {
            cancelAutoPairPopup(); // no pending/visible auto pairing card off-browser
            hideToolbarTip(); // v1.25: tooltips exist only over the browser toolbar
            cancelActionFeedback(); // v1.26: the center flash is browser-only
        }
        if (v == settingsScroll) {
            refreshSettingsLabels();
            // v1.8: entering Settings lands focus on the FIRST
            // settings row's control (the header "‹ Home" stays one
            // UP away). Posted: the pane must be visible + laid out
            // before requestFocus can take effect.
            settingsScroll.post(() -> {
                if (browserModeBtnSettings != null) browserModeBtnSettings.requestFocus();
            });
        }
        refreshCursor("screen"); // v1.32 (was updateCursorVisibility)
        // v1.22: EVERY screen change pushes remote state immediately
        // (previously pushes happened only on navigation callbacks,
        // browser open/close and the 2.5s heartbeat), so a paired
        // phone's trackpad/D-pad mode follows the TV without lag.
        // No-ops until the server exists.
        pushRemoteState();
    }

    /** Mouse cursor exists ONLY inside the browser, in mouse sub-mode. */
    private void updateCursorVisibility() {
        refreshCursor("update");
    }

    // v1.32: what refreshCursor last asserted - lets it tell a
    // routine show/hide from a RESCUE (the view disagreed with the
    // state we believed it was in).
    private boolean cursorAssertedShown = false;

    /**
     * v1.32: the cursor's single SELF-HEALING gate (replaces the
     * old updateCursorVisibility body). Kurt: toggling the browser
     * input mode from the nav-bar Mode button while a phone was
     * connected killed the cursor until an app restart. The old
     * gate only ever set visibility - it never looked at ALPHA.
     * The fullscreen idle fade is the cursor's only alpha writer,
     * and its restore paths (pokeFsCursor / cancelFsFade) are
     * both conditional on the cursorFaded flag, so any divergence
     * between that flag and the view's real alpha - an animator
     * completing or being cancelled across a mode / dialog /
     * pairing transition - left a VISIBLE view sitting at alpha 0
     * with nothing that would ever restore it (moveCursor only
     * calls pokeFsCursor, which no-ops when cursorFaded is
     * false), and setBrowserMouse(true) - the exact transition in
     * Kurt's report - never cancelled a fade animation or
     * restored alpha either. On top of that, the phone-connected
     * transitions (remoteSessionsChanged, dialog show/dismiss,
     * pairing card show/hide) never re-asserted the cursor at all.
     *
     * refreshCursor recomputes the truth from (browser current &&
     * mouse mode): when the cursor should show, it forces VISIBLE,
     * cancels any stray alpha animation and restores alpha 1 -
     * UNLESS the fsLike idle fade legitimately owns the alpha
     * (fsLikeActive() && cursorFaded) - re-asserts the decor
     * z-order (cursor above content/fsContainer, the v1.26
     * feedback flash back on top), and invalidates. When it
     * should hide, GONE. HeckYeahFS logs ONLY on change or rescue:
     * "cursor show/hide/rescue why=<reason>".
     */
    private void refreshCursor(String why) {
        if (cursorView == null) return;
        boolean show = currentScreen == browserPane && browserMouse;
        if (!show) {
            if (cursorView.getVisibility() == View.VISIBLE)
                Log.d("HeckYeahFS", "cursor hide why=" + why);
            cursorView.setVisibility(View.GONE);
            cursorAssertedShown = false;
            return;
        }
        boolean wasGone = cursorView.getVisibility() != View.VISIBLE;
        boolean legitFade = fsLikeActive() && cursorFaded;
        boolean alphaBad = !legitFade && cursorView.getAlpha() < 0.99f;
        if (alphaBad) {
            // A fade animator (or its cancel) left the alpha
            // somewhere the flag says it shouldn't be: kill the
            // animator and put the alpha back, or this cursor
            // would stay invisible while marked VISIBLE forever.
            cursorView.animate().cancel();
            cursorView.setAlpha(1f);
        }
        if (wasGone) {
            Log.d("HeckYeahFS", (cursorAssertedShown
                    ? "cursor rescue why=" : "cursor show why=") + why);
        } else if (alphaBad) {
            Log.d("HeckYeahFS", "cursor rescue why=" + why);
        }
        cursorView.setVisibility(View.VISIBLE);
        cursorView.bringToFront();
        if (feedbackView != null) feedbackView.bringToFront(); // v1.26 flash stays topmost
        cursorView.invalidate();
        cursorAssertedShown = true;
    }

    // ---------------------------------------------------------- Resume bar (Back to page)

    private boolean hasResume() {
        loadResume(); // v1.4: in sync with SharedPreferences, always
        return browserSuspended || (resumeUrl != null && !resumeUrl.isEmpty());
    }

    /**
     * Prominent return button pinned to the TOP of Home/category/search.
     * v1.9: the returned block is [bar] + [hint] + [preview area].
     * While the BAR has focus, the last screenshot of the page it
     * resumes to expands open underneath; on focus loss it
     * collapses. The height is animated with a ValueAnimator on
     * the preview's LayoutParams (a pure scaleY leaves a
     * full-height transparent hole in the scroll content and the
     * rows below do not move up - height animation is what makes
     * this a real expand/collapse inside the ScrollViews). The
     * bar sits at the very top of each screen, so growing downward
     * from it never needs extra scrolling: focusIntoView() on the
     * bar already ran, and the preview unfolds into the space the
     * page scrolls through. No shot file = no expansion, and the
     * bar behaves exactly as in v1.8.
     */
    private View resumeBar() {
        loadResume(); // v1.4: always build the bar from the CURRENT persisted values
        final String targetUrl = resumeUrl;
        String label = "\u2039  Back to " + (resumeTitle == null || resumeTitle.isEmpty()
                ? hostOf(resumeUrl) : resumeTitle);
        Button b = new Button(this);
        b.setText(label); b.setAllCaps(false); b.setTextSize(17);
        b.setTextColor(Color.WHITE); b.setTypeface(tf(true));
        b.setPadding(dp(20), dp(12), dp(20), dp(12));
        b.setMinHeight(dp(48));
        GradientDrawable g = new GradientDrawable();
        g.setColor(ACCENT); g.setCornerRadius(dp(14));
        b.setBackground(g);
        b.setFocusable(true); b.setFocusableInTouchMode(false);
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        lp.setMargins(0, 0, 0, dp(14));
        b.setLayoutParams(lp);
        b.setOnClickListener(v -> resumeBrowser());
        TextView hint = tv("Paused where you left it \u2022 " + hostOf(resumeUrl), 12, DIM, false);
        // v1.9 preview area: starts GONE at height 0; only ever
        // shown when a resume shot exists for the current target.
        final ImageView preview = new ImageView(this);
        preview.setScaleType(ImageView.ScaleType.CENTER_CROP);
        final Bitmap shotBm = resumeShotBitmap();
        final int previewW = dp(480), previewH = dp(270);
        LinearLayout.LayoutParams plp = new LinearLayout.LayoutParams(previewW, 0);
        plp.setMargins(0, dp(10), 0, 0);
        preview.setLayoutParams(plp);
        preview.setVisibility(View.GONE);
        preview.setAlpha(0f);
        if (shotBm != null) preview.setImageBitmap(shotBm);
        final ValueAnimator[] previewAnim = {null};
        final int[] previewGen = {0};
        b.setOnFocusChangeListener((view, has) -> {
            if (has) focusIntoView(view); // v1.4: keep the whole focused bar visible
            else view.setTranslationZ(0); // v1.5: undo the draw-order raise
            GradientDrawable gg = new GradientDrawable();
            gg.setColor(ACCENT); gg.setCornerRadius(dp(14));
            gg.setStroke(has ? dp(3) : 0, Color.WHITE);
            view.setBackground(gg);
            float s = has ? 1.03f : 1f;
            view.animate().scaleX(s).scaleY(s).setDuration(120).start();
            // v1.9: expand/collapse the hover preview with the
            // bar's focus. Rapid in/out: each transition bumps the
            // generation, cancels the running animator, starts
            // from the CURRENT height, and stale animators'
            // end-actions check the generation - so the preview
            // can never be left stuck half-open or visible.
            if (shotBm != null) {
                final int gen = ++previewGen[0];
                if (previewAnim[0] != null) previewAnim[0].cancel();
                int from = preview.getLayoutParams().height;
                int to = has ? previewH : 0;
                if (has) preview.setVisibility(View.VISIBLE);
                ValueAnimator va = ValueAnimator.ofInt(from, to);
                va.setDuration(has ? 200 : 150);
                va.setInterpolator(new DecelerateInterpolator());
                va.addUpdateListener(a -> {
                    int val = (Integer) a.getAnimatedValue();
                    ViewGroup.LayoutParams p = preview.getLayoutParams();
                    p.height = val;
                    preview.setLayoutParams(p);
                    preview.setAlpha(previewH == 0 ? 1f : (float) val / previewH);
                });
                va.addListener(new AnimatorListenerAdapter() {
                    @Override public void onAnimationEnd(android.animation.Animator a) {
                        if (previewGen[0] != gen) return; // superseded by a newer transition
                        if (has) {
                            ViewGroup.LayoutParams p = preview.getLayoutParams();
                            p.height = previewH; preview.setLayoutParams(p);
                            preview.setAlpha(1f);
                        } else {
                            preview.setVisibility(View.GONE);
                            preview.setAlpha(0f);
                        }
                    }
                });
                previewAnim[0] = va;
                va.start();
            }
        });
        LinearLayout wrap = new LinearLayout(this); wrap.setOrientation(LinearLayout.VERTICAL);
        unclip(wrap); // v1.4 chain: the expanding preview must not be clipped by its own block
        wrap.addView(b); wrap.addView(hint); wrap.addView(preview);
        return wrap;
    }

    /**
     * v1.4 resume fix. ROOT CAUSE of the stuck "Back to" button: the
     * resume fields had exactly ONE writer - closeBrowser()'s finalize
     * - and its fallback wrote back sessUrl, a field the live-resume
     * path itself seeds FROM the saved resume values, so a resumed
     * session could re-save its own starting page forever; nothing
     * updated the target during browsing or when a new entry opened.
     * Now updateResume() is called at EVERY navigation point
     * (openEntry/openRw, onPageFinished, doUpdateVisitedHistory,
     * onReceivedTitle, and the authoritative Done snapshot) and
     * always overwrites + persists immediately - no write-once guard.
     */
    private void updateResume(String url, String title, boolean resetSeek) {
        if (url != null && url.startsWith("http")) {
            if (resetSeek && !url.equals(resumeUrl)) resumeSeek = 0;
            resumeUrl = url;
            lastNavUrl = url;
        }
        if (title != null && !title.trim().isEmpty() && !title.equals(resumeUrl))
            resumeTitle = title.trim();
        resumeApplet = currentEntry != null ? currentEntry.name : rwSource;
        if (currentEntry != null) resumeCat = currentEntry.cat;
        resumeTs = System.currentTimeMillis();
        saveResume();
    }

    private void saveResume() {
        String s = cleanField(resumeUrl) + SEP + cleanField(resumeTitle) + SEP
                + cleanField(resumeApplet) + SEP + cleanField(resumeCat) + SEP
                + resumeTs + SEP + resumeSeek;
        prefs.edit().putString(KEY_RESUME, s).apply();
    }
    private void loadResume() {
        String raw = prefs.getString(KEY_RESUME, "");
        if (raw == null || raw.isEmpty()) return;
        String[] p = raw.split(SEP, -1);
        if (p.length >= 4) {
            resumeUrl = p[0]; resumeTitle = p[1]; resumeApplet = p[2]; resumeCat = p[3];
            try { resumeTs = Long.parseLong(p[4]); } catch (Exception ignored) {}
            try { resumeSeek = Double.parseDouble(p[5]); } catch (Exception ignored) {}
        }
    }
    private static String cleanField(String s) {
        return s == null ? "" : s.replace(SEP, " ").replace("\n", " ");
    }

    // ------------------------------------------- v1.9: resume hover-preview shot
    //
    // The Back-to bar shows a preview of the page it resumes to
    // while it has focus. The image is the LAST SCREEN of the
    // suspended browser session, captured for EVERY session (the
    // RW shots are video-category-only, so they cannot serve
    // here). One file, overwritten on every Done, so it always
    // pairs with the resume target updateResume() saved at the
    // same moment. Same honest limit as the RW shots: if the
    // last screen was fullscreen video, the capture may be black
    // (protected surfaces do not composite into a view draw).

    private Bitmap resumeShotBm;
    private long resumeShotStamp = -1;

    private File resumeShotFile() { return new File(getFilesDir(), "resume_shot.jpg"); }

    /** Decoded preview bitmap, or null when no shot exists. Cached by file stamp. */
    private Bitmap resumeShotBitmap() {
        File f = resumeShotFile();
        if (!f.isFile()) { resumeShotBm = null; resumeShotStamp = -1; return null; }
        long stamp = f.lastModified() ^ (f.length() << 1);
        if (resumeShotBm == null || stamp != resumeShotStamp) {
            try { resumeShotBm = BitmapFactory.decodeFile(f.getAbsolutePath()); }
            catch (Exception ignored) { resumeShotBm = null; }
            resumeShotStamp = stamp;
        }
        return resumeShotBm;
    }

    private void deleteResumeShot() {
        try { File f = resumeShotFile(); if (f.isFile()) f.delete(); }
        catch (Exception ignored) {}
        resumeShotBm = null; resumeShotStamp = -1;
    }

    /**
     * v1.10: strict viewport capture, shared by the resume shot
     * and the RW shots. The bitmap is the WebView's exact
     * on-screen pixel size and webView.draw() runs on an
     * UNTRANSFORMED canvas - no canvas scale/translate, no
     * content-height sizing, no scrolling, no re-layout - so the
     * result is a pixel-faithful copy of the visible region at
     * its current scroll position, exactly what the user sees.
     *
     * The pre-v1.10 code instead created the bitmap at the
     * already-shrunk size and called cv.scale() before draw(),
     * letting the WebView rasterize its document through that
     * shrink transform; the stored image came out as a
     * re-rendered squeeze of page content (a device photo showed
     * content from well past the visible area) instead of the
     * viewport itself. Shrinking now happens AFTER capture, on
     * the finished bitmap, where it cannot change the framing.
     *
     * Returns null when there is nothing to capture (no WebView
     * or a zero-size view). Caller owns the returned bitmap.
     */
    private Bitmap captureViewportBitmap() {
        if (webView == null) return null;
        int w = webView.getWidth(), h = webView.getHeight();
        if (w <= 0 || h <= 0) return null;
        Bitmap bm = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888);
        webView.draw(new Canvas(bm));
        return bm;
    }

    /**
     * Pure image resample of a finished capture to at most maxW
     * wide, aspect preserved, never upscaled. Returns bm itself
     * when no scaling is needed; otherwise returns a new bitmap
     * and recycles bm. Caller owns the returned bitmap.
     */
    private Bitmap downscaleToWidth(Bitmap bm, int maxW) {
        int w = bm.getWidth();
        if (w <= maxW) return bm;
        int nh = Math.max(1, Math.round(bm.getHeight() * (maxW / (float) w)));
        Bitmap small = Bitmap.createScaledBitmap(bm, maxW, nh, true);
        if (small != bm) bm.recycle();
        return small;
    }

    /**
     * Capture the WebView's current viewport (~640px wide, JPEG)
     * as the resume preview, via the strict v1.10 viewport
     * capture; must be called while the browser is still on
     * screen (an invisible WebView may draw blank), i.e. at the
     * top of closeBrowser().
     */
    private void captureResumeShot() {
        if (webView == null) return;
        try {
            Bitmap bm = captureViewportBitmap();
            if (bm == null) { deleteResumeShot(); return; }
            bm = downscaleToWidth(bm, 640);
            FileOutputStream fos = new FileOutputStream(resumeShotFile());
            bm.compress(Bitmap.CompressFormat.JPEG, 75, fos);
            fos.flush(); fos.close();
            bm.recycle();
            resumeShotBm = null; resumeShotStamp = -1; // re-decode lazily on next bar build
        } catch (Exception ignored) {}
    }

    // ---------------------------------------------------------- Home

    private void showHome() {
        buildHome();
        showScreen(homeScroll);
        homeScroll.scrollTo(0, 0);
    }

    private void buildHome() {
        homeContent.removeAllViews();
        // v1.6 focus fix: locals for the explicit DOWN wiring at the
        // end of this method (first RW tile / first category tile).
        View firstRwTile = null, firstCatTile = null;
        if (hasResume()) homeContent.addView(resumeBar());
        LinearLayout head = new LinearLayout(this);
        head.setOrientation(LinearLayout.HORIZONTAL); head.setGravity(Gravity.CENTER_VERTICAL);
        LinearLayout titles = new LinearLayout(this); titles.setOrientation(LinearLayout.VERTICAL);
        TextView logo = tv("HeckYeah TV", 34, TEXT, true);
        TextView sub = tv("freemediaheckyeah  \u2022  " + catalog.size() + " sites", 14, DIM, false);
        titles.addView(logo); titles.addView(sub);
        head.addView(titles, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        Button bSearch = safariBtn("\u2315 Search", false);
        // v1.6: the Search button is Home's only focusable search
        // control (the EditText lives on the separate Search screen);
        // it needs a real id so the RW shelf can point UP at it and
        // it can point DOWN at the first tile (wired at the end).
        bSearch.setId(View.generateViewId());
        bSearch.setOnClickListener(v -> showSearch());
        // v1.22: pairing from the main menu - Phone sits BETWEEN
        // Search and Settings (Kurt's placement) and opens the same
        // pairing panel as the browser toolbar button (which offers
        // its inline Turn-on when the server is off - nothing about
        // the panel is browser-specific; its dialog is tracked via
        // trackDialog and the video kick no-ops off-browser).
        homePhoneBtn = safariBtn("Phone", false);
        homePhoneBtn.setOnClickListener(v -> openRemotePanel());
        Button bSettings = safariBtn("\u2699 Settings", false);
        bSettings.setOnClickListener(v -> showScreen(settingsScroll));
        head.addView(bSearch, gapLp(false));
        head.addView(homePhoneBtn, gapLp(false));
        head.addView(bSettings, gapLp(false));
        refreshPhoneBtn(); // v1.22: a fresh Home build re-applies the paired tint
        homeContent.addView(head);

        // RW: Recently Watched is the FIRST shelf when non-empty.
        if (!rw.isEmpty()) {
            homeContent.addView(shelfHeader("Recently Watched", "Exact episode pages from Movies / TV"));
            HorizontalScrollView hs = new HorizontalScrollView(this);
            hs.setHorizontalScrollBarEnabled(false);
            // v1.4: >=10dp shelf padding on ALL sides (RW tiles sit at
            // the top of Home - their top border clipped here before)
            // and no clipping anywhere in the shelf chain.
            hs.setPadding(dp(10), dp(10), dp(10), dp(10)); hs.setClipToPadding(false);
            unclip(hs);
            LinearLayout row = new LinearLayout(this); row.setOrientation(LinearLayout.HORIZONTAL);
            unclip(row);
            for (final RwItem item : rw) {
                View t = rwTile(item);
                if (firstRwTile == null) {
                    // v1.6: the FIRST (leftmost) RW tile is the
                    // deterministic entry point into this shelf.
                    firstRwTile = t;
                    if (t.getId() == View.NO_ID) t.setId(View.generateViewId());
                }
                row.addView(t);
            }
            // v1.10: the Clear utility card is the LAST tile in
            // the row, after the final entry (Requested correction -
            // v1.9 had it leftmost). Shelf order is now
            // [entry1 ... entryN, Clear], so a native RIGHT-walk
            // traverses every entry and reaches Clear last. It is
            // still NOT the v1.6 entry point: firstRwTile above
            // captures the first rwTile(), so Search DOWN keeps
            // landing on the first content entry.
            View clearCard = rwClearCard(rw.size());
            clearCard.setNextFocusUpId(bSearch.getId());
            row.addView(clearCard);
            hs.addView(row); homeContent.addView(hs);
            // v1.6: belt-and-braces vs focus memory - always enter
            // the shelf scrolled to its left edge.
            hs.post(() -> hs.scrollTo(0, 0));
        }

        if (!favKeys.isEmpty()) {
            homeContent.addView(shelfHeader("Favorites", "Your pinned sites"));
            HorizontalScrollView hs = new HorizontalScrollView(this);
            hs.setHorizontalScrollBarEnabled(false);
            hs.setPadding(dp(10), dp(10), dp(10), dp(10)); hs.setClipToPadding(false);
            unclip(hs);
            LinearLayout row = new LinearLayout(this); row.setOrientation(LinearLayout.HORIZONTAL);
            unclip(row);
            for (String k : new ArrayList<>(favKeys)) {
                String[] p = k.split(SEP, -1);
                if (p.length < 3) continue;
                Entry e = findEntry(p[1]);
                if (e == null) e = new Entry(p[2], "", p[0], p[1], catTitle(p[2]), false, "");
                row.addView(appletTile(e, true));
            }
            hs.addView(row); homeContent.addView(hs);
        }

        homeContent.addView(shelfHeader("Categories", "Tap a category - shelves inside follow FMHY headers"));
        for (int i = 0; i < CATS.length; i += 3) {
            LinearLayout row = new LinearLayout(this); row.setOrientation(LinearLayout.HORIZONTAL);
            unclip(row); // v1.4: first/last column tiles must not lose their side borders
            for (int j = i; j < Math.min(i + 3, CATS.length); j++) {
                LinearLayout.LayoutParams clp = new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f);
                clp.setMargins(j == i ? 0 : dp(6), dp(6),
                        j == Math.min(i + 3, CATS.length) - 1 ? 0 : dp(6), dp(6)); // 12dp gaps between tiles + rows
                View ct = catTile(CATS[j]);
                if (firstCatTile == null) {
                    // v1.6: leftmost tile of the first category row -
                    // the DOWN target when the RW shelf is absent.
                    firstCatTile = ct;
                    if (ct.getId() == View.NO_ID) ct.setId(View.generateViewId());
                }
                row.addView(ct, clp);
            }
            homeContent.addView(row);
        }
        // v1.6 focus fix: deterministic DOWN out of the Search
        // button. Field report on v1.5: resume -> DOWN -> Search -> DOWN
        // landed on a RIGHT-side RW tile (Android focus memory
        // restores the shelf's last focused descendant, and
        // geometry-based FocusFinder from the wide header row is
        // not guaranteed to pick the first tile either). Explicit
        // nextFocus ids bypass both: DOWN from Search ALWAYS enters
        // the shelf at its first (leftmost) tile, and UP from that
        // tile returns to Search. Ids are generated fresh in THIS
        // build pass and wired here, so no id can point at a view
        // from a previous buildHome(). No ids are wired AMONG the
        // RW tiles - Left/Right inside the shelf stays native.
        View downTarget = firstRwTile != null ? firstRwTile : firstCatTile;
        if (downTarget != null) bSearch.setNextFocusDownId(downTarget.getId());
        if (firstRwTile != null) firstRwTile.setNextFocusUpId(bSearch.getId());
        // v1.7 trigger (ii): any RW entry with no lookup state (or
        // a stale miss) gets a lookup kicked from here. Throttling
        // lives inside kickEnrichment: the in-flight set dedupes,
        // "ok" never re-runs, "miss" waits 24h - so rebuilding
        // Home can never start a lookup storm.
        for (RwItem item : new ArrayList<>(rw)) kickEnrichment(item, false);
        TextView foot = tv("Menus use D-pad  \u2022  Mouse lives only in the browser (hold OK or press Menu in the browser for the Browser Menu)  \u2022  Long-press a tile for mirrors / favorites  \u2022  Update the catalog in Settings", 12, DIM, false);
        foot.setPadding(0, dp(14), 0, 0);
        homeContent.addView(foot);
    }

    private LinearLayout shelfHeader(String title, String subText) {
        LinearLayout l = new LinearLayout(this); l.setOrientation(LinearLayout.VERTICAL);
        l.setPadding(0, dp(18), 0, dp(8));
        l.addView(tv(title, 22, TEXT, true));
        if (subText != null) l.addView(tv(subText, 13, DIM, false));
        return l;
    }

    private View catTile(final String[] c) {
        LinearLayout t = new LinearLayout(this); t.setOrientation(LinearLayout.VERTICAL);
        t.setPadding(dp(16), dp(14), dp(16), dp(14));
        int c1 = Color.parseColor(c[5]), c2 = Color.parseColor(c[6]);
        t.setBackground(tileBg(c1, c2, false));
        styleFocusable(t, c1, c2, true);
        TextView glyph = tv(c[4], 30, Color.WHITE, true);
        TextView name = tv(c[1], 19, Color.WHITE, true);
        TextView desc = tv(c[2], 12, Color.parseColor("#E5E5EA"), false);
        desc.setMaxLines(2); desc.setEllipsize(TextUtils.TruncateAt.END);
        int count = 0; for (Entry e : catalog) if (e.cat.equals(c[0])) count++;
        TextView meta = tv(count + " sites", 11, Color.parseColor("#D1D1D6"), false);
        t.addView(glyph); t.addView(name); t.addView(desc); t.addView(meta);
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        lp.setMargins(dp(6), dp(6), dp(6), dp(6)); t.setLayoutParams(lp);
        t.setOnClickListener(v -> showCategory(c[0], null));
        return t;
    }

    private View rwTile(final RwItem item) {
        LinearLayout t = new LinearLayout(this); t.setOrientation(LinearLayout.VERTICAL);
        t.setPadding(dp(14), dp(12), dp(14), dp(12));
        t.setBackground(solidBg(Color.parseColor("#3A3A3C"), false));
        TextView title = tv(item.title, 15, TEXT, true);
        title.setMaxLines(2); title.setEllipsize(TextUtils.TruncateAt.END);
        String when = new SimpleDateFormat("MMM d, h:mm a", Locale.US).format(new Date(item.ts));
        t.addView(title);
        t.addView(tv("via " + item.source + "  \u2022  " + when, 12, DIM, false));
        t.addView(tv(item.url, 11, ACCENT, false));
        // v1.3: screenshot of the last screen, shown BELOW the header /
        // link info only while the tile is focused ("hovering").
        final ImageView shotView = new ImageView(this);
        shotView.setScaleType(ImageView.ScaleType.CENTER_CROP);
        shotView.setVisibility(View.GONE);
        LinearLayout.LayoutParams slp = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, dp(160));
        slp.setMargins(0, dp(8), 0, 0);
        shotView.setLayoutParams(slp);
        t.addView(shotView);
        // v1.7: poster expansion - shown INSTEAD of the screenshot
        // when enrichment found a poster for this entry: poster on
        // the left, IMDb / Rotten Tomatoes lines on the right (each
        // omitted when absent). Header title/link lines above stay
        // exactly as they are.
        final LinearLayout posterBox = new LinearLayout(this);
        posterBox.setOrientation(LinearLayout.HORIZONTAL);
        posterBox.setVisibility(View.GONE);
        LinearLayout.LayoutParams pbLp = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        pbLp.setMargins(0, dp(8), 0, 0);
        posterBox.setLayoutParams(pbLp);
        final ImageView posterView = new ImageView(this);
        posterView.setScaleType(ImageView.ScaleType.CENTER_CROP);
        posterView.setLayoutParams(new LinearLayout.LayoutParams(dp(92), dp(138)));
        posterBox.addView(posterView);
        LinearLayout ratingCol = new LinearLayout(this);
        ratingCol.setOrientation(LinearLayout.VERTICAL);
        ratingCol.setGravity(Gravity.CENTER_VERTICAL);
        LinearLayout.LayoutParams rcLp = new LinearLayout.LayoutParams(
                0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f);
        rcLp.setMargins(dp(10), 0, 0, 0);
        ratingCol.setLayoutParams(rcLp);
        final TextView imdbLine = tv("", 14, TEXT, true);
        final TextView rtLine = tv("", 14, TEXT, false);
        rtLine.setPadding(0, dp(6), 0, 0);
        ratingCol.addView(imdbLine); ratingCol.addView(rtLine);
        posterBox.addView(ratingCol);
        t.addView(posterBox);
        // v1.7: tag lets a finished background lookup avoid
        // rebuilding Home while the user is inside this shelf.
        t.setTag("rwTile");
        t.setFocusable(true); t.setFocusableInTouchMode(false);
        t.setOnFocusChangeListener((view, has) -> {
            if (has) focusIntoView(view); // v1.4: full border + scroll fully into view
            else view.setTranslationZ(0); // v1.5: undo the draw-order raise
            view.setBackground(solidBg(Color.parseColor("#3A3A3C"), has));
            float s = has ? 1.03f : 1f;
            view.animate().scaleX(s).scaleY(s).setDuration(120).start();
            if (!has) {
                shotView.setVisibility(View.GONE);
                posterBox.setVisibility(View.GONE);
                return;
            }
            // v1.7: poster wins over the screenshot when one exists.
            String hash = rwHash(item.url);
            Bitmap pbm = null;
            File pf = posterFile(hash);
            if (pf.isFile()) {
                pbm = posterCache.get(hash);
                if (pbm == null) {
                    try { pbm = BitmapFactory.decodeFile(pf.getAbsolutePath()); }
                    catch (Exception ignored) {}
                    if (pbm != null) posterCache.put(hash, pbm);
                }
            }
            if (pbm != null) {
                posterView.setImageBitmap(pbm);
                String im = enrGet(hash, "imdb"), rt = enrGet(hash, "rt");
                imdbLine.setText(im.isEmpty() ? "" : "IMDb \u2605 " + im);
                imdbLine.setVisibility(im.isEmpty() ? View.GONE : View.VISIBLE);
                rtLine.setText(rt.isEmpty() ? "" : "Rotten Tomatoes \uD83C\uDF45 " + rt);
                rtLine.setVisibility(rt.isEmpty() ? View.GONE : View.VISIBLE);
                posterBox.setVisibility(View.VISIBLE);
                shotView.setVisibility(View.GONE);
                return;
            }
            posterBox.setVisibility(View.GONE);
            if (item.shot != null && !item.shot.isEmpty()) {
                Bitmap bm = shotCache.get(item.shot);
                if (bm == null) {
                    try {
                        bm = BitmapFactory.decodeFile(
                                new File(rwShotsDir(), item.shot).getAbsolutePath());
                        if (bm != null) shotCache.put(item.shot, bm);
                    } catch (Exception ignored) {}
                }
                if (bm != null) {
                    shotView.setImageBitmap(bm);
                    shotView.setVisibility(View.VISIBLE);
                } else shotView.setVisibility(View.GONE);
            }
        });
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(dp(300), ViewGroup.LayoutParams.WRAP_CONTENT);
        lp.setMargins(dp(6), dp(6), dp(6), dp(6)); t.setLayoutParams(lp);
        t.setOnClickListener(v -> openRw(item));
        t.setOnLongClickListener(v -> {
            // v1.8: the remove dialog grew two poster-correction
            // options (any matcher can still miss): Re-fetch runs the
            // chain again for this entry; Remove poster art deletes
            // the poster and blocks auto-lookup until a Re-fetch.
            final String hash = rwHash(item.url);
            String[] opts = {"\u21BB Re-fetch poster", "\u2715 Remove poster art", "Remove entry"};
            AlertDialog rwDlg = new AlertDialog.Builder(this).setTitle(item.title)
                    .setItems(opts, (d, w) -> {
                        if (w == 0) {
                            deleteEnrichment(item); // poster + sidecar (blocked flag too)
                            kickEnrichment(item, true); // fresh chain run; completion rebuilds Home
                            buildHome();
                            Toast.makeText(this, "Re-fetching poster\u2026", Toast.LENGTH_SHORT).show();
                        } else if (w == 1) {
                            try { File f = posterFile(hash); if (f.isFile()) f.delete(); }
                            catch (Exception ignored) {}
                            posterCache.remove(hash);
                            prefs.edit().remove("enr_" + hash + "_poster")
                                    .putString("enr_" + hash + "_blocked", "1").apply();
                            buildHome(); // tile falls back to its screenshot
                            Toast.makeText(this, "Poster art removed", Toast.LENGTH_SHORT).show();
                        } else {
                            rw.remove(item); if (sessItem == item) sessItem = null;
                            deleteShotFile(item); shotCache.remove(item.shot);
                            deleteEnrichment(item); // v1.7: poster file + sidecar
                            saveRw(); buildHome();
                        }
                    })
                    .setNegativeButton("Cancel", null).create();
            trackDialog(rwDlg); // v1.22: phone keys must reach open dialogs too
            rwDlg.show();
            return true;
        });
        return t;
    }

    /** v1.9: dim red-tinted chrome for the RW Clear card (focused = white border, like tiles). */
    private GradientDrawable rwClearBg(boolean focused) {
        GradientDrawable g = new GradientDrawable();
        g.setColor(Color.parseColor(focused ? "#402227" : "#241417"));
        g.setCornerRadius(dp(16));
        g.setStroke(focused ? dp(3) : dp(1),
                focused ? Color.WHITE : Color.parseColor("#7A2E35"));
        return g;
    }

    /**
     * v1.9: utility card in the RW shelf (v1.10: moved to the FAR
     * RIGHT, after the last entry): a big text-glyph X (set in
     * the app fonts, not an emoji) over a two-line "Clear Recently
     * Watched" label. Same tile chrome/focus behavior as an RW
     * tile (focusIntoView + translationZ lift + 1.03 scale), but
     * a dimmer red-tinted fill so it reads as a utility action,
     * not content. Narrower than a content tile (170dp vs 300dp);
     * its minimum height plus vertical centering in the row keep
     * it visually level with the entry tiles.
     */
    private View rwClearCard(final int count) {
        LinearLayout t = new LinearLayout(this); t.setOrientation(LinearLayout.VERTICAL);
        t.setGravity(Gravity.CENTER);
        t.setPadding(dp(12), dp(12), dp(12), dp(12));
        t.setMinimumHeight(dp(96));
        t.setBackground(rwClearBg(false));
        TextView x = tv("\u2715", 30, Color.parseColor("#FF453A"), true);
        x.setGravity(Gravity.CENTER);
        TextView label = tv("Clear Recently Watched", 14, DIM, true);
        label.setGravity(Gravity.CENTER);
        label.setMaxLines(2);
        t.addView(x); t.addView(label);
        t.setFocusable(true); t.setFocusableInTouchMode(false);
        t.setOnFocusChangeListener((view, has) -> {
            if (has) focusIntoView(view);
            else view.setTranslationZ(0);
            view.setBackground(rwClearBg(has));
            float s = has ? 1.03f : 1f;
            view.animate().scaleX(s).scaleY(s).setDuration(120).start();
        });
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(dp(170),
                ViewGroup.LayoutParams.WRAP_CONTENT);
        lp.setMargins(dp(6), dp(6), dp(6), dp(6));
        lp.gravity = Gravity.CENTER_VERTICAL;
        t.setLayoutParams(lp);
        t.setOnClickListener(v -> confirmClearRw(count));
        return t;
    }

    /** v1.9: confirm, then run the SAME wipe Settings uses and rebuild Home (shelf + card vanish). */
    private void confirmClearRw(final int count) {
        AlertDialog d = new AlertDialog.Builder(this)
                .setTitle("Clear Recently Watched?")
                .setMessage("This removes all " + count
                        + (count == 1 ? " entry" : " entries")
                        + ", their screenshots and poster art.")
                .setPositiveButton("Clear", (dlg, w) -> {
                    clearAllRw(); // factored single routine - see clearAllRw()
                    Toast.makeText(this, "Recently Watched cleared", Toast.LENGTH_SHORT).show();
                    buildHome(); // empty RW: the shelf is gone and Search DOWN falls back to the first category tile (v1.6 wiring)
                })
                .setNegativeButton("Cancel", null)
                .create();
        trackDialog(d);
        d.show();
    }

    // ---------------------------------------------------------- Category / shelf pages

    private Entry findEntry(String url) {
        for (Entry e : catalog) if (e.url.equals(url)) return e;
        return null;
    }

    private Map<String, List<Entry>> shelvesFor(String catId) {
        Map<String, List<Entry>> map = new LinkedHashMap<>();
        for (Entry e : catalog) if (e.cat.equals(catId)) {
            List<Entry> l = map.get(e.shelf);
            if (l == null) { l = new ArrayList<>(); map.put(e.shelf, l); }
            l.add(e);
        }
        return map;
    }

    private void showCategory(String catId, String onlyShelf) {
        currentCat = catId;
        LinearLayout col = onlyShelf == null ? catContent : shelfContent;
        ScrollView sc = onlyShelf == null ? catScroll : shelfScroll;
        if (onlyShelf != null) expandedShelfBackCat = catId;
        col.removeAllViews();
        if (hasResume()) col.addView(resumeBar());
        LinearLayout head = new LinearLayout(this); head.setOrientation(LinearLayout.HORIZONTAL);
        head.setGravity(Gravity.CENTER_VERTICAL);
        Button back = safariBtn("\u2039 Home", false);
        back.setOnClickListener(v -> { if (onlyShelf != null) showCategory(catId, null); else showHome(); });
        head.addView(back);
        TextView title = tv(onlyShelf == null ? catTitle(catId) : onlyShelf, 26, TEXT, true);
        title.setPadding(dp(14), 0, 0, 0); head.addView(title);
        col.addView(head);

        Map<String, List<Entry>> shelves = shelvesFor(catId);
        if (shelves.isEmpty()) {
            col.addView(tv("No sites loaded yet for this category. Open Settings and tap Update FMHY Data.", 15, DIM, false));
        }
        for (Map.Entry<String, List<Entry>> sh : shelves.entrySet()) {
            if (onlyShelf != null && !onlyShelf.equals(sh.getKey())) continue;
            List<Entry> entries = sh.getValue();
            col.addView(shelfHeader(sh.getKey(), entries.size() + " sites"));
            int cap = onlyShelf == null ? 9 : entries.size();
            List<Entry> shown = entries.subList(0, Math.min(cap, entries.size()));
            for (int i = 0; i < shown.size(); i += 3) {
                LinearLayout row = new LinearLayout(this); row.setOrientation(LinearLayout.HORIZONTAL);
                unclip(row); // v1.4
                for (int j = i; j < Math.min(i + 3, shown.size()); j++) {
                    row.addView(appletTile(shown.get(j), false), tileLp());
                }
                int rem = 3 - Math.min(3, shown.size() - i);
                for (int k = 0; k < rem; k++) row.addView(new View(this),
                        new LinearLayout.LayoutParams(0, 1, 1f));
                col.addView(row);
            }
            if (onlyShelf == null && entries.size() > cap) {
                Button more = safariBtn("Show all " + entries.size() + " in " + sh.getKey() + " \u2192", false);
                final String shelfName = sh.getKey();
                more.setOnClickListener(v -> showCategory(catId, shelfName));
                LinearLayout.LayoutParams mlp = new LinearLayout.LayoutParams(
                        ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
                mlp.setMargins(0, dp(8), 0, 0);
                col.addView(more, mlp);
            }
        }
        showScreen(sc);
        sc.scrollTo(0, 0);
    }

    /** Weighted tile slot with 12dp gutters so neighbours never touch and focus growth stays on screen. */
    private LinearLayout.LayoutParams tileLp() {
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f);
        lp.setMargins(dp(6), dp(6), dp(6), dp(6));
        return lp;
    }

    private View appletTile(final Entry e, boolean wide) {
        LinearLayout t = new LinearLayout(this); t.setOrientation(LinearLayout.VERTICAL);
        t.setPadding(dp(12), dp(10), dp(12), dp(10));
        t.setBackground(solidBg(CARD, false));
        styleFocusable(t, CARD, 0, false);
        TextView name = tv((e.rec ? "\u2605 " : "") + e.name, 15, e.rec ? Color.parseColor("#FFD60A") : TEXT, true);
        name.setMaxLines(1); name.setEllipsize(TextUtils.TruncateAt.END);
        TextView desc = tv(e.desc, 12, DIM, false);
        desc.setMaxLines(2); desc.setEllipsize(TextUtils.TruncateAt.END);
        t.addView(name); t.addView(desc);
        LinearLayout.LayoutParams lp = wide
                ? new LinearLayout.LayoutParams(dp(280), ViewGroup.LayoutParams.WRAP_CONTENT)
                : new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        lp.setMargins(dp(6), dp(6), dp(6), dp(6)); t.setLayoutParams(lp);
        t.setOnClickListener(v -> openEntry(e));
        t.setOnLongClickListener(v -> { showEntryDetail(e); return true; });
        return t;
    }

    // ---------------------------------------------------------- E: mirror picker + A: favorites

    private boolean isFav(Entry e) { return favKeys.contains(e.favKey()); }

    private void toggleFav(Entry e) {
        if (isFav(e)) { favKeys.remove(e.favKey()); Toast.makeText(this, "Removed from Favorites", Toast.LENGTH_SHORT).show(); }
        else { favKeys.add(e.favKey()); Toast.makeText(this, "Added to Favorites", Toast.LENGTH_SHORT).show(); }
        saveFavs();
    }
    private void loadFavs() {
        favKeys.clear();
        String raw = prefs.getString(KEY_FAVS, "");
        if (raw != null && !raw.isEmpty()) for (String l : raw.split("\n")) if (!l.isEmpty()) favKeys.add(l);
    }
    private void saveFavs() {
        StringBuilder sb = new StringBuilder();
        for (String k : favKeys) { if (sb.length() > 0) sb.append('\n'); sb.append(k); }
        prefs.edit().putString(KEY_FAVS, sb.toString()).apply();
    }

    private void showEntryDetail(final Entry e) {
        lastMenuShownAt = System.currentTimeMillis();
        List<String> labels = new ArrayList<>();
        final List<String> urls = new ArrayList<>();
        labels.add("\u25B6 Open: " + e.url); urls.add(e.url);
        for (String m : e.mirrorList()) { labels.add("Mirror: " + m); urls.add(m); }
        labels.add(isFav(e) ? "\u2605 Remove from Favorites" : "\u2606 Add to Favorites");
        urls.add("__FAV__");
        labels.add("Copy primary URL"); urls.add("__COPY__");
        AlertDialog entryDlg = new AlertDialog.Builder(this)
                .setTitle((e.rec ? "\u2605 " : "") + e.name)
                .setMessage(e.desc + "\n\nCategory: " + catTitle(e.cat) + "  \u2022  " + e.shelf)
                .setItems(labels.toArray(new CharSequence[0]), (d, which) -> {
                    String pick = urls.get(which);
                    if ("__FAV__".equals(pick)) { toggleFav(e); if (currentScreen == homeScroll) buildHome(); }
                    else if ("__COPY__".equals(pick)) {
                        ClipboardManager cm = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
                        if (cm != null) cm.setPrimaryClip(ClipData.newPlainText("url", e.url));
                        Toast.makeText(this, "URL copied", Toast.LENGTH_SHORT).show();
                    } else {
                        Entry m = new Entry(e.cat, e.shelf, e.name, pick, e.desc, e.rec, e.mirrors);
                        openEntry(m);
                    }
                })
                .setNegativeButton("Close", null).create();
        trackDialog(entryDlg); // v1.22: phone keys must reach open dialogs too
        entryDlg.show();
    }

    // ---------------------------------------------------------- B: global search

    private LinearLayout buildSearchPane() {
        LinearLayout pane = pageColumn(); pane.setBackgroundColor(BG);
        searchResumeSlot = new LinearLayout(this); searchResumeSlot.setOrientation(LinearLayout.VERTICAL);
        pane.addView(searchResumeSlot);
        LinearLayout head = new LinearLayout(this); head.setOrientation(LinearLayout.HORIZONTAL);
        head.setGravity(Gravity.CENTER_VERTICAL);
        Button back = safariBtn("\u2039 Home", false); back.setOnClickListener(v -> showHome());
        head.addView(back);
        TextView t = tv("Search FMHY", 26, TEXT, true); t.setPadding(dp(14), 0, 0, 0); head.addView(t);
        pane.addView(head);
        LinearLayout row = new LinearLayout(this); row.setOrientation(LinearLayout.HORIZONTAL);
        row.setPadding(0, dp(12), 0, dp(6));
        searchInput = new EditText(this);
        searchInput.setHint("Search name or description across all categories");
        searchInput.setTextColor(TEXT); searchInput.setHintTextColor(DIM);
        searchInput.setSingleLine(true); searchInput.setImeOptions(EditorInfo.IME_ACTION_SEARCH);
        searchInput.setTypeface(tf(false));
        searchInput.setBackground(solidBg(CARD, false)); searchInput.setPadding(dp(14), dp(10), dp(14), dp(10));
        searchInput.setOnEditorActionListener((v, a, ev) -> { runSearch(); return true; });
        Button go = safariBtn("Search", false); go.setOnClickListener(v -> runSearch());
        row.addView(searchInput, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        row.addView(go, gapLp(false)); pane.addView(row);
        ScrollView sc = new ScrollView(this);
        unclip(sc); // v1.4
        searchResults = new LinearLayout(this); searchResults.setOrientation(LinearLayout.VERTICAL);
        unclip(searchResults); // v1.4
        sc.addView(searchResults); pane.addView(sc, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));
        return pane;
    }

    private void showSearch() {
        searchResumeSlot.removeAllViews();
        if (hasResume()) searchResumeSlot.addView(resumeBar());
        showScreen(searchPane);
        searchInput.requestFocus();
    }

    private void runSearch() {
        String q = searchInput.getText().toString().trim().toLowerCase(Locale.US);
        searchResults.removeAllViews();
        if (q.isEmpty()) return;
        List<Entry> hits = new ArrayList<>();
        for (Entry e : catalog) {
            if (e.name.toLowerCase(Locale.US).contains(q) || e.desc.toLowerCase(Locale.US).contains(q)
                    || e.shelf.toLowerCase(Locale.US).contains(q)) hits.add(e);
            if (hits.size() >= 60) break;
        }
        searchResults.addView(tv(hits.size() + " result(s) - long-press for mirrors / favorites", 14, DIM, false));
        for (int i = 0; i < hits.size(); i += 3) {
            LinearLayout row = new LinearLayout(this); row.setOrientation(LinearLayout.HORIZONTAL);
            unclip(row); // v1.4
            for (int j = i; j < Math.min(i + 3, hits.size()); j++)
                row.addView(appletTile(hits.get(j), false), tileLp());
            int rem = 3 - Math.min(3, hits.size() - i);
            for (int k = 0; k < rem; k++) row.addView(new View(this), new LinearLayout.LayoutParams(0, 1, 1f));
            searchResults.addView(row);
        }
        if (hits.isEmpty()) searchResults.addView(tv("No matches. Try a shorter word.", 15, DIM, false));
    }

    // ---------------------------------------------------------- Settings

    private LinearLayout buildSettingsPane() {
        LinearLayout pane = pageColumn(); pane.setBackgroundColor(BG);
        LinearLayout head = new LinearLayout(this); head.setOrientation(LinearLayout.HORIZONTAL);
        head.setGravity(Gravity.CENTER_VERTICAL);
        Button back = safariBtn("\u2039 Home", false); back.setOnClickListener(v -> showHome());
        head.addView(back);
        TextView t = tv("Settings", 26, TEXT, true); t.setPadding(dp(14), 0, 0, 0); head.addView(t);
        pane.addView(head);

        browserModeBtnSettings = settingRow(pane, "Browser control mode (Menu key toggles, browser only)", v -> setBrowserMouse(!browserMouse, true));
        scaleBtnSettings = settingRow(pane, "Browser scale (applies to every site)", v -> cycleScale(true));
        appearanceBtnSettings = settingRow(pane, "Browser appearance (browser chrome only, not web pages)", v -> {
            prefs.edit().putString(KEY_APPEARANCE, browserDark() ? "light" : "dark").apply();
            applyBrowserAppearance(); refreshSettingsLabels();
        });
        adblockBtnSettings = settingRow(pane, "Ad blocking (host-level)", v -> {
            boolean on = !prefs.getBoolean(KEY_ADBLOCK, true);
            prefs.edit().putBoolean(KEY_ADBLOCK, on).apply(); AdBlock.enabled = on; refreshSettingsLabels();
        });
        cursorBtnSettings = settingRow(pane, "Browser mouse cursor speed", v -> cycleCursorStep());
        // v1.13: while a video plays inline, app-driven page scrolls
        // are refused so moving toward the player's controls can't
        // slide the video away (fullscreen is never locked).
        videoLockBtnSettings = settingRow(pane, "Lock scrolling while a video plays (browser)", v -> {
            boolean on = !prefs.getBoolean(KEY_VIDEO_LOCK, true);
            prefs.edit().putBoolean(KEY_VIDEO_LOCK, on).apply(); refreshSettingsLabels();
        });
        // v1.19: phone remote - a LAN web page (PIN-paired, max 2
        // phones) that drives this browser: trackpad cursor, taps,
        // navigation, URL entry, typing, volume and seek.
        remoteBtnSettings = settingRow(pane, "Phone remote (control the browser from your phone)", v -> {
            boolean on = !prefs.getBoolean(KEY_REMOTE, false);
            prefs.edit().putBoolean(KEY_REMOTE, on).apply();
            if (on) startRemoteServer(); else stopRemoteServer();
            refreshSettingsLabels();
        });
        // v1.7: OMDb key row - drives the scores half of RW poster
        // enrichment (posters themselves also load without a key).
        omdbBtnSettings = settingRow(pane, "OMDb API key \u2014 posters + IMDb/RT scores", v -> openOmdbKeyDialog());
        TextView omdbNote = tv("Free key at omdbapi.com (1,000 lookups/day). Without a key, posters still load; scores need the key.", 12, DIM, false);
        omdbNote.setPadding(0, 0, 0, dp(6));
        pane.addView(omdbNote);

        Button upd = safariBtn("Update FMHY Data (live refresh from GitHub)", false);
        upd.setOnClickListener(v -> startRefresh());
        pane.addView(upd, stackLp());
        settingsStatus = tv("", 13, DIM, false); settingsStatus.setPadding(0, dp(8), 0, dp(12));
        pane.addView(settingsStatus);
        Button clearRw = safariBtn("Clear Recently Watched", false);
        clearRw.setOnClickListener(v -> { clearAllRw(); Toast.makeText(this, "Recently Watched cleared", Toast.LENGTH_SHORT).show(); }); // v1.9: shared with the Home Clear card
        pane.addView(clearRw, stackLp());
        Button clearResume = safariBtn("Clear saved browser page (Back-to button)", false);
        clearResume.setOnClickListener(v -> {
            resumeUrl = ""; resumeTitle = ""; resumeApplet = ""; resumeCat = ""; resumeSeek = 0;
            browserSuspended = false; saveResume();
            deleteResumeShot(); // v1.9: the hover preview belongs to the cleared target
            Toast.makeText(this, "Saved browser page cleared", Toast.LENGTH_SHORT).show();
        });
        pane.addView(clearResume, stackLp());
        Button clearFav = safariBtn("Clear Favorites", false);
        clearFav.setOnClickListener(v -> { favKeys.clear(); saveFavs(); Toast.makeText(this, "Favorites cleared", Toast.LENGTH_SHORT).show(); });
        pane.addView(clearFav, stackLp());
        Button clearCache = safariBtn("Clear browser cache + history", false);
        clearCache.setOnClickListener(v -> { if (webView != null) { webView.clearCache(true); webView.clearHistory(); } Toast.makeText(this, "Browser cache cleared", Toast.LENGTH_SHORT).show(); });
        pane.addView(clearCache, stackLp());
        TextView note = tv("Data source: github.com/fmhy/edit (docs/*.md). Bundled seed is used until the first successful update, and as fallback if an update fails.", 12, DIM, false);
        note.setPadding(0, dp(14), 0, 0); pane.addView(note);
        return pane;
    }

    private Button settingRow(LinearLayout parent, String label, View.OnClickListener toggle) {
        LinearLayout row = new LinearLayout(this); row.setOrientation(LinearLayout.HORIZONTAL);
        row.setPadding(0, dp(10), 0, dp(10)); row.setGravity(Gravity.CENTER_VERTICAL);
        TextView t = tv(label, 16, TEXT, false);
        Button b = safariBtn("", false); b.setOnClickListener(toggle);
        row.addView(t, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
        row.addView(b, gapLp(false)); parent.addView(row); return b;
    }

    private void cycleCursorStep() {
        int cur = prefs.getInt(KEY_CURSOR_STEP, 52);
        int next = cur == 24 ? 52 : cur == 52 ? 96 : 24;
        prefs.edit().putInt(KEY_CURSOR_STEP, next).apply(); refreshSettingsLabels();
    }
    private String cursorStepLabel() {
        int step = prefs.getInt(KEY_CURSOR_STEP, 52);
        return step == 24 ? "Slow" : step == 96 ? "Fast" : "Normal";
    }
    private int getScale() { return prefs.getInt(KEY_SCALE, 100); }
    private void cycleScale(boolean reload) {
        int cur = getScale(), next = SCALES[0];
        for (int i = 0; i < SCALES.length; i++) if (SCALES[i] == cur) { next = SCALES[(i + 1) % SCALES.length]; break; }
        setScale(next, reload);
    }
    private void setScale(int scale, boolean reload) {
        prefs.edit().putInt(KEY_SCALE, scale).apply();
        if (webView != null) {
            try {
                webView.getSettings().setTextZoom(scale);
                webView.setInitialScale(scale);
                if (reload && webView.getUrl() != null) webView.reload();
            } catch (Exception ignored) {}
        }
        refreshSettingsLabels();
    }

    private void refreshSettingsLabels() {
        if (browserModeBtnSettings == null) return;
        browserModeBtnSettings.setText(browserMouse ? "Mouse (default)" : "D-pad");
        scaleBtnSettings.setText(getScale() + "%");
        adblockBtnSettings.setText(prefs.getBoolean(KEY_ADBLOCK, true) ? "ON" : "OFF");
        cursorBtnSettings.setText(cursorStepLabel());
        if (videoLockBtnSettings != null) videoLockBtnSettings.setText(prefs.getBoolean(KEY_VIDEO_LOCK, true) ? "ON" : "OFF");
        if (remoteBtnSettings != null) remoteBtnSettings.setText(prefs.getBoolean(KEY_REMOTE, false) ? "ON" : "OFF");
        if (appearanceBtnSettings != null) appearanceBtnSettings.setText(browserDark() ? "Dark" : "Light");
        if (omdbBtnSettings != null) omdbBtnSettings.setText(
                prefs.getString(KEY_OMDB, "").trim().isEmpty() ? "Not set" : "Key saved");
        long upd = prefs.getLong(KEY_UPDATED, 0);
        settingsStatus.setText("Catalog: " + catalog.size() + " sites"
                + (upd > 0 ? "  \u2022  last updated " + new SimpleDateFormat("MMM d, h:mm a", Locale.US).format(new Date(upd)) : "  \u2022  bundled seed (not yet updated)"));
    }

    /** v1.7: paste/edit the OMDb API key. Saving persists the key,
     * clears every cached lookup "miss" (a miss recorded without a
     * key may simply have lacked scores) and kicks lookups for all
     * current RW entries. Blank input removes the key. */
    private void openOmdbKeyDialog() {
        final EditText input = new EditText(this);
        input.setTypeface(tf(false));
        input.setSingleLine(true);
        input.setText(prefs.getString(KEY_OMDB, ""));
        input.setSelectAllOnFocus(true);
        input.setHint("Paste OMDb API key (blank removes it)");
        AlertDialog omdbDlg = new AlertDialog.Builder(this)
                .setTitle("OMDb API key")
                .setMessage("Free key at omdbapi.com (1,000 lookups/day). Without a key, posters still load; scores need the key.")
                .setView(input)
                .setPositiveButton("Save", (d, w) -> {
                    String k = input.getText().toString().trim();
                    prefs.edit().putString(KEY_OMDB, k).apply();
                    refreshSettingsLabels();
                    clearEnrMisses();
                    for (RwItem i : new ArrayList<>(rw)) kickEnrichment(i, true);
                    Toast.makeText(this, k.isEmpty() ? "OMDb key removed"
                            : "OMDb key saved", Toast.LENGTH_SHORT).show();
                })
                .setNegativeButton("Cancel", null)
                .create();
        trackDialog(omdbDlg); // v1.22: phone keys must reach open dialogs too
        omdbDlg.show();
    }

    // ---------------------------------------------------------- Safari-style browser

    /**
     * v1.27: browser toolbar button factory - icon-only buttons
     * (canvas-drawn icons, no text; names come from the v1.25
     * tooltips). v1.29: buttons no longer carry their own outlined
     * circles - they are segments/ghosts inside the floating glass
     * clusters and capsule, and only the focus state paints a
     * background (see segmentBg/ghostBg). Sizes are layout params.
     * Backgrounds are owned by toolbarBg()/styleToolbarFocus().
     * v1.30: the control is a FrameLayout holding ONE centered
     * ImageView child. It was a Button whose icon rode as a LEFT
     * compound drawable, and TextView.onDraw pins that drawable at
     * x = mPaddingLeft + getHorizontalOffsetForDrawables() (= 0):
     * vertically centered but hard against the LEFT edge - in a
     * 48dp segment the 30dp glyph sat 9dp left of center (7dp in
     * Done's 44dp, 5dp in Reload's 40dp), on every control. That
     * was Kurt's "icons are shifted left". A gravity-CENTER child
     * ImageView cannot drift.
     */
    private FrameLayout toolbarBtn() {
        FrameLayout f = new FrameLayout(this);
        f.setFocusable(false); f.setFocusableInTouchMode(false); // keys handled at activity level
        ImageView iv = new ImageView(this);
        iv.setClickable(false);
        f.addView(iv, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT,
                Gravity.CENTER));
        return f;
    }

    /** Tint + attach a drawn icon to a toolbar control's centered ImageView. */
    private void setBtnIcon(FrameLayout f, BitmapDrawable icon, int tint) {
        if (icon != null) icon.setColorFilter(tint, PorterDuff.Mode.SRC_IN);
        ((ImageView) f.getChildAt(0)).setImageDrawable(icon);
    }

    /**
     * v1.29: the floating glass surface - a vertical translucent
     * gradient (lighter at the top edge, the fake-glass sheen), a
     * 1dp hairline border, and rounded corners. No blur exists on
     * API 25 (no RenderEffect), so translucency + gradient + a soft
     * elevation shadow on the view is the whole trick. Focused swaps
     * the hairline for a 2dp accent ring (used by the address
     * capsule, the bar's one free-standing glass element).
     */
    private GradientDrawable glassBg(int radiusDp, boolean focused) {
        GradientDrawable g = new GradientDrawable(
                GradientDrawable.Orientation.TOP_BOTTOM,
                new int[]{glassTop(), glassBot()});
        g.setCornerRadius(dp(radiusDp));
        g.setStroke(focused ? dp(2) : dp(1), focused ? ACCENT : glassBorder());
        return g;
    }

    /**
     * A cluster segment's background: NOTHING at rest (the cluster's
     * glass is the surface); focused = a translucent highlight fill
     * + accent ring, inset inside the cluster by its padding, so
     * the ring can never clip or shift the layout.
     */
    private GradientDrawable segmentBg(boolean focused) {
        GradientDrawable g = new GradientDrawable();
        g.setCornerRadius(dp(15));
        if (focused) {
            g.setColor(focusFill());
            g.setStroke(dp(2), ACCENT);
        } else {
            g.setColor(Color.TRANSPARENT);
        }
        return g;
    }

    /** Reload's background: a ghost inside the address capsule - same rule as a segment, circular. */
    private GradientDrawable ghostBg(boolean focused) {
        GradientDrawable g = new GradientDrawable();
        g.setShape(GradientDrawable.OVAL);
        if (focused) {
            g.setColor(focusFill());
            g.setStroke(dp(2), ACCENT);
        } else {
            g.setColor(Color.TRANSPARENT);
        }
        return g;
    }

    /**
     * The address capsule: glassBg plus a second, fainter fill
     * inset 3dp inside it (Safari's inner field shading), layered
     * so the outer layer alone carries the border/focus ring.
     */
    private Drawable capsuleBg(boolean focused) {
        GradientDrawable inset = new GradientDrawable();
        inset.setColor(capsuleInset());
        inset.setCornerRadius(dp(19));
        LayerDrawable ld = new LayerDrawable(
                new Drawable[]{glassBg(22, focused), inset});
        ld.setLayerInset(1, dp(3), dp(3), dp(3), dp(3));
        return ld;
    }

    /**
     * Done's background (v1.29 restyle): a restrained red-TINTED
     * glass circle in BOTH appearances - ~20% red fill, hairline
     * red-tinted border - so Done belongs to the same material
     * family as the clusters instead of clashing as a flat solid
     * disc. Focus strengthens the fill and rings it solid red.
     */
    private GradientDrawable doneBg(boolean focused) {
        GradientDrawable g = new GradientDrawable();
        g.setShape(GradientDrawable.OVAL);
        g.setColor(focused ? Color.argb(0x59, 0xFF, 0x45, 0x3A)
                            : Color.argb(0x33, 0xFF, 0x45, 0x3A)); // ~35% / ~20%
        g.setStroke(focused ? dp(2) : dp(1),
                focused ? DONE_RED : Color.argb(0x73, 0xFF, 0x45, 0x3A));
        return g;
    }

    /** The one place that knows each toolbar element's background. */
    private Drawable toolbarBg(View v, boolean focused) {
        if (v == addrCapsule) return capsuleBg(focused);
        if (v == tbDone) return doneBg(focused);
        if (v == tbReload) return ghostBg(focused);
        return segmentBg(focused);
    }

    /** v1.29: one segmented glass cluster (Back|Forward or Mode|Phone|Gear). */
    private LinearLayout glassCluster() {
        LinearLayout c = new LinearLayout(this);
        c.setOrientation(LinearLayout.HORIZONTAL);
        c.setGravity(Gravity.CENTER_VERTICAL);
        c.setPadding(dp(4), dp(4), dp(4), dp(4));
        c.setBackground(glassBg(20, false));
        c.setElevation(dp(4)); // soft shadow - the glass floats
        return c;
    }

    /** A 48x44dp segment slot inside a cluster. */
    private LinearLayout.LayoutParams segmentLp() {
        return new LinearLayout.LayoutParams(dp(48), dp(44));
    }

    /** Hairline divider between two cluster segments. */
    private View clusterDivider() {
        View d = new View(this);
        d.setBackgroundColor(glassDivider());
        tbDividers.add(d);
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(dp(1), dp(24));
        lp.leftMargin = dp(2); lp.rightMargin = dp(2);
        d.setLayoutParams(lp);
        return d;
    }

    /** Re-tint the drawn toolbar icons for the current appearance (Done stays red). */
    private void retintToolbarIcons() {
        int t = tbText();
        if (backIcon != null) backIcon.setColorFilter(t, PorterDuff.Mode.SRC_IN);
        if (fwdIcon != null) fwdIcon.setColorFilter(t, PorterDuff.Mode.SRC_IN);
        if (reloadIcon != null) reloadIcon.setColorFilter(t, PorterDuff.Mode.SRC_IN);
        if (gearIcon != null) gearIcon.setColorFilter(t, PorterDuff.Mode.SRC_IN);
        if (doneIcon != null) doneIcon.setColorFilter(DONE_RED, PorterDuff.Mode.SRC_IN);
    }

    /**
     * Back/Forward dim with history state: full-strength icon when
     * the direction is available, faded when it is not. Called from
     * setUrlPillText, which every navigation update funnels through
     * (page start/finish, pushState, entry open, resume restore).
     */
    private void refreshNavIcons() {
        if (webView == null) return;
        if (backIcon != null) backIcon.setAlpha(webView.canGoBack() ? 255 : 90);
        if (fwdIcon != null) fwdIcon.setAlpha(webView.canGoForward() ? 255 : 90);
    }

    /**
     * v1.25: draw one stroke-style toolbar icon into a Bitmap, in
     * white; callers tint the returned drawable with a PorterDuff
     * SRC_IN color filter. Runtime-drawn because neither the bundled
     * Inter font nor the Fire OS system fonts can be trusted to
     * carry mouse/phone glyphs. v1.27: canvas is 30dp (glyphs render
     * ~26-28dp inside the 48-52dp circular buttons) and the set grew:
     * "back", "fwd", "reload", "gear", "done" join "mouse", "dpad",
     * "phone".
     */
    private BitmapDrawable strokeIcon(String kind) {
        int s = dp(30);
        Bitmap bmp = Bitmap.createBitmap(s, s, Bitmap.Config.ARGB_8888);
        Canvas c = new Canvas(bmp);
        // v1.28 (Part 2): the whole set redrawn to SF Symbols
        // discipline - strokes ~0.07-0.075 of the canvas (v1.27 used
        // 0.13-0.15, which read chunky at TV distance and turned the
        // gear into a sun), glyph ~70% of the canvas, round caps and
        // joins. The numbers below were verified BEFORE porting by
        // rendering the identical geometry in a Python/PIL prototype
        // at button scale: gear = option (b), 8 fat arc segments
        // (the "dashed ring") over a ring + hub; reload's head uses
        // the same construction as the remote page's arrows - tip
        // exactly at the arc endpoint, base behind it along the arc.
        Paint p = new Paint(Paint.ANTI_ALIAS_FLAG);
        p.setStyle(Paint.Style.STROKE);
        p.setStrokeWidth(s * 0.072f);
        p.setStrokeCap(Paint.Cap.ROUND);
        p.setStrokeJoin(Paint.Join.ROUND);
        p.setColor(Color.WHITE);
        Paint fill = new Paint(Paint.ANTI_ALIAS_FLAG);
        fill.setStyle(Paint.Style.FILL);
        fill.setColor(Color.WHITE);
        float cx = s / 2f, cy = s / 2f;
        if ("mouse".equals(kind)) {
            // Slimmer body; wheel = short dash; the divider is
            // butt-capped and inset - with round caps at full width
            // it poked nubs outside the body outline.
            RectF body = new RectF(s * 0.315f, s * 0.09f, s * 0.685f, s * 0.91f);
            c.drawRoundRect(body, s * 0.17f, s * 0.17f, p);
            c.drawLine(cx, s * 0.215f, cx, s * 0.315f, p);
            Paint dv = new Paint(Paint.ANTI_ALIAS_FLAG);
            dv.setStyle(Paint.Style.STROKE);
            dv.setStrokeWidth(s * 0.045f);
            dv.setStrokeCap(Paint.Cap.BUTT);
            dv.setColor(Color.WHITE);
            c.drawLine(s * 0.325f, s * 0.335f, s * 0.675f, s * 0.335f, dv);
        } else if ("dpad".equals(kind)) {
            // One plus-outline path (arm half-width 0.105, span
            // 0.155-0.845) + a small filled center dot; replaces the
            // two-overlapping-rounded-rects cross.
            float aw = s * 0.105f, lo = s * 0.155f, hi = s * 0.845f;
            Path plus = new Path();
            plus.moveTo(cx - aw, lo);
            plus.lineTo(cx + aw, lo);
            plus.lineTo(cx + aw, cy - aw);
            plus.lineTo(hi, cy - aw);
            plus.lineTo(hi, cy + aw);
            plus.lineTo(cx + aw, cy + aw);
            plus.lineTo(cx + aw, hi);
            plus.lineTo(cx - aw, hi);
            plus.lineTo(cx - aw, cy + aw);
            plus.lineTo(lo, cy + aw);
            plus.lineTo(lo, cy - aw);
            plus.lineTo(cx - aw, cy - aw);
            plus.close();
            Paint pp = new Paint(p);
            pp.setStrokeWidth(s * 0.07f);
            c.drawPath(plus, pp);
            c.drawCircle(cx, cy, s * 0.042f, fill);
        } else if ("back".equals(kind) || "fwd".equals(kind)) {
            // SF chevron proportions: tall and narrow
            // (0.64, 0.18) -> (0.36, 0.50) -> (0.64, 0.82).
            // v1.30: nudged +0.02 from the v1.28 numbers so the
            // ink's bounding box is exactly canvas-centered
            // ((0.36-0.0375 + 0.64+0.0375) / 2 = 0.50).
            Paint cp = new Paint(p);
            cp.setStrokeWidth(s * 0.075f);
            Path path = new Path();
            if ("back".equals(kind)) {
                path.moveTo(s * 0.64f, s * 0.18f);
                path.lineTo(s * 0.36f, s * 0.50f);
                path.lineTo(s * 0.64f, s * 0.82f);
            } else {
                path.moveTo(s * 0.36f, s * 0.18f);
                path.lineTo(s * 0.64f, s * 0.50f);
                path.lineTo(s * 0.36f, s * 0.82f);
            }
            c.drawPath(path, cp);
        } else if ("reload".equals(kind)) {
            // v1.30: canonical circular-arrow construction, verified
            // in real renders BEFORE porting (the exact canvas calls
            // replicated in PIL): the arc is drawn only as far as
            // the head's BASE CENTER and the filled head grows out
            // of that point - tip on the circle at 300 deg, base
            // center 0.50 rad back along the arc, base corners
            // radial at +-0.289r. The v1.28/29 version ran a
            // 300-degree arc UNDER a narrow head (base ~1.2x the
            // stroke): on device it rendered as a thorn, not a tip.
            Paint rp = new Paint(p);
            rp.setStrokeWidth(s * 0.08f);
            rp.setStrokeCap(Paint.Cap.BUTT);
            RectF oval = new RectF(s * 0.185f, s * 0.185f, s * 0.815f, s * 0.815f);
            float r = s * 0.315f;
            double end = Math.toRadians(300);
            double bk = end - 0.50; // head length 0.50 * r, as radians of arc
            c.drawArc(oval, 50f, (float) (Math.toDegrees(bk) - 50.0), false, rp);
            float ex = cx + (float) (r * Math.cos(end));
            float ey = cy + (float) (r * Math.sin(end));
            float bx = cx + (float) (r * Math.cos(bk));
            float by = cy + (float) (r * Math.sin(bk));
            float hb = r * 0.289f; // half the base width
            float nx = (float) Math.cos(bk), ny = (float) Math.sin(bk);
            Path head = new Path();
            head.moveTo(ex, ey);
            head.lineTo(bx + nx * hb, by + ny * hb);
            head.lineTo(bx - nx * hb, by - ny * hb);
            head.close();
            c.drawPath(head, fill);
        } else if ("gear".equals(kind)) {
            // Dashed-ring teeth: 8 arc segments of 20 degrees on
            // r=0.395 with a 0.13 stroke (band 0.33-0.46), over a
            // stroked ring r=0.295 (its outer edge meets the band)
            // and a filled hub r=0.095. The PIL prototype picked
            // this over trapezoid teeth: it reads as a gear at
            // 48dp on a TV; radial-line teeth read as a sun.
            Paint tp = new Paint(Paint.ANTI_ALIAS_FLAG);
            tp.setStyle(Paint.Style.STROKE);
            tp.setStrokeWidth(s * 0.13f);
            tp.setStrokeCap(Paint.Cap.BUTT);
            tp.setColor(Color.WHITE);
            RectF teeth = new RectF(s * 0.105f, s * 0.105f, s * 0.895f, s * 0.895f);
            for (int i = 0; i < 8; i++)
                c.drawArc(teeth, i * 45f - 10f, 20f, false, tp);
            Paint rp = new Paint(p);
            rp.setStrokeWidth(s * 0.07f);
            c.drawCircle(cx, cy, s * 0.295f, rp);
            c.drawCircle(cx, cy, s * 0.095f, fill);
        } else if ("done".equals(kind)) {
            // X matched to the new set; the RED comes from the tint
            // the caller applies (this bitmap stays white).
            Paint xp = new Paint(p);
            xp.setStrokeWidth(s * 0.09f);
            c.drawLine(s * 0.335f, s * 0.335f, s * 0.665f, s * 0.665f, xp);
            c.drawLine(s * 0.665f, s * 0.335f, s * 0.335f, s * 0.665f, xp);
        } else { // "phone": slimmer outline, smaller speaker + dot
            RectF body = new RectF(s * 0.295f, s * 0.075f, s * 0.705f, s * 0.925f);
            c.drawRoundRect(body, s * 0.085f, s * 0.085f, p);
            Paint sp = new Paint(p);
            sp.setStrokeWidth(s * 0.055f);
            c.drawLine(cx - s * 0.06f, s * 0.165f, cx + s * 0.06f, s * 0.165f, sp);
            c.drawCircle(cx, s * 0.83f, s * 0.03f, fill);
        }
        return new BitmapDrawable(getResources(), bmp);
    }

    /**
     * v1.25: the Mode button's icon follows the SAME state rule the
     * old text label followed: it names the CURRENT sub-mode
     * (mouse icon while browserMouse is on, D-pad cross while off).
     */
    private void updateModeBtn() {
        if (tbMode == null) return;
        BitmapDrawable icon = browserMouse ? modeMouseIcon : modeDpadIcon;
        if (icon != null) setBtnIcon(tbMode, icon, tbText()); // v1.30: centered ImageView child
        tbMode.setContentDescription(browserMouse ? "Mouse" : "D-pad");
        // Keep an already-visible tooltip on this button truthful
        // across a mode flip (the flip usually comes from a click,
        // which hides the tip first - this is just belt and braces).
        if (toolbarTipTarget == tbMode && toolbarTipView != null
                && toolbarTipView.getVisibility() == View.VISIBLE)
            toolbarTipView.setText(toolbarTipText(tbMode));
    }

    private void styleToolbarFocus() {
        for (int i = 0; i < toolbarViews.size(); i++) {
            View v = toolbarViews.get(i);
            boolean foc = toolbarFocus && i == toolbarIndex;
            // v1.27: per-element backgrounds (URL pill / red Done /
            // circular buttons) come from the one toolbarBg() helper.
            v.setBackground(toolbarBg(v, foc));
            // v1.3: no scale transform on toolbar buttons - a growing
            // button pushed its outline past the bar/screen edge and
            // the stroke got clipped (shown in a device screenshot). Focus is
            // shown by the accent ring in toolbarBg() only.
            v.animate().scaleX(1f).scaleY(1f).setDuration(100).start();
        }
        // v1.25: D-pad toolbar focus also raises the name bubble for
        // the focused element (two buttons are icon-only now). A
        // 150ms dwell keeps fast LEFT/RIGHT runs from flashing it;
        // losing toolbar focus hides it. In mouse sub-mode
        // toolbarFocus is never set, so hover tips (driven from
        // moveCursor) are unaffected in practice.
        if (toolbarFocus && !browserMouse && currentScreen == browserPane
                && fsView == null && toolbarIndex >= 0
                && toolbarIndex < toolbarViews.size()) {
            scheduleToolbarTip(toolbarViews.get(toolbarIndex), 150);
        } else if (!browserMouse) {
            hideToolbarTip();
        }
    }

    private FrameLayout buildBrowserPane() {
        FrameLayout root = new FrameLayout(this);
        // BLACK, never white (v1.2): a white root + the WebView's default
        // opaque white background painted behind transparent/rgba page
        // elements and showed through as white boxes.
        root.setBackgroundColor(Color.BLACK);
        browserCol = new LinearLayout(this); browserCol.setOrientation(LinearLayout.VERTICAL);
        browserCol.setBackgroundColor(browserDark() ? Color.BLACK : SAFARI_BG); // behind pages while loading
        // v1.4: toolbar + title strip + progress live in ONE header
        // block so the whole header can slide away on scroll-down and
        // the WebView expands into the freed height (Safari-style).
        headerBlock = new LinearLayout(this); headerBlock.setOrientation(LinearLayout.VERTICAL);
        // v1.29: the header block is only the floating bar's
        // positioning frame - transparent, inset from the screen
        // edges (20dp sides, 12dp top), no slab behind the glass.
        headerBlock.setPadding(dp(20), dp(12), dp(20), dp(8));
        unclip(headerBlock);
        browserBar = new FrameLayout(this);
        FrameLayout bar = browserBar;
        bar.setBackgroundColor(Color.TRANSPARENT);
        unclip(bar);

        // v1.29 toolbar: ONE floating glass bar, three integrated
        // clusters, no per-button circles. LEFT: a segmented capsule
        // Back | Forward. CENTER (held screen-centered by equal-
        // weight spacers): the address capsule - host on top, page
        // title as a quiet second line, Reload a ghost inside its
        // trailing edge. RIGHT: a segmented capsule Mode | Phone |
        // Gear, then Done as a red-tinted glass circle. The 8 stops
        // and their order are unchanged from v1.27; every icon is
        // still canvas-drawn (strokeIcon) - no font glyphs.
        tbBack = toolbarBtn();
        backIcon = strokeIcon("back");
        setBtnIcon(tbBack, backIcon, tbText());
        tbBack.setContentDescription("Back");
        tbBack.setOnClickListener(v -> { if (webView.canGoBack()) webView.goBack(); });
        tbFwd = toolbarBtn();
        fwdIcon = strokeIcon("fwd");
        setBtnIcon(tbFwd, fwdIcon, tbText());
        tbFwd.setContentDescription("Forward");
        tbFwd.setOnClickListener(v -> { if (webView.canGoForward()) webView.goForward(); });
        // URL is a BUTTON (the address capsule), not an EditText:
        // D-pad focus lands on it as one stop; clicking opens the
        // URL editor dialog (which pre-fills the FULL current URL
        // from the WebView, not the host shown here - see
        // openUrlDialog / setUrlPillText). v1.29: the capsule holds
        // TWO lines - the host (15sp semibold) and, under it, the
        // page title (11sp secondary) - so the separate title strip
        // under the bar is gone. Side padding keeps the centered
        // text clear of Reload's ghost slot at the trailing edge.
        urlPill = new TextView(this);
        urlPill.setSingleLine(true); urlPill.setTextSize(15); urlPill.setTextColor(tbText());
        urlPill.setTypeface(fontSemi != null ? fontSemi : tf(true));
        urlPill.setGravity(Gravity.CENTER); urlPill.setEllipsize(TextUtils.TruncateAt.END);
        urlPill.setText("Search or enter website name");
        urlPill.setFocusable(false); urlPill.setFocusableInTouchMode(false);
        urlPill.setOnClickListener(v -> openUrlDialog());
        browserTitle = tv("", 11, tbSecondary(), false);
        browserTitle.setSingleLine(true); browserTitle.setEllipsize(TextUtils.TruncateAt.END);
        browserTitle.setGravity(Gravity.CENTER);
        browserTitle.setBackgroundColor(Color.TRANSPARENT);
        browserTitle.setPadding(0, dp(2), 0, 0);
        tbReload = toolbarBtn();
        reloadIcon = strokeIcon("reload");
        setBtnIcon(tbReload, reloadIcon, tbText());
        tbReload.setContentDescription("Reload");
        tbReload.setOnClickListener(v -> webView.reload());
        // v1.25: Mode is an ICON button (was the text "Mouse"/"D-pad").
        // The icon follows the label's state rule - it shows the
        // CURRENT sub-mode - and updateModeBtn() owns icon + tint.
        tbMode = toolbarBtn();
        modeMouseIcon = strokeIcon("mouse");
        modeDpadIcon = strokeIcon("dpad");
        tbMode.setOnClickListener(v -> setBrowserMouse(!browserMouse, true));
        updateModeBtn();
        // v1.20: Phone button opens the v1.19 pairing panel; its
        // label tinted accent while a phone is paired. v1.25: it is
        // an icon button too (canvas-drawn smartphone - bundled
        // Inter has no trustworthy phone glyph on Fire OS), and
        // refreshPhoneBtn() now tints the icon instead.
        tbPhone = toolbarBtn();
        phoneIconDrawable = strokeIcon("phone");
        setBtnIcon(tbPhone, phoneIconDrawable, tbText()); // v1.30: centered ImageView child
        tbPhone.setContentDescription("Phone");
        tbPhone.setOnClickListener(v -> openRemotePanel());
        tbGear = toolbarBtn();
        gearIcon = strokeIcon("gear");
        setBtnIcon(tbGear, gearIcon, tbText());
        tbGear.setContentDescription("Settings");
        tbGear.setOnClickListener(v -> openBrowserSettings());
        // v1.27: Done loses its text: a red X on a translucent red
        // circle (doneBg), red in both appearances.
        tbDone = toolbarBtn();
        doneIcon = strokeIcon("done");
        setBtnIcon(tbDone, doneIcon, DONE_RED);
        tbDone.setContentDescription("Done");
        tbDone.setOnClickListener(v -> closeBrowser());

        // The capsule replaces urlPill as the URL focus stop; the
        // stop ORDER is unchanged: Back, Forward, URL, Reload,
        // Mode, Phone, Gear, Done.
        addrCapsule = new FrameLayout(this);
        addrCapsule.setOnClickListener(v -> openUrlDialog());
        addrCapsule.setElevation(dp(4));
        LinearLayout addrText = new LinearLayout(this);
        addrText.setOrientation(LinearLayout.VERTICAL);
        addrText.setGravity(Gravity.CENTER);
        addrText.setPadding(dp(50), 0, dp(50), 0); // clear of Reload's slot
        addrText.addView(urlPill, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));
        addrText.addView(browserTitle, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));
        addrCapsule.addView(addrText, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        FrameLayout.LayoutParams reloadLp = new FrameLayout.LayoutParams(dp(40), dp(40));
        reloadLp.gravity = Gravity.RIGHT | Gravity.CENTER_VERTICAL;
        reloadLp.rightMargin = dp(8);
        addrCapsule.addView(tbReload, reloadLp);

        toolbarViews.clear();
        toolbarViews.add(tbBack); toolbarViews.add(tbFwd); toolbarViews.add(addrCapsule);
        toolbarViews.add(tbReload); toolbarViews.add(tbMode); toolbarViews.add(tbPhone);
        toolbarViews.add(tbGear); toolbarViews.add(tbDone);
        // v1.30: the bar is a FrameLayout so the address capsule is
        // centered by gravity, exactly. The v1.29 LinearLayout used
        // two equal-weight spacers, but the flanks are NOT equal
        // (left cluster 109dp vs right cluster + Done 216dp), which
        // parked the capsule - and everything in it - ~53dp left of
        // center at 1080p. Now: nav cluster pinned LEFT, capsule at
        // Gravity.CENTER, right cluster + Done pinned RIGHT.
        // LEFT cluster: Back | Forward
        navCluster = glassCluster();
        navCluster.addView(tbBack, segmentLp());
        navCluster.addView(clusterDivider());
        navCluster.addView(tbFwd, segmentLp());
        bar.addView(navCluster, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, dp(52),
                Gravity.LEFT | Gravity.CENTER_VERTICAL));
        bar.addView(addrCapsule, new FrameLayout.LayoutParams(
                dp(460), dp(56), Gravity.CENTER));
        // RIGHT cluster: Mode | Phone | Gear, then Done.
        rightCluster = glassCluster();
        rightCluster.addView(tbMode, segmentLp());
        rightCluster.addView(clusterDivider());
        rightCluster.addView(tbPhone, segmentLp());
        rightCluster.addView(clusterDivider());
        rightCluster.addView(tbGear, segmentLp());
        LinearLayout rightSide = new LinearLayout(this);
        rightSide.setOrientation(LinearLayout.HORIZONTAL);
        rightSide.setGravity(Gravity.CENTER_VERTICAL);
        rightSide.addView(rightCluster, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, dp(52)));
        tbDone.setStateListAnimator(null); // keep the elevation shadow steady
        tbDone.setElevation(dp(4));
        LinearLayout.LayoutParams doneLp = new LinearLayout.LayoutParams(dp(44), dp(44));
        doneLp.leftMargin = dp(10);
        rightSide.addView(tbDone, doneLp);
        bar.addView(rightSide, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, dp(52),
                Gravity.RIGHT | Gravity.CENTER_VERTICAL));
        headerBlock.addView(bar, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, dp(64)));
        // Creation-time backgrounds (styleToolbarFocus re-applies
        // them with focus state on every browser open).
        for (View v : toolbarViews) v.setBackground(toolbarBg(v, false));

        pageProgress = new ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal);
        pageProgress.setMax(100); pageProgress.setVisibility(View.GONE);
        try { pageProgress.setProgressTintList(ColorStateList.valueOf(ACCENT)); } catch (Exception ignored) {}
        headerBlock.addView(pageProgress);
        browserCol.addView(headerBlock);
        webView = new ScrollWebView(this);
        ((ScrollWebView) webView).listener = (oldY, newY) -> onWebScrollChanged(oldY, newY);
        webView.setBackgroundColor(0x00000000); // transparent, see root note above
        browserCol.addView(webView, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));
        root.addView(browserCol, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));

        // (v1.28: the auto pairing prompt at browser start is the
        // themed CARD built below - the v1.27 auto AlertDialog and
        // the v1.20 hint bubble before it are both gone.)

        // v1.25: toolbar name bubble (hover / D-pad focus tooltips).
        // Same visual family as the retired pair tip; renders above
        // the WebView, still below the decor-level cursor overlay.
        // Non-clickable/non-focusable.
        toolbarTipView = new TextView(this);
        toolbarTipView.setTextSize(13); toolbarTipView.setTextColor(Color.WHITE);
        toolbarTipView.setTypeface(tf(false));
        toolbarTipView.setPadding(dp(12), dp(6), dp(12), dp(6));
        GradientDrawable tbTipBg = new GradientDrawable();
        tbTipBg.setColor(Color.parseColor("#2C2C2E"));
        tbTipBg.setCornerRadius(dp(8));
        tbTipBg.setStroke(dp(1), Color.parseColor("#48484A"));
        toolbarTipView.setBackground(tbTipBg);
        toolbarTipView.setClickable(false);
        toolbarTipView.setFocusable(false); toolbarTipView.setFocusableInTouchMode(false);
        toolbarTipView.setVisibility(View.GONE);
        FrameLayout.LayoutParams tbTipLp = new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        tbTipLp.gravity = Gravity.TOP | Gravity.LEFT;
        root.addView(toolbarTipView, tbTipLp);

        // v1.28: the auto pairing CARD - same overlay layer as the
        // tooltip (above the WebView, below the decor-level cursor
        // and feedback overlays), anchored top-right at show time.
        buildPairCard(root);

        // fsContainer is NOT created here anymore (v1.2): it is attached
        // to the window's decorView in buildUi() so fullscreen video can
        // cover the entire display.
        return root;
    }

    private void trackDialog(AlertDialog d) {
        trackDialog(d, null);
    }

    private void trackDialog(AlertDialog d, Runnable dismissedExtra) {
        d.setOnShowListener(x -> { browserDialogDepth++; openDialog = d; hideToolbarTip(); hidePairCardNow(); refreshCursor("dialog-show"); }); // v1.22: openDialog; v1.25: no tooltip under a dialog; v1.28: no pairing card under one either; v1.32: cursor re-asserted across dialog transitions
        d.setOnDismissListener(x -> {
            if (openDialog == d) openDialog = null; // v1.22
            if (browserDialogDepth > 0) browserDialogDepth--;
            if (dismissedExtra != null) dismissedExtra.run();
            refreshCursor("dialog-dismiss"); // v1.32
            // v1.21: the LAST browser dialog closing is one of
            // the moments the video surface can be left black -
            // revive it with a 1px layout kick (gated inside).
            if (browserDialogDepth == 0) ui.post(() -> kickVideoLayer("dialog"));
        });
    }

    /** WebView that reports scroll changes (v1.4 auto-hiding header). */
    static class ScrollWebView extends WebView {
        interface Listener { void onScroll(int oldY, int newY); }
        Listener listener;
        ScrollWebView(Context ctx) { super(ctx); }
        @Override protected void onScrollChanged(int l, int t, int oldl, int oldt) {
            super.onScrollChanged(l, t, oldl, oldt);
            if (listener != null) listener.onScroll(oldt, t);
        }
    }

    // ------------------------------------------------ v1.4: auto-hiding header

    private void onWebScrollChanged(int oldY, int newY) {
        if (currentScreen != browserPane || fsView != null || browserDialogDepth > 0) {
            scrollAccum = 0;
            return;
        }
        if (newY <= 0) { scrollAccum = 0; if (headerHidden) showHeader(true); return; }
        int dy = newY - oldY;
        if (dy == 0) return;
        if ((dy > 0) != (scrollAccum > 0)) scrollAccum = dy; else scrollAccum += dy;
        if (!headerHidden && newY > dp(150) && scrollAccum > dp(40)) hideHeader();
        else if (headerHidden && scrollAccum < -dp(20)) showHeader(true);
    }

    private void hideHeader() {
        if (headerHidden || headerBlock == null) return;
        headerHidden = true;
        hideToolbarTip(); // v1.25: the bar it names is going away
        scrollAccum = 0;
        // Toolbar focus cannot live in a hidden bar.
        toolbarFocus = false; styleToolbarFocus();
        int h = headerBlock.getHeight() > 0 ? headerBlock.getHeight() : dp(88);
        headerBlock.animate().cancel();
        headerBlock.animate().translationY(-h).alpha(0f).setDuration(180)
                .withEndAction(() -> {
                    if (headerHidden) {
                        headerBlock.setVisibility(View.GONE);
                        // Reset transform so the next show starts clean.
                        headerBlock.setTranslationY(0); headerBlock.setAlpha(1f);
                    }
                }).start();
    }

    private void showHeader(boolean animate) {
        if (headerBlock == null) return;
        headerBlock.animate().cancel();
        if (!headerHidden && headerBlock.getVisibility() == View.VISIBLE) {
            headerBlock.setTranslationY(0); headerBlock.setAlpha(1f);
            return;
        }
        headerHidden = false;
        scrollAccum = 0;
        headerBlock.setVisibility(View.VISIBLE);
        if (animate) {
            float h = headerBlock.getHeight() > 0 ? headerBlock.getHeight() : dp(88);
            headerBlock.setTranslationY(-h); headerBlock.setAlpha(0f);
            headerBlock.animate().translationY(0).alpha(1f).setDuration(180).start();
        } else {
            headerBlock.setTranslationY(0); headerBlock.setAlpha(1f);
        }
    }

    // ------------------------------------------------ v1.4: long-press nav menu

    /** Entry describing the page currently in the WebView, for Favorites. */
    private Entry currentPageEntry() {
        String url = webView != null ? webView.getUrl() : null;
        if (url == null || url.isEmpty()) url = currentEntry != null ? currentEntry.url : "";
        String title = webView != null ? webView.getTitle() : null;
        if ((title == null || title.isEmpty()) && currentEntry != null) title = currentEntry.name;
        if (title == null || title.isEmpty()) title = hostOf(url);
        String cat = currentEntry != null ? currentEntry.cat : "misc";
        String shelf = currentEntry != null ? currentEntry.shelf : "Browser";
        String name = (currentEntry != null && url.equals(currentEntry.url))
                ? currentEntry.name : title;
        return new Entry(cat, shelf, name, url, title, false, "");
    }

    /**
     * Long-press / MENU "Browser Menu": replicates every toolbar
     * option, in toolbar order, reusing the toolbar's own handlers.
     */
    private void openBrowserNavMenu() {
        if (fsView != null || currentScreen != browserPane || webView == null) return;
        showHeader(true); // the bar this menu mirrors should be visible
        final Entry pageEntry = currentPageEntry();
        final boolean fav = isFav(pageEntry);
        final String[] items = {
            "\u2039 Back",
            "\u203A Forward",
            "Edit URL\u2026",
            "\u21BB Reload",
            "Mouse / D-pad mode: " + (browserMouse ? "Mouse" : "D-pad"),
            "\u2699 Browser Settings\u2026",
            fav ? "\u2605 Remove current page from Favorites"
                : "\u2605 Add current page to Favorites",
            "Phone remote\u2026",
            "\u2715 Done (close browser)",
        };
        AlertDialog d = new AlertDialog.Builder(dialogContext())
                .setTitle("Browser Menu")
                .setItems(items, (dlg, which) -> {
                    switch (which) {
                        case 0: if (webView.canGoBack()) webView.goBack(); break;
                        case 1: if (webView.canGoForward()) webView.goForward(); break;
                        case 2: openUrlDialog(); break;
                        case 3: webView.reload(); break;
                        case 4: setBrowserMouse(!browserMouse, true); break;
                        case 5: openBrowserSettings(); break;
                        case 6: toggleFav(pageEntry); break;
                        case 7: openRemotePanel(); break;
                        case 8: closeBrowser(); break;
                        default: break;
                    }
                })
                .setNegativeButton("Cancel", null)
                .create();
        trackDialog(d);
        d.show();
    }

    private void openUrlDialog() {
        final EditText input = new EditText(dialogContext());
        input.setTypeface(tf(false));
        input.setSingleLine(true);
        input.setText(webView.getUrl() != null ? webView.getUrl() : "");
        input.setSelectAllOnFocus(true);
        input.setHint("Search or enter website name");
        input.setImeOptions(EditorInfo.IME_ACTION_GO);
        AlertDialog d = new AlertDialog.Builder(dialogContext())
                .setTitle("Go to")
                .setView(input)
                .setPositiveButton("Go", (dlg, w) -> loadResolved(input.getText().toString().trim()))
                .setNegativeButton("Cancel", null)
                .create();
        trackDialog(d);
        d.show();
        input.requestFocus();
        input.selectAll();
    }

    /** Gear button: browser settings sheet. Items toggle in place; dialog re-opens refreshed. */
    private void openBrowserSettings() {
        final String[] items = {
            "Scale: " + getScale() + "%",
            "Appearance: " + (browserDark() ? "Dark" : "Light"),
            "Ad blocking: " + (prefs.getBoolean(KEY_ADBLOCK, true) ? "ON" : "OFF"),
            "JavaScript: " + (prefs.getBoolean(KEY_JS, true) ? "ON" : "OFF"),
            "Desktop user-agent: " + (prefs.getBoolean(KEY_DESKTOP_UA, false) ? "ON" : "OFF"),
            "Cursor speed: " + cursorStepLabel(),
            "Video scroll lock: " + (prefs.getBoolean(KEY_VIDEO_LOCK, true) ? "ON" : "OFF"),
            "Phone remote\u2026",
            "Clear cache",
            "Clear history",
        };
        AlertDialog d = new AlertDialog.Builder(dialogContext())
                .setTitle("\u2699 Browser Settings")
                .setItems(items, (dlg, which) -> {
                    switch (which) {
                        case 0: cycleScale(true); break;
                        case 1: {
                            prefs.edit().putString(KEY_APPEARANCE, browserDark() ? "light" : "dark").apply();
                            applyBrowserAppearance();
                            break;
                        }
                        case 2: {
                            boolean on = !prefs.getBoolean(KEY_ADBLOCK, true);
                            prefs.edit().putBoolean(KEY_ADBLOCK, on).apply();
                            AdBlock.enabled = on; refreshSettingsLabels(); break;
                        }
                        case 3: {
                            boolean on = !prefs.getBoolean(KEY_JS, true);
                            prefs.edit().putBoolean(KEY_JS, on).apply();
                            try { webView.getSettings().setJavaScriptEnabled(on); webView.reload(); }
                            catch (Exception ignored) {}
                            break;
                        }
                        case 4: {
                            boolean on = !prefs.getBoolean(KEY_DESKTOP_UA, false);
                            prefs.edit().putBoolean(KEY_DESKTOP_UA, on).apply();
                            applyUserAgent();
                            try { webView.reload(); } catch (Exception ignored) {}
                            break;
                        }
                        case 5: cycleCursorStep(); break;
                        case 6: { // v1.13: takes effect immediately, no reload
                            boolean on = !prefs.getBoolean(KEY_VIDEO_LOCK, true);
                            prefs.edit().putBoolean(KEY_VIDEO_LOCK, on).apply();
                            refreshSettingsLabels(); break;
                        }
                        case 7: ui.post(this::openRemotePanel); break; // v1.19: panel replaces the sheet (no reopen below)
                        case 8: webView.clearCache(true); Toast.makeText(this, "Cache cleared", Toast.LENGTH_SHORT).show(); break;
                        case 9: webView.clearHistory(); Toast.makeText(this, "History cleared", Toast.LENGTH_SHORT).show(); break;
                        default: break;
                    }
                    if (which != 7) ui.post(this::openBrowserSettings);
                })
                .setNegativeButton("Close", null)
                .create();
        trackDialog(d);
        d.show();
    }

    // ---------------------------------------------------------- v1.19: phone remote

    private RemoteServer remoteServer;
    private boolean remoteHeartbeatOn = false;
    private int lastRemotePairedCount = 0; // v1.21: for the 0 -> >0 pair transition
    private String remoteNote; // one-shot note carried by the next state push
    private TextView remotePairCountView; // live label inside the pairing panel, if open

    // v1.28: auto pairing CARD. Same trigger semantics as the
    // v1.27 auto popup (once per browser session, 700ms after
    // browser entry, only when no phone is paired, 15s countdown
    // since v1.32 - see PAIR_CARD_MS),
    // but rendered as a themed IN-WINDOW card in the browser pane
    // - top-right, below the header - instead of the stock-gray
    // AlertDialog the v1.27 photo showed. Because the card is a
    // plain view (not a dialog window), showing/hiding it cannot
    // disturb a playing video's surface (the v1.21 videokick class
    // does not apply) and it never touches browserDialogDepth.
    // The manual pairing panel (openRemotePanel) is unchanged;
    // opening it supersedes the card. The card is non-focusable:
    // in D-pad mode its CENTER/BACK handling is intercepted at the
    // activity key level in dispatchKeyEvent, and in mouse mode
    // taps are claimed in dispatchTap/dispatchLongTap.
    private boolean autoPairShownThisSession = false; // reset in closeBrowser
    private LinearLayout pairCard;
    private GradientDrawable pairCardBg, pairCardTrackBg;
    private TextView pairCardLabel, pairCardTitle, pairCardBody,
            pairCardAddr, pairCardPinTag, pairCardPinDigits, pairCardCount;
    private LinearLayout pairCardPinRow;
    private FrameLayout pairCardQrTile;
    private ImageView pairCardQr;
    private View pairCardProgressFill;
    private boolean pairCardVisible = false;
    private int pairCardSecondsLeft = 0;
    private int pairCardGen = 0; // invalidates stale delayed hides
    private ValueAnimator pairCardDrain;
    // v1.32: the card's auto-close window, 10s -> 15s (Kurt).
    // ONE constant: the drain animator's duration and the
    // "Closes in Ns" counter both derive from it, on the show
    // path and on every countdown restart (pairCardAction).
    private static final long PAIR_CARD_MS = 15000;

    private final Runnable autoPairShowRunnable = new Runnable() {
        @Override public void run() {
            // The flag is CONSUMED even when re-validation fails:
            // a session that goes fullscreen (or into a dialog)
            // inside the 700ms window does not get a late card
            // after it comes back.
            if (autoPairShownThisSession) return;
            autoPairShownThisSession = true;
            if (currentScreen != browserPane || fsView != null) return;
            if (browserDialogDepth > 0) return;
            if (pairedCount() > 0) return;
            showPairCard();
        }
    };
    private final Runnable pairCardTickRunnable = new Runnable() {
        @Override public void run() {
            if (!pairCardVisible) return;
            pairCardSecondsLeft--;
            if (pairCardSecondsLeft <= 0) { hidePairCard(); return; }
            updatePairCardCountdownText();
            ui.postDelayed(this, 1000);
        }
    };

    /** Schedule the auto pairing card for a fresh browser session. */
    private void maybeAutoPairPopup() {
        if (autoPairShownThisSession) return;
        if (pairedCount() > 0) return;
        ui.removeCallbacks(autoPairShowRunnable);
        ui.postDelayed(autoPairShowRunnable, 700);
    }

    /** Cancel any pending auto card and hide a visible one (screen change / browser exit / fullscreen). */
    private void cancelAutoPairPopup() {
        ui.removeCallbacks(autoPairShowRunnable);
        hidePairCardNow();
    }

    private boolean remoteOn() {
        return remoteServer != null && remoteServer.isRunning();
    }

    /** Build the card into the browser pane root (above the WebView, below the cursor). */
    private void buildPairCard(FrameLayout root) {
        pairCard = new LinearLayout(this);
        pairCard.setOrientation(LinearLayout.VERTICAL);
        pairCard.setPadding(dp(18), dp(14), dp(18), dp(14));
        pairCardBg = new GradientDrawable();
        pairCardBg.setCornerRadius(dp(18));
        pairCard.setBackground(pairCardBg);
        pairCard.setClickable(false);
        pairCard.setFocusable(false);
        pairCard.setFocusableInTouchMode(false);
        pairCard.setVisibility(View.GONE);

        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        LinearLayout col = new LinearLayout(this);
        col.setOrientation(LinearLayout.VERTICAL);

        pairCardLabel = new TextView(this);
        pairCardLabel.setText("PHONE REMOTE");
        pairCardLabel.setTextSize(11);
        pairCardLabel.setTypeface(tf(true));
        try { pairCardLabel.setLetterSpacing(0.14f); } catch (Exception ignored) {}
        col.addView(pairCardLabel);

        pairCardTitle = new TextView(this);
        pairCardTitle.setTextSize(19);
        pairCardTitle.setTypeface(tf(true));
        LinearLayout.LayoutParams titleLp = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        titleLp.topMargin = dp(3);
        col.addView(pairCardTitle, titleLp);

        pairCardBody = new TextView(this);
        pairCardBody.setTextSize(13.5f);
        pairCardBody.setTypeface(tf(false));
        LinearLayout.LayoutParams bodyLp = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        bodyLp.topMargin = dp(4);
        col.addView(pairCardBody, bodyLp);

        pairCardAddr = new TextView(this);
        pairCardAddr.setTextSize(14);
        pairCardAddr.setTypeface(Typeface.MONOSPACE);
        LinearLayout.LayoutParams addrLp = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        addrLp.topMargin = dp(6);
        col.addView(pairCardAddr, addrLp);

        pairCardPinRow = new LinearLayout(this);
        pairCardPinRow.setOrientation(LinearLayout.HORIZONTAL);
        pairCardPinRow.setGravity(Gravity.BOTTOM);
        pairCardPinTag = new TextView(this);
        pairCardPinTag.setText("PIN");
        pairCardPinTag.setTextSize(12);
        pairCardPinTag.setTypeface(tf(true));
        pairCardPinRow.addView(pairCardPinTag);
        pairCardPinDigits = new TextView(this);
        pairCardPinDigits.setTextSize(24);
        pairCardPinDigits.setTypeface(tf(true));
        pairCardPinDigits.setTextColor(ACCENT);
        LinearLayout.LayoutParams digLp = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        digLp.leftMargin = dp(10);
        pairCardPinRow.addView(pairCardPinDigits, digLp);
        LinearLayout.LayoutParams pinLp = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        pinLp.topMargin = dp(6);
        col.addView(pairCardPinRow, pinLp);

        row.addView(col, new LinearLayout.LayoutParams(
                0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));

        pairCardQrTile = new FrameLayout(this);
        GradientDrawable tileBg = new GradientDrawable();
        tileBg.setColor(Color.WHITE); // QR quiet zone: always white, both appearances
        tileBg.setCornerRadius(dp(10));
        pairCardQrTile.setBackground(tileBg);
        pairCardQrTile.setPadding(dp(6), dp(6), dp(6), dp(6));
        pairCardQr = new ImageView(this);
        pairCardQrTile.addView(pairCardQr,
                new FrameLayout.LayoutParams(dp(92), dp(92)));
        LinearLayout.LayoutParams tileLp = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        tileLp.leftMargin = dp(14);
        row.addView(pairCardQrTile, tileLp);
        pairCard.addView(row);

        pairCardCount = new TextView(this);
        pairCardCount.setTextSize(12);
        pairCardCount.setTypeface(tf(false));
        LinearLayout.LayoutParams countLp = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        countLp.topMargin = dp(12);
        pairCard.addView(pairCardCount, countLp);

        // The countdown made visual: a 3dp accent track under the
        // content, drained full -> 0 over PAIR_CARD_MS by a
        // ValueAnimator (the text above it ticks per second).
        FrameLayout track = new FrameLayout(this);
        pairCardTrackBg = new GradientDrawable();
        pairCardTrackBg.setCornerRadius(dp(2));
        track.setBackground(pairCardTrackBg);
        pairCardProgressFill = new View(this);
        GradientDrawable fillBg = new GradientDrawable();
        fillBg.setColor(ACCENT);
        fillBg.setCornerRadius(dp(2));
        pairCardProgressFill.setBackground(fillBg);
        pairCardProgressFill.setPivotX(0f);
        track.addView(pairCardProgressFill, new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        LinearLayout.LayoutParams trackLp = new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, dp(3));
        trackLp.topMargin = dp(8);
        pairCard.addView(track, trackLp);

        FrameLayout.LayoutParams lp = new FrameLayout.LayoutParams(
                dp(430), ViewGroup.LayoutParams.WRAP_CONTENT);
        lp.gravity = Gravity.TOP | Gravity.RIGHT;
        lp.rightMargin = dp(20);
        lp.topMargin = dp(114); // placeholder; showPairCard sets it from the header height
        root.addView(pairCard, lp);
        applyPairCardTheme();
    }

    /** Card colors follow the browser appearance (dark/light chrome). */
    private void applyPairCardTheme() {
        if (pairCard == null) return;
        boolean dark = browserDark();
        pairCardBg.setColor(dark ? Color.argb(235, 0x1C, 0x1C, 0x1E)
                                 : Color.argb(235, 0xF2, 0xF2, 0xF7));
        pairCardBg.setStroke(dp(1), dark ? Color.argb(36, 255, 255, 255) : tbBorder());
        int text = tbText();
        int dim = dark ? DIM : Color.parseColor("#8E8E93");
        pairCardTitle.setTextColor(text);
        pairCardAddr.setTextColor(text);
        pairCardLabel.setTextColor(dim);
        pairCardBody.setTextColor(dim);
        pairCardCount.setTextColor(dim);
        pairCardPinTag.setTextColor(dim);
        pairCardTrackBg.setColor(dark ? Color.argb(38, 255, 255, 255)
                                      : Color.argb(31, 0, 0, 0));
        // PIN digits stay ACCENT and the QR tile stays white in
        // both appearances (set once at build / in the drawable).
    }

    /** This build's versionCode, straight from the package. */
    private int appVersionCode() {
        try {
            return getPackageManager().getPackageInfo(getPackageName(), 0).versionCode;
        } catch (Exception ignored) {
            return 0;
        }
    }

    /**
     * v1.32: the pairing address is VERSIONED - .../?v=<versionCode>.
     * The unversioned address was identical for every build, so a
     * phone tab restored from its snapshot/cache at the old address
     * could keep running a pre-update copy of the remote page (its
     * WebSocket reconnects, so the stale page looks alive) - that
     * is how the v1.31 text removals "never happened" on Kurt's
     * phone. A new versionCode makes the phone fetch the fresh
     * document. RemoteServer strips the query string when routing
     * (and the page's WebSocket uses location.host), so nothing
     * server-side changes.
     */
    private String remotePageUrl() {
        return "http://" + RemoteServer.localIpAddress(this)
                + ":" + RemoteServer.PORT + "/?v=" + appVersionCode();
    }

    /** Fill the card for the CURRENT server state (on: address + PIN + QR; off: turn-on prompt). */
    private void fillPairCard() {
        if (pairCard == null) return;
        applyPairCardTheme();
        if (remoteOn()) {
            pairCardTitle.setText("Pair your phone");
            pairCardBody.setVisibility(View.GONE);
            String url = remotePageUrl();
            pairCardAddr.setText(url);
            pairCardAddr.setVisibility(View.VISIBLE);
            pairCardPinDigits.setText(spacedPin(remoteServer.pin()));
            pairCardPinRow.setVisibility(View.VISIBLE);
            Bitmap qr = null;
            try { qr = renderQrBitmap(url, dp(92)); } catch (Exception ignored) {}
            if (qr != null) {
                pairCardQr.setImageBitmap(qr);
                pairCardQrTile.setVisibility(View.VISIBLE);
            } else {
                pairCardQrTile.setVisibility(View.GONE);
            }
        } else {
            pairCardTitle.setText("Phone remote is off");
            pairCardBody.setText("Tap this card or press OK to turn it on");
            pairCardBody.setVisibility(View.VISIBLE);
            pairCardAddr.setVisibility(View.GONE);
            pairCardPinRow.setVisibility(View.GONE);
            pairCardQrTile.setVisibility(View.GONE);
        }
    }

    private void showPairCard() {
        if (pairCard == null) return;
        pairCardGen++;
        fillPairCard();
        // Anchor: top-right, 10dp below the header block (the
        // v1.29 floating bar + progress), 20dp off the right edge.
        FrameLayout.LayoutParams lp = (FrameLayout.LayoutParams) pairCard.getLayoutParams();
        int hb = headerBlock != null ? headerBlock.getHeight() : 0;
        if (hb <= 0) hb = dp(90);
        lp.topMargin = hb + dp(10);
        pairCard.setLayoutParams(lp);
        pairCardVisible = true;
        pairCard.setVisibility(View.VISIBLE);
        pairCard.animate().cancel();
        pairCard.setTranslationX(dp(40));
        pairCard.setAlpha(0f);
        pairCard.animate().translationX(0f).alpha(1f).setDuration(240)
                .setInterpolator(new DecelerateInterpolator()).start();
        startPairCardCountdown();
        refreshCursor("paircard-show"); // v1.32
    }

    private void hidePairCard() { hidePairCard(true); }

    private void hidePairCardNow() { hidePairCard(false); }

    private void hidePairCard(boolean animate) {
        stopPairCardCountdown();
        if (pairCard == null || !pairCardVisible) return;
        refreshCursor("paircard-hide"); // v1.32: only on an actual hide, not every no-op call
        pairCardVisible = false;
        pairCardGen++;
        pairCard.animate().cancel();
        if (animate) {
            pairCard.animate().translationX(dp(40)).alpha(0f).setDuration(180)
                    .setInterpolator(new DecelerateInterpolator())
                    .withEndAction(() -> {
                        if (!pairCardVisible && pairCard != null) {
                            pairCard.setVisibility(View.GONE);
                            pairCard.setAlpha(1f);
                            pairCard.setTranslationX(0f);
                        }
                    }).start();
        } else {
            pairCard.setVisibility(View.GONE);
            pairCard.setAlpha(1f);
            pairCard.setTranslationX(0f);
        }
    }

    private void startPairCardCountdown() {
        stopPairCardCountdown();
        pairCardSecondsLeft = (int) (PAIR_CARD_MS / 1000);
        updatePairCardCountdownText();
        if (pairCardProgressFill != null) {
            pairCardProgressFill.setScaleX(1f);
            pairCardDrain = ValueAnimator.ofFloat(1f, 0f);
            pairCardDrain.setDuration(PAIR_CARD_MS);
            pairCardDrain.setInterpolator(new LinearInterpolator());
            pairCardDrain.addUpdateListener(a -> {
                if (pairCardProgressFill != null)
                    pairCardProgressFill.setScaleX((Float) a.getAnimatedValue());
            });
            pairCardDrain.start();
        }
        ui.postDelayed(pairCardTickRunnable, 1000);
    }

    private void stopPairCardCountdown() {
        ui.removeCallbacks(pairCardTickRunnable);
        if (pairCardDrain != null) { pairCardDrain.cancel(); pairCardDrain = null; }
    }

    private void updatePairCardCountdownText() {
        if (pairCardCount != null)
            pairCardCount.setText("Closes in " + pairCardSecondsLeft + "s");
    }

    /**
     * The card's one action, shared by the mouse/phone tap claim
     * (dispatchTap), the D-pad CENTER interception
     * (dispatchKeyEvent) and nothing else: server OFF -> turn the
     * remote on (the panel's exact turn-on sequence), rebuild to
     * the ON content and restart the full countdown; server ON ->
     * the card hands over to the full manual pairing panel.
     */
    private void pairCardAction() {
        if (!pairCardVisible) return;
        if (!remoteOn()) {
            prefs.edit().putBoolean(KEY_REMOTE, true).apply();
            stopRemoteServer(); // clear any dead instance
            startRemoteServer();
            refreshSettingsLabels();
            fillPairCard();
            startPairCardCountdown();
        } else {
            hidePairCard();
            openRemotePanel();
        }
    }

    /** Hit test in window coords (the tap path's coordinate space). */
    private boolean pairCardContains(float x, float y) {
        if (pairCard == null || pairCard.getWidth() <= 0) return false;
        int[] loc = new int[2];
        pairCard.getLocationInWindow(loc);
        return x >= loc[0] && x < loc[0] + pairCard.getWidth()
                && y >= loc[1] && y < loc[1] + pairCard.getHeight();
    }

    private int pairedCount() {
        return remoteServer != null ? remoteServer.sessionCount() : 0;
    }

    /**
     * v1.20: Phone toolbar button label tints accent while paired.
     * v1.22: the Home header Phone button follows the same rule
     * (its resting color is Home's TEXT, not the browser toolbar's).
     */
    private void refreshPhoneBtn() {
        boolean paired = pairedCount() > 0;
        // v1.25: the browser-toolbar Phone button is icon-only now;
        // the paired accent drives the icon's color filter. The
        // Home Phone button stays text (v1.22) and keeps its rule.
        if (tbPhone != null && phoneIconDrawable != null)
            phoneIconDrawable.setColorFilter(paired ? ACCENT : tbText(), PorterDuff.Mode.SRC_IN);
        if (homePhoneBtn != null) homePhoneBtn.setTextColor(paired ? ACCENT : TEXT);
    }

    // -------------------------------------------- v1.25: toolbar tooltips

    /** Name bubble text for a toolbar element (Mode = current sub-mode). */
    private String toolbarTipText(View v) {
        if (v == tbBack) return "Back";
        if (v == tbFwd) return "Forward";
        if (v == addrCapsule) return "URL";
        if (v == tbReload) return "Reload";
        if (v == tbMode) return browserMouse ? "Mouse" : "D-pad";
        if (v == tbPhone) return "Phone";
        if (v == tbGear) return "Settings";
        if (v == tbDone) return "Done";
        return null;
    }

    /** Tooltips live only over the visible browser toolbar, one at a time. */
    private boolean toolbarTipSuppressed() {
        // v1.28: the pairing prompt is an in-window CARD now, not a
        // tracked dialog, so the browserDialogDepth clause no
        // longer covers it - pairCardVisible takes that clause's
        // place: no tooltip competes with the card while it is up.
        return toolbarTipView == null || currentScreen != browserPane
                || fsView != null || browserDialogDepth > 0 || headerHidden
                || pairCardVisible;
    }

    private void scheduleToolbarTip(View v, long delayMs) {
        if (v == null || toolbarTipText(v) == null || toolbarTipSuppressed()) return;
        if (toolbarTipTarget == v && toolbarTipView.getVisibility() == View.VISIBLE)
            return; // already naming this element
        if (toolbarTipView.getVisibility() == View.VISIBLE) {
            // A different element's bubble is up: drop it now, the
            // new one still waits out its (short) dwell.
            toolbarTipView.setVisibility(View.GONE);
            toolbarTipTarget = null;
        }
        pendingTipTarget = v;
        ui.removeCallbacks(toolbarTipShowRunnable);
        ui.postDelayed(toolbarTipShowRunnable, delayMs);
    }

    private void showToolbarTip(View v) {
        if (toolbarTipSuppressed()) return;
        String text = toolbarTipText(v);
        if (text == null) return;
        toolbarTipTarget = v;
        toolbarTipView.setText(text);
        positionToolbarTip(v);
        toolbarTipView.setVisibility(View.VISIBLE);
    }

    private void hideToolbarTip() {
        ui.removeCallbacks(toolbarTipShowRunnable);
        pendingTipTarget = null;
        toolbarTipTarget = null;
        if (toolbarTipView != null) toolbarTipView.setVisibility(View.GONE);
    }

    /**
     * Center the bubble on the element (pane coordinates, clamped
     * inside the pane). Vertically it goes ABOVE the element when
     * there is room; the toolbar sits at the very top of the
     * screen, so in practice there is not, and it lands just below
     * the element instead - still inside the pane, never covering
     * the button it names.
     */
    private void positionToolbarTip(View v) {
        if (browserPane == null || v.getWidth() <= 0) return;
        FrameLayout.LayoutParams lp =
                (FrameLayout.LayoutParams) toolbarTipView.getLayoutParams();
        try {
            int[] paneLoc = new int[2]; browserPane.getLocationInWindow(paneLoc);
            int[] elLoc = new int[2]; v.getLocationInWindow(elLoc);
            int elLeft = elLoc[0] - paneLoc[0], elTop = elLoc[1] - paneLoc[1];
            int elCx = elLeft + v.getWidth() / 2;
            int wSpec = View.MeasureSpec.makeMeasureSpec(
                    Math.max(0, browserPane.getWidth()), View.MeasureSpec.AT_MOST);
            int hSpec = View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED);
            toolbarTipView.measure(wSpec, hSpec);
            int tw = toolbarTipView.getMeasuredWidth();
            int th = toolbarTipView.getMeasuredHeight();
            int left = Math.max(dp(4), Math.min(
                    Math.max(dp(4), browserPane.getWidth() - tw - dp(4)), elCx - tw / 2));
            int above = elTop - th - dp(6);
            int top = above >= dp(2) ? above : elTop + v.getHeight() + dp(6);
            lp.leftMargin = left; lp.topMargin = top;
            toolbarTipView.setLayoutParams(lp);
        } catch (Exception ignored) {}
    }

    /**
     * Mouse-hover tracking, called at the end of every moveCursor():
     * hit-test the cursor (window coordinates, the same space
     * getLocationInWindow reports) against the toolbar elements.
     * A new element starts a 350ms dwell; leaving all elements,
     * or any suppressed state, hides the bubble at once.
     */
    private void updateToolbarHover() {
        if (!browserMouse || toolbarTipSuppressed()) { hideToolbarTip(); return; }
        int[] rootLoc = new int[2];
        if (windowRoot != null) windowRoot.getLocationInWindow(rootLoc);
        float wx = cursorX + rootLoc[0], wy = cursorY + rootLoc[1];
        View hit = null;
        // v1.29: iterate in REVERSE - Reload now lives INSIDE the
        // address capsule's bounds, so the specific child must win
        // the hit test over the capsule that contains it.
        for (int i = toolbarViews.size() - 1; i >= 0; i--) {
            View v = toolbarViews.get(i);
            if (v == null || v.getWidth() <= 0 || v.getVisibility() != View.VISIBLE) continue;
            int[] loc = new int[2]; v.getLocationInWindow(loc);
            if (wx >= loc[0] && wx < loc[0] + v.getWidth()
                    && wy >= loc[1] && wy < loc[1] + v.getHeight()) { hit = v; break; }
        }
        if (hit == null) { hideToolbarTip(); return; }
        if (hit == toolbarTipTarget && toolbarTipView.getVisibility() == View.VISIBLE) return;
        if (hit == pendingTipTarget) return; // dwell already running for it
        scheduleToolbarTip(hit, 350);
    }

    private final Runnable remoteHeartbeat = new Runnable() {
        @Override public void run() {
            if (remoteServer == null) { remoteHeartbeatOn = false; return; }
            pushRemoteState();
            ui.postDelayed(this, 2500);
        }
    };

    private void startRemoteServer() {
        if (remoteServer != null) return;
        remoteServer = new RemoteServer(this, ui, remoteHost);
        remoteServer.start();
        if (!remoteHeartbeatOn) {
            remoteHeartbeatOn = true;
            ui.postDelayed(remoteHeartbeat, 2500);
        }
        pushRemoteState();
    }

    private void stopRemoteServer() {
        if (remoteServer == null) return;
        remoteServer.stop();
        remoteServer = null;
        refreshPhoneBtn(); // v1.20: no server = no pairing = normal tint
    }

    private void pushRemoteState() {
        refreshPhoneBtn(); // v1.20: heartbeat (2.5s) funnels through here - tint stays fresh
        if (remoteServer == null) return;
        try { remoteServer.broadcast(buildRemoteStateJson()); }
        catch (Exception ignored) {}
    }

    private String buildRemoteStateJson() {
        JSONObject o = new JSONObject();
        try {
            o.put("t", "state");
            o.put("title", webView != null && webView.getTitle() != null ? webView.getTitle() : "");
            o.put("url", webView != null && webView.getUrl() != null ? webView.getUrl() : "");
            o.put("fs", fsLikeActive());
            o.put("mouse", browserMouse);
            o.put("browser", currentScreen == browserPane);
            // v1.22: which screen the TV is on (the phone page keys
            // its mode caption off this; "browser" stays for compat).
            o.put("screen", currentScreen == homeScroll ? "home"
                    : currentScreen == searchPane ? "search"
                    : currentScreen == settingsScroll ? "settings"
                    : currentScreen == browserPane ? "browser" : "other");
            o.put("canBack", webView != null && webView.canGoBack());
            o.put("canFwd", webView != null && webView.canGoForward());
            o.put("paired", remoteServer != null ? remoteServer.sessionCount() : 0);
            if (remoteNote != null) { o.put("note", remoteNote); remoteNote = null; }
        } catch (Exception ignored) {}
        return o.toString();
    }

    // Extra C: insert text into the page's focused editable element
    // (top document, then same-origin iframes). Returns '1' on success.
    private static final String REMOTE_TYPE_JS =
            "(function(){try{"
            + "function ins(doc){var el=doc.activeElement;if(!el)return false;"
            + "var tag=(el.tagName||'').toLowerCase();"
            + "if(tag!=='input'&&tag!=='textarea'&&!el.isContentEditable)return false;"
            + "var text=%s;"
            + "try{"
            + "if(tag!=='input'&&tag!=='textarea'){doc.execCommand('insertText',false,text);}"
            + "else if(el.setRangeText&&typeof el.selectionStart==='number'){"
            + "el.setRangeText(text,el.selectionStart,el.selectionEnd,'end');}"
            + "else{el.value+=text;}"
            + "}catch(e){try{el.value+=text;}catch(e2){}}"
            + "try{el.dispatchEvent(new Event('input',{bubbles:true}));}catch(e){}"
            + "return true;}"
            + "if(ins(document))return '1';"
            + "var fs=document.querySelectorAll('iframe');"
            + "for(var i=0;i<fs.length;i++){try{var d=fs[i].contentDocument;"
            + "if(d&&ins(d))return '1';}catch(e){}}"
            + "return '0';}catch(e){return '0';}})()";

    // Extra C: Enter key on the focused element (best-effort form submit).
    private static final String REMOTE_ENTER_JS =
            "(function(){try{"
            + "function fire(doc){var el=doc.activeElement;if(!el)return false;"
            + "var types=['keydown','keypress','keyup'];"
            + "for(var i=0;i<types.length;i++){"
            + "var ev;try{ev=new KeyboardEvent(types[i],{key:'Enter',code:'Enter',bubbles:true,cancelable:true});}"
            + "catch(e){ev=doc.createEvent('Event');ev.initEvent(types[i],true,true);}"
            + "try{Object.defineProperty(ev,'keyCode',{value:13});"
            + "Object.defineProperty(ev,'which',{value:13});}catch(e){}"
            + "el.dispatchEvent(ev);}"
            + "return true;}"
            + "if(fire(document))return '1';"
            + "var fs=document.querySelectorAll('iframe');"
            + "for(var i=0;i<fs.length;i++){try{var d=fs[i].contentDocument;"
            + "if(d&&fire(d))return '1';}catch(e){}}"
            + "return '0';}catch(e){return '0';}})()";

    // Extra D: seek the biggest playing (else first) <video> by N seconds.
    private static final String REMOTE_SEEK_JS =
            "(function(){try{"
            + "function best(doc){var vs=doc.querySelectorAll('video');"
            + "var bp=null,ba=-1,first=null;"
            + "for(var i=0;i<vs.length;i++){var v=vs[i];if(!first)first=v;"
            + "if(!v.paused){var r=v.getBoundingClientRect();var a=r.width*r.height;"
            + "if(a>ba){ba=a;bp=v;}}}"
            + "return bp||first;}"
            + "var v=best(document);"
            + "if(!v){var fs=document.querySelectorAll('iframe');"
            + "for(var i=0;i<fs.length;i++){try{var d=fs[i].contentDocument;"
            + "if(d){v=best(d);if(v)break;}}catch(e){}}}"
            + "if(v){try{v.currentTime=Math.max(0,(v.currentTime||0)+(%d));}catch(e){}}"
            + "}catch(e){}})()";

    /**
     * RemoteServer.Host: every callback arrives on the UI thread.
     * Page commands are browser-gated (the state push tells phones
     * when the browser is not open); volume is global.
     */
    private final RemoteServer.Host remoteHost = new RemoteServer.Host() {
        @Override public void remoteMove(float dx, float dy) {
            if (currentScreen != browserPane) return;
            moveCursor(dx, dy); // gain applied server-side (trackpad feel)
        }
        @Override public void remoteTap() {
            if (currentScreen != browserPane) return;
            if (fsView != null) dispatchFsTap(cursorX, cursorY);
            else dispatchTap(cursorX, cursorY);
            if (fsLikeActive()) pokeFsCursor(); // a tap is interaction: un-fade + re-arm
        }
        @Override public void remoteLongTap() {
            if (currentScreen != browserPane) {
                // v1.25: the phone's trackpad used on a TV menu
                // (trackpad mode manually picked while off-browser)
                // long-presses like the Apple TV Remote's touch
                // surface: a long DPAD_CENTER through the input
                // queue, so Home tiles open their long-press menus.
                injectKeyHold(KeyEvent.KEYCODE_DPAD_CENTER, 600);
                return;
            }
            if (fsView != null) dispatchFsTap(cursorX, cursorY);
            else dispatchLongTap(cursorX, cursorY);
            if (fsLikeActive()) pokeFsCursor();
        }
        @Override public void remoteScroll(int dyRaw) {
            if (currentScreen != browserPane) return;
            float density = getResources().getDisplayMetrics().density;
            doScrollBy(0, Math.round(dyRaw * density)); // clamps/freezes inside govern
        }
        @Override public void remoteBack() {
            if (currentScreen == browserPane && webView != null && webView.canGoBack())
                webView.goBack();
        }
        @Override public void remoteForward() {
            if (currentScreen == browserPane && webView != null && webView.canGoForward())
                webView.goForward();
        }
        @Override public void remoteReload() {
            if (currentScreen == browserPane && webView != null) webView.reload();
        }
        @Override public void remoteDone() {
            if (currentScreen == browserPane) closeBrowser();
        }
        @Override public void remotePlayPause() {
            if (currentScreen != browserPane) return;
            // v1.31: one shared layered implementation with the
            // physical media keys. In real fullscreen it still ends
            // in the FS center tap: the shared path fires the
            // media-key fallback, and the injected key lands back
            // in handleFullscreenKey (the pass-through in
            // dispatchKeyEvent is non-FS only).
            togglePlayPause("phone");
        }
        @Override public void remoteUrl(String u) {
            if (webView == null) return;
            String t = u == null ? "" : u.trim();
            if (t.isEmpty()) return;
            if (currentScreen == browserPane) { loadResolved(t); return; }
            // v1.28 (Part 4): the URL panel is visible in D-pad
            // mode too now, so a URL sent from a menu screen does
            // what the user expects - it opens the browser on the
            // resolved address, like tapping a generic entry.
            String resolved = resolveUrlString(t);
            if (resolved == null) return;
            String host = hostOf(resolved);
            openEntry(new Entry("misc", "Browser", host, resolved, host, false, ""));
        }
        @Override public void remoteType(final String text) {
            if (text == null || text.isEmpty()) return;
            if (currentScreen != browserPane || webView == null) {
                // v1.28 (Part 4): with the Type panel visible in
                // D-pad mode too, an off-browser send gets the same
                // honest note an in-browser miss gets (below),
                // instead of silence.
                remoteNote = "No text field focused on TV";
                pushRemoteState();
                return;
            }
            try {
                webView.evaluateJavascript(
                        String.format(REMOTE_TYPE_JS, JSONObject.quote(text)),
                        value -> {
                            boolean ok = value != null && value.contains("1");
                            if (!ok) {
                                remoteNote = "No text field focused on TV";
                                pushRemoteState();
                            }
                        });
            } catch (Exception ignored) {}
        }
        @Override public void remoteEnterKey() {
            if (currentScreen != browserPane || webView == null) return;
            try { webView.evaluateJavascript(REMOTE_ENTER_JS, null); }
            catch (Exception ignored) {}
        }
        @Override public void remoteKey(String k) {
            // v1.22 D-pad mode: deliberately NOT browser-gated -
            // these keys drive whichever screen/dialog is current.
            if (k == null) return;
            int code;
            switch (k) {
                case "up": code = KeyEvent.KEYCODE_DPAD_UP; break;
                case "down": code = KeyEvent.KEYCODE_DPAD_DOWN; break;
                case "left": code = KeyEvent.KEYCODE_DPAD_LEFT; break;
                case "right": code = KeyEvent.KEYCODE_DPAD_RIGHT; break;
                case "ok": code = KeyEvent.KEYCODE_DPAD_CENTER; break;
                case "back": code = KeyEvent.KEYCODE_BACK; break;
                case "menu": code = KeyEvent.KEYCODE_MENU; break;
                default: return;
            }
            injectKey(code);
        }
        @Override public void remoteVolume(int dir) {
            try {
                AudioManager am = (AudioManager) getSystemService(Context.AUDIO_SERVICE);
                if (am != null) am.adjustStreamVolume(AudioManager.STREAM_MUSIC,
                        dir >= 0 ? AudioManager.ADJUST_RAISE : AudioManager.ADJUST_LOWER,
                        AudioManager.FLAG_SHOW_UI);
            } catch (Exception ignored) {}
        }
        @Override public void remoteSeek(int seconds) {
            if (currentScreen != browserPane || webView == null) return;
            // v1.26: the seek flash fires immediately with the command.
            showActionFeedback(fbBitmap(seconds >= 0 ? "fwd" : "back"));
            try {
                webView.evaluateJavascript(
                        String.format(Locale.US, REMOTE_SEEK_JS, seconds), null);
            } catch (Exception ignored) {}
        }
        @Override public void remoteSessionsChanged() {
            int count = pairedCount();
            boolean firstPhone = lastRemotePairedCount == 0 && count > 0;
            lastRemotePairedCount = count;
            if (remotePairCountView != null && remoteServer != null)
                remotePairCountView.setText(
                        "Phones paired: " + remoteServer.sessionCount() + " / 2");
            refreshPhoneBtn(); // v1.20
            refreshCursor("remote-sessions"); // v1.32: phone connect/disconnect re-asserts the cursor (Kurt's repro is phone-connected)
            // v1.21: the first phone connecting is the exact
            // reported black-video moment - kick the layer.
            if (firstPhone) kickVideoLayer("pair");
            // v1.28: a phone pairing while the auto CARD is up
            // ends it: stop the countdown, swap the card to its
            // connected state, and slide it out after ~800ms.
            if (count > 0 && pairCardVisible) {
                stopPairCardCountdown();
                if (pairCardTitle != null) pairCardTitle.setText("Phone connected ✓");
                if (pairCardCount != null) pairCardCount.setText("Paired!");
                final int gen = pairCardGen;
                ui.postDelayed(() -> {
                    if (gen == pairCardGen && pairCardVisible) hidePairCard();
                }, 800);
            }
            pushRemoteState();
        }
    };

    /**
     * v1.23: phone-remote keys go through the system input queue
     * first (Instrumentation.sendKeySync on the injection thread -
     * see the input-injection section below). The queue targets
     * whichever window is foreground, so keys reach WebView native
     * popups (e.g. a <select> list, which lives in its own window)
     * as well as dialogs and the activity, and this app's own
     * dispatchKeyEvent interception still runs for them exactly as
     * for hardware keys, because that IS how hardware keys arrive.
     * If injection throws, fall back per-event to the v1.22 direct
     * dispatch below.
     */
    private void injectKey(final int code) {
        injectExecutor().execute(() -> {
            try {
                if (injectIns == null) injectIns = new Instrumentation();
                long now = SystemClock.uptimeMillis();
                injectIns.sendKeySync(new KeyEvent(now, now,
                        KeyEvent.ACTION_DOWN, code, 0));
                long upNow = SystemClock.uptimeMillis();
                injectIns.sendKeySync(new KeyEvent(now, upNow,
                        KeyEvent.ACTION_UP, code, 0));
                logInjectOk();
            } catch (Exception e) {
                Log.d("HeckYeahFS", "inject fallback key " + code + ": "
                        + e.getClass().getSimpleName());
                ui.post(() -> injectKeyViaDispatch(code));
            }
        });
    }

    /**
     * v1.25: injectKey's hold variant - the key stays DOWN for
     * holdMs before the UP, a genuine long press through the
     * system input queue (used for menu long-presses from the
     * phone trackpad). Same thread and fallback discipline as
     * injectKey; the quick DOWN+UP path is untouched.
     */
    private void injectKeyHold(final int code, final long holdMs) {
        injectExecutor().execute(() -> {
            try {
                if (injectIns == null) injectIns = new Instrumentation();
                long now = SystemClock.uptimeMillis();
                injectIns.sendKeySync(new KeyEvent(now, now,
                        KeyEvent.ACTION_DOWN, code, 0));
                try { Thread.sleep(holdMs); }
                catch (InterruptedException ie) { Thread.currentThread().interrupt(); }
                long upNow = SystemClock.uptimeMillis();
                injectIns.sendKeySync(new KeyEvent(now, upNow,
                        KeyEvent.ACTION_UP, code, 0));
                logInjectOk();
            } catch (Exception e) {
                Log.d("HeckYeahFS", "inject fallback keyhold " + code + ": "
                        + e.getClass().getSimpleName());
                ui.post(() -> injectKeyHoldViaDispatch(code, holdMs));
            }
        });
    }

    /** injectKeyHold fallback: direct dispatch, UP posted after the hold. */
    private void injectKeyHoldViaDispatch(final int code, final long holdMs) {
        final long now = SystemClock.uptimeMillis();
        try {
            AlertDialog d = openDialog;
            KeyEvent down = new KeyEvent(now, now, KeyEvent.ACTION_DOWN, code, 0);
            if (d != null && d.isShowing()) d.dispatchKeyEvent(down);
            else dispatchKeyEvent(down);
        } catch (Exception ignored) {}
        ui.postDelayed(() -> {
            try {
                AlertDialog d = openDialog;
                KeyEvent up = new KeyEvent(now, SystemClock.uptimeMillis(),
                        KeyEvent.ACTION_UP, code, 0);
                if (d != null && d.isShowing()) d.dispatchKeyEvent(up);
                else dispatchKeyEvent(up);
            } catch (Exception ignored) {}
        }, holdMs);
    }

    /**
     * v1.22 fallback (kept): deliver a synthesized key by direct
     * dispatch. If a tracked dialog is showing, the
     * key goes to the DIALOG's own dispatchKeyEvent - physical keys
     * target the foreground window, so D-pad moves dialog focus and
     * BACK cancels the dialog, never the screen behind it. Otherwise
     * the key goes through THIS activity's dispatchKeyEvent: the
     * app's interception layer (MENU handling, fullscreen-first,
     * browser key swallowing) runs exactly as for hardware keys,
     * and unhandled keys fall through super.dispatchKeyEvent to the
     * focused view / focus pipeline. BACK needs no special case:
     * it is not a browser key (see isBrowserKey), so the pair falls
     * to the stock Activity callback - onKeyDown starts BACK
     * tracking, onKeyUp fires this app's onBackPressed override
     * (browser page-back / close, screen back, fullscreen exit).
     */
    private void injectKeyViaDispatch(int code) {
        long now = SystemClock.uptimeMillis();
        KeyEvent down = new KeyEvent(now, now, KeyEvent.ACTION_DOWN, code, 0);
        KeyEvent up = new KeyEvent(now, now, KeyEvent.ACTION_UP, code, 0);
        try {
            AlertDialog d = openDialog;
            if (d != null && d.isShowing()) {
                d.dispatchKeyEvent(down);
                d.dispatchKeyEvent(up);
            } else {
                dispatchKeyEvent(down);
                dispatchKeyEvent(up);
            }
        } catch (Exception ignored) {}
    }

    /**
     * Pairing panel (TV side): address, QR, PIN, session controls.
     * Manual opens only (Phone buttons, menu, settings). v1.28: the
     * v1.27 auto variant of this dialog is GONE - the browser-start
     * auto prompt is the themed in-window CARD now (see the
     * pairCard block); opening this panel supersedes the card.
     */
    private void openRemotePanel() {
        hidePairCardNow(); // v1.28: the manual panel supersedes the auto card
        boolean on = remoteServer != null && remoteServer.isRunning();
        LinearLayout box = new LinearLayout(dialogContext());
        box.setOrientation(LinearLayout.VERTICAL);
        box.setPadding(dp(24), dp(8), dp(24), dp(4));
        if (!on) {
            box.addView(tv("Phone remote is OFF. Turn it on to control the browser "
                    + "from a phone on the same Wi-Fi network. Pairing uses a "
                    + "PIN shown here; the server is off by default.", 15, TEXT, false));
            AlertDialog d = new AlertDialog.Builder(dialogContext())
                    .setTitle("Phone remote")
                    .setView(box)
                    .setPositiveButton("Turn on", (dlg, w) -> {
                        prefs.edit().putBoolean(KEY_REMOTE, true).apply();
                        stopRemoteServer(); // clear any dead instance
                        startRemoteServer();
                        refreshSettingsLabels();
                        ui.post(() -> openRemotePanel());
                    })
                    .setNegativeButton("Close", null)
                    .create();
            trackDialog(d);
            d.show();
            return;
        }
        final String url = remotePageUrl(); // v1.32: versioned, see remotePageUrl()
        box.addView(tv(url, 17, TEXT, true));
        TextView alt = tv("or http://HeckYeah-TV.local:" + RemoteServer.PORT
                + "/?v=" + appVersionCode()
                + " (extra B: name resolution is reliable on iPhone; "
                + "Android phones may need the IP address above)", 12, DIM, false);
        alt.setPadding(0, dp(4), 0, 0);
        box.addView(alt);
        TextView pinView = tv("PIN  " + spacedPin(remoteServer.pin()), 30, TEXT, true);
        pinView.setPadding(0, dp(14), 0, 0);
        box.addView(pinView);
        try {
            Bitmap qr = renderQrBitmap(url, dp(220));
            if (qr != null) {
                ImageView iv = new ImageView(this);
                iv.setImageBitmap(qr);
                LinearLayout.LayoutParams lp =
                        new LinearLayout.LayoutParams(dp(220), dp(220));
                lp.gravity = Gravity.CENTER_HORIZONTAL;
                lp.topMargin = dp(12);
                box.addView(iv, lp);
            }
        } catch (Exception ignored) {}
        remotePairCountView = tv("Phones paired: " + remoteServer.sessionCount() + " / 2",
                14, DIM, false);
        remotePairCountView.setPadding(0, dp(12), 0, 0);
        box.addView(remotePairCountView);
        AlertDialog d = new AlertDialog.Builder(dialogContext())
                .setTitle("Phone remote")
                .setView(box)
                .setPositiveButton("Close", null)
                .setNegativeButton("New PIN", (dlg, w) -> {
                    if (remoteServer != null) remoteServer.regeneratePinAndDropSessions();
                    ui.post(() -> openRemotePanel());
                })
                .setNeutralButton("Disconnect all", (dlg, w) -> {
                    if (remoteServer != null) remoteServer.dropAllSessions();
                    ui.post(() -> openRemotePanel());
                })
                .create();
        // v1.21: use the shared dialog funnel (it previously
        // open-coded the same counting) so this panel's dismiss
        // also runs the depth-0 video kick; the extra runnable
        // keeps its label cleanup.
        trackDialog(d, () -> {
            remotePairCountView = null;
        });
        d.show();
    }

    private static String spacedPin(String pin) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < pin.length(); i++) {
            if (i > 0) sb.append(' ');
            sb.append(pin.charAt(i));
        }
        return sb.toString();
    }

    /** Extra A: QR bitmap (dark modules on white, 4-module quiet zone). */
    private Bitmap renderQrBitmap(String text, int px) {
        QrCode qr = QrCode.encodeText(text);
        int n = qr.size();
        int quiet = 4;
        int total = n + quiet * 2;
        int scale = Math.max(1, px / total);
        Bitmap bmp = Bitmap.createBitmap(total * scale, total * scale,
                Bitmap.Config.ARGB_8888);
        Canvas canvas = new Canvas(bmp);
        canvas.drawColor(Color.WHITE);
        Paint paint = new Paint();
        paint.setColor(Color.BLACK);
        for (int y = 0; y < n; y++) {
            for (int x = 0; x < n; x++) {
                if (qr.getModule(x, y)) {
                    canvas.drawRect((x + quiet) * scale, (y + quiet) * scale,
                            (x + quiet + 1) * scale, (y + quiet + 1) * scale, paint);
                }
            }
        }
        return bmp;
    }

    private void applyUserAgent() {
        try {
            webView.getSettings().setUserAgentString(
                    prefs.getBoolean(KEY_DESKTOP_UA, false) ? DESKTOP_UA : null);
        } catch (Exception ignored) {}
    }

    private void setupWebView() {
        WebSettings s = webView.getSettings();
        s.setJavaScriptEnabled(prefs.getBoolean(KEY_JS, true));
        s.setDomStorageEnabled(true);
        s.setDatabaseEnabled(true);
        s.setAllowFileAccess(true);
        s.setAllowContentAccess(true);
        // Scaling: wide viewport + overview so pages fit the TV width
        // instead of rendering zoomed-in; TEXT_AUTOSIZING keeps text sane.
        s.setUseWideViewPort(true);
        s.setLoadWithOverviewMode(true);
        try { s.setLayoutAlgorithm(WebSettings.LayoutAlgorithm.TEXT_AUTOSIZING); } catch (Exception ignored) {}
        s.setBuiltInZoomControls(true);
        s.setDisplayZoomControls(false);
        s.setSupportZoom(true);
        s.setMediaPlaybackRequiresUserGesture(false);   // autoplay / inline video works
        s.setMixedContentMode(WebSettings.MIXED_CONTENT_ALWAYS_ALLOW);
        try { s.setPluginState(WebSettings.PluginState.ON); } catch (Exception ignored) {}
        s.setTextZoom(getScale());
        s.setJavaScriptCanOpenWindowsAutomatically(false);
        s.setSupportMultipleWindows(false);
        applyUserAgent();
        try {
            CookieManager cm = CookieManager.getInstance();
            cm.setAcceptCookie(true);
            cm.setAcceptThirdPartyCookies(webView, true); // players/logins need 3p cookies
        } catch (Exception ignored) {}
        webView.setBackgroundColor(0x00000000); // transparent WebView (v1.2 white-bg fix)
        webView.setVerticalScrollBarEnabled(true);
        webView.setScrollbarFadingEnabled(false);
        webView.setInitialScale(getScale());

        webView.setWebChromeClient(new WebChromeClient() {
            @Override public void onProgressChanged(WebView view, int progress) {
                pageProgress.setProgress(progress);
                pageProgress.setVisibility(progress >= 100 ? View.GONE : View.VISIBLE);
            }
            @Override public boolean onCreateWindow(WebView view, boolean isDialog, boolean isUserGesture, android.os.Message resultMsg) {
                return false; // popup blocking: never spawn new windows
            }
            @Override public void onReceivedTitle(WebView view, String title) {
                if (title != null && !title.isEmpty()) {
                    browserTitle.setText(title);
                    updateResume(view.getUrl(), title, false); // v1.4: title refresh, any category
                    if (rwActive) updateRwSession(view.getUrl(), title);
                    pushRemoteState(); // v1.19
                }
            }
            // HTML5 fullscreen video (the big one for players):
            @Override public void onShowCustomView(View view, CustomViewCallback callback) {
                if (fsView != null) { try { callback.onCustomViewHidden(); } catch (Exception ignored) {} return; }
                showFullscreen(view, callback);
            }
            @Override public void onHideCustomView() { hideFullscreen(); }
        });

        webView.setWebViewClient(new WebViewClient() {
            @Override public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest req) {
                return handleUrl(req.getUrl().toString());
            }
            @Override @SuppressWarnings("deprecation")
            public boolean shouldOverrideUrlLoading(WebView view, String url) { return handleUrl(url); }

            @Override public WebResourceResponse shouldInterceptRequest(WebView view, WebResourceRequest req) {
                if (AdBlock.isAd(req.getUrl().toString())) return AdBlock.empty();
                return super.shouldInterceptRequest(view, req);
            }
            @Override @SuppressWarnings("deprecation")
            public WebResourceResponse shouldInterceptRequest(WebView view, String url) {
                if (AdBlock.isAd(url)) return AdBlock.empty();
                return super.shouldInterceptRequest(view, url);
            }
            @Override public void onPageStarted(WebView view, String url, Bitmap favicon) {
                clearVideoData(); videoHintShown = false; // v1.15: fresh page, fresh poll data + hint
                inPageFsLogged = false; pageFsElement = ""; // v1.16: "inpage-fs" logs once per page
                pageProgress.setVisibility(View.VISIBLE);
                setUrlPillText(url);
                showHeader(true); // v1.4: every new page starts with the bar visible
                pushRemoteState(); // v1.19
            }
            @Override public void doUpdateVisitedHistory(WebView view, String url, boolean isReload) {
                super.doUpdateVisitedHistory(view, url, isReload);
                setUrlPillText(url);
                updateResume(url, view.getTitle(), true); // v1.4: SPA nav moves the resume target too
                // Fires for SPA pushState navigation too - this is how the
                // episode page gets recorded even when onPageFinished for
                // it never happens (v1.0 missed these entirely).
                if (rwActive) updateRwSession(url, view.getTitle());
                pushRemoteState(); // v1.19
            }
            @Override public void onPageFinished(WebView view, String url) {
                pageProgress.setVisibility(View.GONE);
                setUrlPillText(url);
                String title = view.getTitle();
                if (title != null) browserTitle.setText(title);
                updateResume(url, title, true); // v1.4: resume target = last finished page, any category
                if (rwActive) { updateRwSession(url, title); maybeCaptureShot(url); }
                maybeSeek();
                pushRemoteState(); // v1.19
            }
        });

        webView.setOnLongClickListener(v -> {
            WebView.HitTestResult r = webView.getHitTestResult();
            if (r != null && r.getExtra() != null
                    && (r.getType() == WebView.HitTestResult.SRC_ANCHOR_TYPE
                        || r.getType() == WebView.HitTestResult.SRC_IMAGE_ANCHOR_TYPE)) {
                lastMenuShownAt = System.currentTimeMillis();
                final String link = r.getExtra();
                AlertDialog d = new AlertDialog.Builder(this).setTitle(link)
                        .setItems(new CharSequence[]{"Open link", "Copy link", "\u2606 Favorite this site"},
                                (dlg, w) -> {
                                    if (w == 0) { if (!handleUrl(link)) webView.loadUrl(link); }
                                    else if (w == 1) {
                                        ClipboardManager cm = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
                                        if (cm != null) cm.setPrimaryClip(ClipData.newPlainText("url", link));
                                        Toast.makeText(this, "Link copied", Toast.LENGTH_SHORT).show();
                                    } else if (currentEntry != null) toggleFav(currentEntry);
                                }).create();
                trackDialog(d);
                d.show();
            }
            return true;
        });
    }

    /**
     * HTML5 fullscreen, v1.2: the custom view goes into fsContainer,
     * which is attached directly to the window's decorView (top level,
     * above the browser pane) at MATCH_PARENT x MATCH_PARENT, the
     * window gets FLAG_FULLSCREEN + FLAG_KEEP_SCREEN_ON, and the
     * system UI goes immersive. v1.1 nested the container inside the
     * browser layout and set no window flags, so the video stayed at
     * its in-page size.
     */
    private void showFullscreen(View view, WebChromeClient.CustomViewCallback callback) {
        fsView = view; fsCallback = callback;
        cancelAutoPairPopup(); // v1.28: no pending auto card, and a visible one hides, into fullscreen
        hideToolbarTip(); // v1.25: no toolbar tooltip in fullscreen either
        // v1.15: freeze the page scroll for the whole FS session -
        // snapshot BEFORE anything attaches or lays out, and clear
        // the inline video detection (the video bound no longer
        // applies; doScrollBy is a no-op while fsView != null).
        fsSavedScrollX = webView != null ? webView.getScrollX() : 0;
        fsSavedScrollY = webView != null ? webView.getScrollY() : 0;
        clearVideoData();
        showHeader(false); // v1.4: never carry a hidden header into/out of fullscreen

        // v1.8 (order fix): window state goes on BEFORE the custom
        // view is attached, so its first layout pass already happens
        // at the final window size. v1.4 attached first and set the
        // flags after, so the first pass ran at pre-immersive size
        // and had to be repaired later.
        View decor = getWindow().getDecorView();
        fsPrevUiVisibility = decor.getSystemUiVisibility();
        WindowManager.LayoutParams attrs = getWindow().getAttributes();
        fsPrevWindowFlags = attrs.flags;
        fsPrevOrientation = getRequestedOrientation();
        setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE); // locked while fullscreen
        attrs.flags |= WindowManager.LayoutParams.FLAG_FULLSCREEN
                | WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON;
        getWindow().setAttributes(attrs);
        decor.setSystemUiVisibility(
                View.SYSTEM_UI_FLAG_FULLSCREEN | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                | View.SYSTEM_UI_FLAG_LAYOUT_STABLE);

        // Neither the custom view nor its container may take focus:
        // keys stay at Activity.dispatchKeyEvent level (v1.1 let the
        // custom view steal them, killing D-pad/mouse control).
        view.setFocusable(false);
        view.setFocusableInTouchMode(false);
        view.setBackgroundColor(Color.BLACK);
        fsContainer.setFocusable(false);
        fsContainer.setFocusableInTouchMode(false);
        fsContainer.setBackgroundColor(Color.BLACK);
        // v1.4 off-center fix: Chromium hands the custom view over with
        // its in-page geometry (offsets/scales) still attached, and the
        // view could arrive already parented. Detach it, zero every
        // transform, and attach with FRESH MATCH_PARENT/CENTER params.
        if (view.getParent() instanceof ViewGroup)
            ((ViewGroup) view.getParent()).removeView(view);
        FrameLayout.LayoutParams flp = new FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT);
        flp.gravity = Gravity.CENTER;
        fsContainer.addView(view, flp);
        fsContainer.setVisibility(View.VISIBLE);
        browserCol.setVisibility(View.GONE);

        // v1.8: arm the continuous geometry guard (all triggers
        // funnel into the one idempotent enforcer).
        fsFixedSurface = null; fsFixedW = -1; fsFixedH = -1;
        fsLastGlobalCleanSummary = null; // v1.14: each session's first global summary always logs
        enforceFullscreenGeometry("attach");
        // v1.14 (diagnostic): FS ENTER checkpoint - always logged,
        // so a session with NO other HeckYeahFS lines at least
        // proves the guard armed. Bounds here are as-laid at attach
        // time (the "post"/"delay" passes show the settled ones).
        try {
            DisplayMetrics rdm = new DisplayMetrics();
            getWindowManager().getDefaultDisplay().getRealMetrics(rdm);
            View decorV = getWindow().getDecorView();
            Log.d("HeckYeahFS", "enter viewClass=" + view.getClass().getName()
                    + " container=" + fsBounds(fsContainer)
                    + " view=" + fsBounds(view)
                    + " real=" + rdm.widthPixels + "x" + rdm.heightPixels
                    + " window=" + decorV.getWidth() + "x" + decorV.getHeight()
                    + " scroll=" + fsSavedScrollX + "," + fsSavedScrollY
                    + " orientation=" + (getRequestedOrientation()
                            == ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE ? "landscape" : "other"));
        } catch (Exception ignored) {}
        // v1.15: probe the page's <video> rect at t=0 too, not
        // only at delay400/900 - if the player lays its element
        // out wrong from the very first frame, this catches the
        // fullscreen element's rect before anything can move it.
        fsJsProbe("enter");
        fsContainer.post(() -> enforceFullscreenGeometry("post"));
        try {
            ViewTreeObserver vto = fsContainer.getViewTreeObserver();
            if (vto.isAlive()) vto.addOnGlobalLayoutListener(fsGlobalLayoutListener);
        } catch (Exception ignored) {}
        view.addOnLayoutChangeListener(fsLayoutChangeListener);
        ui.postDelayed(fsDelay150, 150);
        ui.postDelayed(fsDelay400, 400);
        ui.postDelayed(fsDelay900, 900);

        // Mouse cursor above the video (it lives at decor level, last
        // child; bringToFront re-asserts that over fsContainer).
        if (cursorView != null) cursorView.bringToFront();
        refreshCursor("fs-show"); // v1.32 (was updateCursorVisibility)
        pokeFsCursor(); // start the 10s idle fade (mouse sub-mode only)
        pushRemoteState(); // v1.19
    }

    /**
     * v1.8: the continuous fullscreen geometry guard (replaces
     * v1.4's one-shot centering pass). Idempotent: asserts the full
     * geometry contract below and touches nothing when everything
     * already conforms, so it can be called from layout listeners
     * without looping. Triggers: "attach", "post", "global"
     * (container global-layout), "layout" (custom view layout
     * change), "focus" (window focus), "delay150/400/900" (slow
     * immersive transitions). v1.14 (diagnostic build): every pass
     * logs a HeckYeahFS summary line, clean or not, and actual
     * corrections additionally log their detail line with the
     * trigger and old -> new bounds - pair a shifted screenshot
     * with `adb logcat -s HeckYeahFS`.
     *
     * Contract: the container fills the decorView above the app
     * content with no margins/transforms; the custom view and
     * every ViewGroup / SurfaceView / TextureView descendant are
     * full-bleed MATCH_PARENT (gravity CENTER where the params
     * type supports it) with zero margins and reset transforms;
     * every SurfaceView's holder is fixed at the real display
     * size. The surface keeps the video's own aspect handling, so
     * black bars can only come from the video's aspect ratio.
     * v1.15 adds: the container's own params anchor with
     * Gravity.FILL (never CENTER), and the WebView's scroll
     * position is frozen at the values snapshotted in
     * showFullscreen - any drift is restored here and logged
     * as a "webscroll" correction.
     */
    private void enforceFullscreenGeometry(String trigger) {
        if (fsView == null || fsContainer == null || fsEnforcing) return;
        fsEnforcing = true;
        try {
            StringBuilder sb = new StringBuilder();
            boolean corrected = false;
            fsPassFixCount = 0; // v1.14: per-pass correction count for the summary line
            ViewGroup decor = (ViewGroup) getWindow().getDecorView();

            // Container placement: child of the decorView, above
            // the app content (the cursor overlay stays above it).
            if (fsContainer.getParent() != decor) {
                String before = "container parent=" + fsContainer.getParent();
                if (fsContainer.getParent() instanceof ViewGroup)
                    ((ViewGroup) fsContainer.getParent()).removeView(fsContainer);
                // v1.15: FILL gravity, so the container is anchored
                // to the decorView's top-left at full size - never
                // centered (a centered container with any size
                // mismatch drifts toward a corner).
                FrameLayout.LayoutParams fillLp = new FrameLayout.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT);
                fillLp.gravity = Gravity.FILL;
                decor.addView(fsContainer, fillLp);
                if (cursorView != null) cursorView.bringToFront();
                corrected = true; fsPassFixCount++;
                sb.append(before).append(" -> decorView full-bleed; ");
            } else {
                int ci = decor.indexOfChild(fsContainer);
                int wi = decor.indexOfChild(windowRoot);
                if (wi >= 0 && ci < wi) {
                    FrameLayout.LayoutParams fillLp2 = new FrameLayout.LayoutParams(
                            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT);
                    fillLp2.gravity = Gravity.FILL;
                    decor.removeView(fsContainer);
                    decor.addView(fsContainer, fillLp2);
                    if (cursorView != null) cursorView.bringToFront();
                    corrected = true; fsPassFixCount++;
                    sb.append("container z-order ").append(ci).append(" -> above content; ");
                }
            }
            // Container params + transforms.
            ViewGroup.LayoutParams clp = fsContainer.getLayoutParams();
            boolean cBad = !(clp instanceof FrameLayout.LayoutParams)
                    || clp.width != ViewGroup.LayoutParams.MATCH_PARENT
                    || clp.height != ViewGroup.LayoutParams.MATCH_PARENT
                    || ((FrameLayout.LayoutParams) clp).gravity != Gravity.FILL
                    || ((FrameLayout.LayoutParams) clp).leftMargin != 0
                    || ((FrameLayout.LayoutParams) clp).topMargin != 0
                    || ((FrameLayout.LayoutParams) clp).rightMargin != 0
                    || ((FrameLayout.LayoutParams) clp).bottomMargin != 0
                    || fsContainer.getX() != 0 || fsContainer.getY() != 0
                    || fsContainer.getTranslationX() != 0 || fsContainer.getTranslationY() != 0;
            if (cBad) {
                FrameLayout.LayoutParams fillLp3 = new FrameLayout.LayoutParams(
                        ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT);
                fillLp3.gravity = Gravity.FILL;
                fsContainer.setLayoutParams(fillLp3);
                fsContainer.setX(0); fsContainer.setY(0);
                fsContainer.setTranslationX(0); fsContainer.setTranslationY(0);
                corrected = true; fsPassFixCount++;
                sb.append("container params/transform reset; ");
            }

            // v1.15: page scroll freeze. The WebView under the
            // fullscreen video must sit exactly where it was when
            // fullscreen started. If anything drifted it (player
            // JS scrolling its own page is the usual suspect, and
            // a scrolled page is also what makes the video LOOK
            // shifted toward a corner), restore the snapshot and
            // count it as a correction so it shows in HeckYeahFS.
            if (webView != null
                    && (webView.getScrollX() != fsSavedScrollX
                        || webView.getScrollY() != fsSavedScrollY)) {
                sb.append("webscroll [").append(webView.getScrollX()).append(',')
                        .append(webView.getScrollY()).append("] -> [")
                        .append(fsSavedScrollX).append(',').append(fsSavedScrollY).append("]; ");
                webView.scrollTo(fsSavedScrollX, fsSavedScrollY);
                corrected = true; fsPassFixCount++;
            }

            DisplayMetrics dm = new DisplayMetrics();
            getWindowManager().getDefaultDisplay().getRealMetrics(dm);
            int cw = fsContainer.getWidth(), ch = fsContainer.getHeight();
            corrected |= enforceFsTree(fsView, cw, ch, dm.widthPixels, dm.heightPixels, sb);
            if (corrected) {
                fsContainer.requestLayout();
                Log.d("HeckYeahFS", "[" + trigger + "] " + sb.toString().trim());
            }
            // v1.14 (diagnostic): EVERY pass also logs a one-line
            // summary, clean or not - v1.8-v1.13 logged only actual
            // corrections, so a clean `adb logcat -s HeckYeahFS`
            // was ambiguous (guard happy vs guard never ran). The
            // "global" trigger fires on every layout pass and can
            // repeat the identical clean state indefinitely, so
            // only there are repeat clean summaries suppressed
            // (keyed on the previous global clean summary; any
            // correction, or any other trigger, resets the key).
            View sv = findFsSurface(fsView);
            String summary = "[" + trigger + "] check container=" + fsBounds(fsContainer)
                    + " view=" + fsBounds(fsView)
                    + " surface=" + (sv == null ? "none" : sv.getClass().getSimpleName())
                    + " holderFixed=" + (fsFixedSurface == null ? "none" : fsFixedW + "x" + fsFixedH)
                    + " fixed=" + fsPassFixCount;
            boolean suppress = !corrected && "global".equals(trigger)
                    && summary.equals(fsLastGlobalCleanSummary);
            if (!suppress) {
                Log.d("HeckYeahFS", summary);
                fsLastGlobalCleanSummary = (!corrected && "global".equals(trigger)) ? summary : null;
            }
        } catch (Exception ignored) {
            // The guard must never take the player down with it.
        } finally {
            fsEnforcing = false;
        }
    }

    /** Recursive full-bleed enforcement for the custom view tree. */
    private boolean enforceFsTree(View v, int targetW, int targetH, int realW, int realH, StringBuilder sb) {
        boolean corrected = false;
        boolean structural = v == fsView || v instanceof ViewGroup
                || v instanceof SurfaceView || v instanceof TextureView;
        if (structural) {
            String before = null;
            ViewGroup.LayoutParams lp = v.getLayoutParams();
            if (lp != null) {
                boolean bad = lp.width != ViewGroup.LayoutParams.MATCH_PARENT
                        || lp.height != ViewGroup.LayoutParams.MATCH_PARENT;
                if (lp instanceof ViewGroup.MarginLayoutParams) {
                    ViewGroup.MarginLayoutParams m = (ViewGroup.MarginLayoutParams) lp;
                    if (m.leftMargin != 0 || m.topMargin != 0 || m.rightMargin != 0 || m.bottomMargin != 0) bad = true;
                }
                if (lp instanceof FrameLayout.LayoutParams
                        && ((FrameLayout.LayoutParams) lp).gravity != Gravity.CENTER) bad = true;
                if (bad) {
                    before = fsDescribe(v);
                    v.setLayoutParams(fullBleedParams(lp));
                    corrected = true; fsPassFixCount++;
                }
            }
            if (v.getTranslationX() != 0 || v.getTranslationY() != 0 || v.getRotation() != 0
                    || v.getScaleX() != 1f || v.getScaleY() != 1f || v.getX() != 0 || v.getY() != 0
                    || v.getPaddingLeft() != 0 || v.getPaddingTop() != 0
                    || v.getPaddingRight() != 0 || v.getPaddingBottom() != 0) {
                if (before == null) before = fsDescribe(v);
                v.setX(0); v.setY(0);
                v.setTranslationX(0); v.setTranslationY(0);
                v.setRotation(0); v.setScaleX(1f); v.setScaleY(1f);
                v.setPadding(0, 0, 0, 0);
                corrected = true; fsPassFixCount++;
            }
            if (v instanceof SurfaceView
                    && (fsFixedSurface != v || fsFixedW != realW || fsFixedH != realH)) {
                if (before == null) before = fsDescribe(v);
                try { ((SurfaceView) v).getHolder().setFixedSize(realW, realH); }
                catch (Exception ignored) {}
                fsFixedSurface = v; fsFixedW = realW; fsFixedH = realH;
                corrected = true; fsPassFixCount++;
            }
            // Last resort: params can be right while a stale laid
            // rect survives (Chromium lays the surface out by hand
            // in some paths) - force the laid bounds directly.
            if (targetW > 0 && targetH > 0
                    && (v.getLeft() != 0 || v.getTop() != 0
                        || v.getRight() != targetW || v.getBottom() != targetH)) {
                if (before == null) before = fsDescribe(v);
                v.layout(0, 0, targetW, targetH);
                corrected = true; fsPassFixCount++;
            }
            if (corrected && before != null) {
                sb.append(before).append(" -> [0,0,").append(targetW).append(',')
                        .append(targetH).append("] full-bleed; ");
            }
        }
        if (v instanceof ViewGroup) {
            ViewGroup g = (ViewGroup) v;
            int pw = v.getWidth() > 0 ? v.getWidth() : targetW;
            int ph = v.getHeight() > 0 ? v.getHeight() : targetH;
            for (int i = 0; i < g.getChildCount(); i++)
                corrected |= enforceFsTree(g.getChildAt(i), pw, ph, realW, realH, sb);
        }
        return corrected;
    }

    /** Fresh MATCH_PARENT params of the same runtime type as the old ones. */
    private static ViewGroup.LayoutParams fullBleedParams(ViewGroup.LayoutParams old) {
        if (old instanceof FrameLayout.LayoutParams) {
            FrameLayout.LayoutParams p = new FrameLayout.LayoutParams(
                    ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT);
            p.gravity = Gravity.CENTER;
            return p;
        }
        if (old instanceof LinearLayout.LayoutParams)
            return new LinearLayout.LayoutParams(
                    ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT);
        if (old instanceof ViewGroup.MarginLayoutParams)
            return new ViewGroup.MarginLayoutParams(
                    ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT);
        return new ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT);
    }

    private static String fsDescribe(View v) {
        ViewGroup.LayoutParams lp = v.getLayoutParams();
        return v.getClass().getSimpleName() + " bounds=[" + v.getLeft() + "," + v.getTop()
                + "," + v.getRight() + "," + v.getBottom() + "] params="
                + (lp == null ? "null" : lp.width + "x" + lp.height);
    }

    /** v1.14 (diagnostic): "[l,t,r,b]" laid bounds of a view. */
    private static String fsBounds(View v) {
        if (v == null) return "null";
        return "[" + v.getLeft() + "," + v.getTop() + "," + v.getRight() + "," + v.getBottom() + "]";
    }

    /** v1.14 (diagnostic): first SurfaceView in the custom view tree, or null. */
    private static View findFsSurface(View v) {
        if (v == null) return null;
        if (v instanceof SurfaceView) return v;
        if (v instanceof ViewGroup) {
            ViewGroup g = (ViewGroup) v;
            for (int i = 0; i < g.getChildCount(); i++) {
                View r = findFsSurface(g.getChildAt(i));
                if (r != null) return r;
            }
        }
        return null;
    }

    // v1.14 (diagnostic): JS-side view of the page's video while in
    // fullscreen - the largest <video> element's CSS rect, its
    // intrinsic size, whether the page thinks something is in
    // document fullscreen, and the page's viewport size. Returned
    // as a plain string (evaluateJavascript hands it back quoted;
    // the caller strips the quotes). Any failure yields "".
    private static final String FS_PROBE_JS =
            "(function(){try{"
            + "var vs=document.querySelectorAll('video');var best=null,ba=-1;"
            + "for(var i=0;i<vs.length;i++){var r=vs[i].getBoundingClientRect();"
            + "var a=r.width*r.height;if(a>ba){ba=a;best=vs[i];}}"
            + "var fe=document.fullscreenElement;"
            + "var s='inner='+window.innerWidth+'x'+window.innerHeight"
            + "+' fs='+(fe?fe.tagName:'none');"
            + "if(!best){return s+' video=none';}"
            + "var r2=best.getBoundingClientRect();"
            + "return s+' rect=['+Math.round(r2.left)+','+Math.round(r2.top)+','"
            + "+Math.round(r2.right)+','+Math.round(r2.bottom)+']'"
            + "+' video='+best.videoWidth+'x'+best.videoHeight;"
            + "}catch(e){return '';}})()";

    /** v1.14 (diagnostic): run FS_PROBE_JS against the page; log-only, never throws. */
    private void fsJsProbe(final String checkpoint) {
        try {
            if (fsView == null || webView == null) return;
            webView.evaluateJavascript(FS_PROBE_JS, value -> {
                try {
                    if (fsView == null) return; // exited fullscreen before the result landed
                    String s = value;
                    if (s != null && s.length() >= 2 && s.startsWith("\"") && s.endsWith("\""))
                        s = s.substring(1, s.length() - 1);
                    if (s != null && !s.isEmpty() && !"null".equals(s))
                        Log.d("HeckYeahFS", "[jsprobe " + checkpoint + "] " + s);
                } catch (Exception ignored) {}
            });
        } catch (Exception ignored) {}
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        // The "sometimes shifted" cases race the immersive transition:
        // re-assert the geometry once the window settles with focus.
        if (hasFocus && fsView != null)
            fsContainer.post(() -> enforceFullscreenGeometry("focus"));
    }

    private void hideFullscreen() {
        if (fsView == null) return;
        cancelFsFade();
        showHeader(false); // v1.4: header restored for the browser underneath
        // v1.8: disarm the geometry guard BEFORE detaching anything -
        // listeners and scheduled re-asserts must not leak into the
        // next fullscreen session.
        try {
            ViewTreeObserver vto = fsContainer.getViewTreeObserver();
            if (vto.isAlive()) vto.removeOnGlobalLayoutListener(fsGlobalLayoutListener);
        } catch (Exception ignored) {}
        fsView.removeOnLayoutChangeListener(fsLayoutChangeListener);
        ui.removeCallbacks(fsDelay150);
        ui.removeCallbacks(fsDelay400);
        ui.removeCallbacks(fsDelay900);
        fsFixedSurface = null;
        // v1.14 (diagnostic): FS EXIT checkpoint - final laid
        // bounds, captured just before the view is detached.
        try {
            Log.d("HeckYeahFS", "exit container=" + fsBounds(fsContainer)
                    + " view=" + fsBounds(fsView));
        } catch (Exception ignored) {}
        fsContainer.removeView(fsView);
        fsContainer.setVisibility(View.GONE);
        browserCol.setVisibility(View.VISIBLE);
        // Restore the window flags + system UI saved in showFullscreen.
        try {
            WindowManager.LayoutParams attrs = getWindow().getAttributes();
            attrs.flags = fsPrevWindowFlags;
            getWindow().setAttributes(attrs);
        } catch (Exception ignored) {}
        getWindow().getDecorView().setSystemUiVisibility(fsPrevUiVisibility);
        setRequestedOrientation(fsPrevOrientation); // v1.8: undo the fullscreen landscape lock
        WebChromeClient.CustomViewCallback cb = fsCallback;
        fsView = null; fsCallback = null;
        refreshCursor("fs-hide"); // v1.32 (was updateCursorVisibility)
        if (cb != null) try { cb.onCustomViewHidden(); } catch (Exception ignored) {}
        pushRemoteState(); // v1.19
    }

    /**
     * v1.27: the pill shows ONLY the main URL - the page's host
     * ("atlantic.st"), not the full URL. The editor still opens
     * with the full URL (openUrlDialog reads webView.getUrl()
     * itself). Host-less URLs (about:, data:, file:) fall back to
     * the string itself; the pill's ellipsize trims it to fit.
     * Also the one funnel every navigation update passes through,
     * so Back/Forward icon dimming refreshes here too.
     */
    private void setUrlPillText(String url) {
        if (urlPill == null || url == null || url.isEmpty()) return;
        urlPill.setText(displayHost(url));
        refreshNavIcons();
    }

    private String displayHost(String url) {
        try {
            String h = Uri.parse(url).getHost();
            if (h != null && !h.isEmpty()) return h;
        } catch (Exception ignored) {}
        return url;
    }

    private boolean handleUrl(String url) {
        if (url == null) return false;
        String lower = url.toLowerCase(Locale.US);
        if (lower.startsWith("http://") || lower.startsWith("https://")
                || lower.startsWith("about:") || lower.startsWith("data:")) return false;
        try {
            Intent i = new Intent(Intent.ACTION_VIEW, Uri.parse(url));
            i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            startActivity(i);
        } catch (Exception e) {
            Toast.makeText(this, "Can't open link: " + url, Toast.LENGTH_SHORT).show();
        }
        return true;
    }

    /** The URL normalization shared by the dialog and the phone remote. */
    private String resolveUrlString(String input) {
        if (input == null || input.isEmpty()) return null;
        String u = input;
        try {
            if (!u.contains("://")) {
                if (u.contains(" ") || !u.contains("."))
                    u = "https://www.google.com/search?q=" + URLEncoder.encode(u, "UTF-8");
                else u = "https://" + u;
            }
        } catch (Exception ignored) {}
        return u;
    }

    private void loadResolved(String input) {
        String u = resolveUrlString(input);
        if (u != null) webView.loadUrl(u);
    }

    private void noteOrigin() {
        if (currentScreen == catScroll || currentScreen == shelfScroll) { originKind = "cat"; originCat = currentCat; }
        else if (currentScreen == searchPane) { originKind = "search"; originCat = null; }
        else if (currentScreen == homeScroll) { originKind = "home"; originCat = null; }
    }

    private void openEntry(Entry e) {
        noteOrigin();
        currentEntry = e;
        browserSuspended = false;
        showHeader(false); // v1.4: fresh open starts with the bar visible
        // v1.9: a NEW target invalidates the old preview shot - the
        // fresh shot is captured at this session's Done, and a
        // stale image must never pair with a new target meanwhile.
        if (!e.url.equals(resumeUrl)) deleteResumeShot();
        updateResume(e.url, e.name, true); // v1.4: opening replaces the resume target NOW, not at Done
        toolbarFocus = false; toolbarIndex = 2; styleToolbarFocus();
        // RW session starts only for video-category launches.
        rwActive = "video".equals(e.cat);
        rwSource = e.name;
        sessUrl = e.url; sessTitle = e.name; sessItem = null; pendingSeek = -1;
        if (rwActive) {
            sessItem = new RwItem(e.name, e.url, e.name, System.currentTimeMillis(), 0);
            // Insert immediately (applet page); updateRwSession upgrades
            // it to the exact content page as the user navigates deeper.
            rw.remove(sessItem);
            rw.add(0, sessItem);
            trimRw(); saveRw();
        }
        showScreen(browserPane);
        browserTitle.setText(e.name);
        setUrlPillText(e.url);
        try { webView.onResume(); webView.resumeTimers(); } catch (Exception ignored) {}
        webView.setInitialScale(getScale());
        webView.loadUrl(e.url);
    }

    private void openRw(RwItem item) {
        noteOrigin();
        currentEntry = new Entry("video", "Recently Watched", item.source, item.url, item.title, false, "");
        browserSuspended = false;
        toolbarFocus = false; toolbarIndex = 2; styleToolbarFocus();
        rwActive = true;
        rwSource = item.source;
        sessUrl = item.url; sessTitle = item.title; sessItem = item;
        pendingSeek = item.seek > 1 ? item.seek : -1;
        showScreen(browserPane);
        browserTitle.setText(item.title);
        setUrlPillText(item.url);
        try { webView.onResume(); webView.resumeTimers(); } catch (Exception ignored) {}
        webView.setInitialScale(getScale());
        webView.loadUrl(item.url); // stored exact URL, never the applet homepage
        if (!item.url.equals(resumeUrl)) deleteResumeShot(); // v1.9: see openEntry - never pair a stale shot with a new target
        updateResume(item.url, item.title, false); // v1.4: resume target = the exact stored page
        resumeSeek = item.seek;
    }

    private void resumeBrowser() {
        loadResume(); // v1.4: act on the CURRENT persisted target
        if (resumeUrl == null || resumeUrl.isEmpty()) return;
        final String target = resumeUrl;
        // v1.4: reuse the live session ONLY when the WebView is actually
        // on the button's URL; otherwise load the target fresh.
        boolean liveSession = browserSuspended && target.equals(webView.getUrl());
        toolbarFocus = false; styleToolbarFocus();
        showHeader(false);
        showScreen(browserPane);
        try { webView.onResume(); webView.resumeTimers(); } catch (Exception ignored) {}
        if (liveSession) {
            // Same WebView, page kept (media stayed paused); just re-show it.
            browserTitle.setText(resumeTitle);
            setUrlPillText(webView.getUrl());
            rwActive = "video".equals(resumeCat);
            rwSource = resumeApplet;
            if (rwActive) {
                sessItem = findRwByUrl(resumeUrl);
                sessUrl = resumeUrl; sessTitle = resumeTitle;
            }
        } else {
            // After an app restart: reload the saved page fresh.
            rwActive = "video".equals(resumeCat);
            rwSource = resumeApplet;
            sessUrl = resumeUrl; sessTitle = resumeTitle;
            sessItem = rwActive ? findRwByUrl(resumeUrl) : null;
            if (sessItem == null && rwActive) {
                sessItem = new RwItem(resumeTitle, resumeUrl, resumeApplet, System.currentTimeMillis(), resumeSeek);
                rw.add(0, sessItem); trimRw(); saveRw();
            }
            pendingSeek = resumeSeek > 1 ? resumeSeek : -1;
            browserTitle.setText(resumeTitle);
            setUrlPillText(resumeUrl);
            webView.setInitialScale(getScale());
            webView.loadUrl(resumeUrl);
        }
        browserSuspended = false;
    }

    /**
     * Done: snapshot the current page (RW + resume state), pause ALL
     * media, suspend timers, and leave the browser WITHOUT destroying
     * the WebView or blanking the page, so Back-to can resume it.
     */
    private void closeBrowser() {
        cancelAutoPairPopup(); // v1.27: dismiss the auto popup + cancel its countdown
        hideToolbarTip(); // v1.25
        cancelActionFeedback(); // v1.26: no feedback flash outlives the browser
        autoPairShownThisSession = false; // v1.27: next browser launch may auto-pop again
        clearVideoData(); // v1.15: poll loop self-stops once another screen shows
        cancelFsFade(); // v1.17: a pseudo-FS exit via Done never reaches hideFullscreen (fsView is null) - never carry a faded cursor or a live fade timer out of the browser
        showHeader(false); // v1.4: restore the bar even if auto-hidden
        if (fsView != null) hideFullscreen();
        // v1.9: capture the resume hover-preview NOW, in the same
        // moment the finalize below saves the new resume target,
        // while the browser is still on screen (finalize hides the
        // pane, and an invisible WebView may draw blank). Resolved
        // exactly like finalize resolves the target: live URL,
        // else last tracked navigation. No page = no capture; any
        // previous shot still pairs with the unchanged target.
        if (webView != null) {
            String cu = webView.getUrl();
            if (cu == null || cu.isEmpty()) cu = lastNavUrl;
            if (cu != null && cu.startsWith("http")) captureResumeShot();
        }
        final boolean[] finalized = {false};
        final Runnable finalize = () -> {
            if (finalized[0]) return;
            finalized[0] = true;
            // Save "page you just left" for the Back-to bar: the LIVE
            // WebView URL is authoritative (v1.4; the old sessUrl
            // fallback could re-save the session's start page forever).
            // Fallback chain: live URL -> last tracked navigation.
            String u = webView.getUrl();
            if (u == null || u.isEmpty()) u = lastNavUrl;
            if (u != null && u.startsWith("http")) {
                String t = webView.getTitle();
                updateResume(u, (t != null && !t.isEmpty()) ? t : sessTitle, false);
            }
            browserSuspended = true;
            try { webView.onPause(); } catch (Exception ignored) {}
            try { webView.pauseTimers(); } catch (Exception ignored) {}
            rwActive = false;
            // Land back where the user came from (bar shows on top there).
            if ("cat".equals(originKind) && originCat != null) showCategory(originCat, null);
            else if ("search".equals(originKind)) showSearch();
            else showHome();
            pushRemoteState(); // v1.19: phones see the browser is gone
        };
        // RW snapshot + playback position first (async JS), then finalize.
        if (rwActive) {
            snapshotRwFromWebView();
            try {
                webView.evaluateJavascript(
                    "(function(){try{var v=document.querySelector('video');"
                    + "return v?String(v.currentTime):'';}catch(e){return '';}})()",
                    new ValueCallback<String>() {
                        @Override public void onReceiveValue(String value) {
                            if (sessItem != null) {
                                double t = parseJsSeconds(value);
                                if (t > 0) sessItem.seek = t;
                                resumeSeek = sessItem.seek;
                                saveRw();
                            }
                            pauseAllMedia();
                            finalize.run();
                        }
                    });
            } catch (Exception e) {
                pauseAllMedia();
                finalize.run();
                return;
            }
            // If the page is hung and JS never answers, close anyway.
            ui.postDelayed(finalize, 800);
        } else {
            pauseAllMedia();
            finalize.run();
        }
    }

    private void pauseAllMedia() {
        try {
            webView.evaluateJavascript(
                "(function(){try{document.querySelectorAll('video,audio')"
                + ".forEach(function(m){try{m.pause();}catch(e){}});}catch(e){}})()", null);
        } catch (Exception ignored) {}
    }

    private static double parseJsSeconds(String value) {
        if (value == null) return -1;
        String v = value.replace("\"", "").trim();
        if (v.isEmpty() || "null".equals(v)) return -1;
        try { return Double.parseDouble(v); } catch (Exception e) { return -1; }
    }

    private void maybeSeek() {
        if (pendingSeek <= 0 || webView.getUrl() == null) return;
        final double target = pendingSeek;
        pendingSeek = -1;
        final String js = "(function(){try{var v=document.querySelector('video');"
                + "if(v){v.currentTime=" + target + ";return 'ok';}return 'novideo';"
                + "}catch(e){return 'err';}})()";
        try { webView.evaluateJavascript(js, null); } catch (Exception ignored) {}
        // Players often attach the <video> a moment after load: one retry.
        ui.postDelayed(() -> {
            try { webView.evaluateJavascript(js, null); } catch (Exception ignored) {}
        }, 3000);
    }

    // ---------------------------------------------------------- RW: Recently Watched

    private static boolean isRootUrl(String url) {
        try {
            URL u = new URL(url);
            String path = u.getPath();
            boolean noPath = path == null || path.isEmpty() || "/".equals(path);
            boolean noQuery = u.getQuery() == null || u.getQuery().isEmpty();
            return noPath && noQuery;
        } catch (Exception e) { return false; }
    }
    private static String hostOf(String url) {
        try { String h = new URL(url).getHost(); return h == null ? url : h; }
        catch (Exception e) { return url; }
    }

    /**
     * Maintains the session's single RW entry. The deepest/latest
     * content URL wins: a site-root URL never overwrites a deeper
     * entry for the same host in the same session.
     */
    private void updateRwSession(String url, String title) {
        if (url == null || !url.startsWith("http")) return;
        if (isRootUrl(url) && sessUrl != null && !sessUrl.isEmpty()
                && !isRootUrl(sessUrl) && hostOf(url).equals(hostOf(sessUrl))) {
            return; // root redirect/reload must not clobber the episode page
        }
        if (sessItem != null && sessItem.url != null && !sessItem.url.equals(url)
                && sessItem.shot != null && sessItem.shot.equals(shotNameFor(sessItem.url))) {
            deleteShotFile(sessItem); shotCache.remove(sessItem.shot); sessItem.shot = "";
        }
        // v1.7: enrichment is keyed by URL hash - if the session is
        // moving to a different page, the old page's poster/scores
        // must not follow it (the new URL gets its own lookup).
        if (sessItem != null && sessItem.url != null && !sessItem.url.equals(url)) {
            deleteEnrichmentForUrl(sessItem.url);
        }
        sessUrl = url;
        if (title != null && !title.trim().isEmpty() && !title.equals(url)) sessTitle = title.trim();
        if (sessItem == null) {
            sessItem = new RwItem(sessTitle, sessUrl, rwSource, System.currentTimeMillis(), 0);
            rw.add(0, sessItem);
        } else {
            sessItem.url = sessUrl;
            if (sessTitle != null && !sessTitle.isEmpty()) sessItem.title = sessTitle;
            sessItem.ts = System.currentTimeMillis();
            rw.remove(sessItem);
            rw.add(0, sessItem);
        }
        trimRw();
        saveRw();
    }

    /** Final snapshot at Done time, straight from the live WebView. */
    private void snapshotRwFromWebView() {
        String u = webView.getUrl();
        String t = webView.getTitle();
        if (u != null && u.startsWith("http")) {
            if (!(isRootUrl(u) && sessUrl != null && !isRootUrl(sessUrl)
                    && hostOf(u).equals(hostOf(sessUrl)))) {
                sessUrl = u;
            }
            if (t != null && !t.trim().isEmpty() && !t.equals(u)) sessTitle = t.trim();
            if (sessItem != null) {
                // v1.7: same URL-keyed invalidation as updateRwSession.
                if (sessItem.url != null && !sessItem.url.equals(sessUrl)) {
                    deleteEnrichmentForUrl(sessItem.url);
                }
                sessItem.url = sessUrl;
                if (sessTitle != null && !sessTitle.isEmpty()) sessItem.title = sessTitle;
                sessItem.ts = System.currentTimeMillis();
                rw.remove(sessItem); rw.add(0, sessItem);
                trimRw();
            }
            saveRw();
            if (sessItem != null) {
                captureRwShot(sessItem); // final screenshot at Done
                // v1.7 trigger (i): entry just created/updated at
                // Done - kick its poster/ratings lookup now (the
                // URL and title are final at this point).
                kickEnrichment(sessItem, false);
            }
        }
    }

    /** Debounced screenshot capture (onPageFinished can fire a lot). */
    private void maybeCaptureShot(String url) {
        long now = System.currentTimeMillis();
        if (now - lastShotAt < 4000 && url != null && url.equals(lastShotUrl)) return;
        lastShotAt = now; lastShotUrl = url;
        if (sessItem != null) captureRwShot(sessItem);
    }

    /**
     * v1.3: capture the WebView's current viewport (downscaled to max
     * 480 wide, JPEG ~70) as the RW entry's screenshot. v1.10: uses
     * the shared strict viewport capture (native-size draw, shrink
     * afterwards) - see captureViewportBitmap(). Honest limit:
     * if the last screen was fullscreen video, Android does not
     * composite protected/hardware video surfaces into a view draw,
     * so the capture may come out black.
     */
    private void captureRwShot(RwItem item) {
        if (item == null || webView == null) return;
        try {
            Bitmap bm = captureViewportBitmap();
            if (bm == null) return;
            bm = downscaleToWidth(bm, 480);
            File dir = rwShotsDir();
            if (!dir.exists()) dir.mkdirs();
            File f = new File(dir, shotNameFor(item.url));
            FileOutputStream fos = new FileOutputStream(f);
            bm.compress(Bitmap.CompressFormat.JPEG, 70, fos);
            fos.flush(); fos.close();
            bm.recycle();
            if (!f.getName().equals(item.shot)) {
                if (item.shot != null && !item.shot.isEmpty()) shotCache.remove(item.shot);
                item.shot = f.getName();
                saveRw();
            }
        } catch (Exception ignored) {}
    }

    private RwItem findRwByUrl(String url) {
        for (RwItem i : rw) if (i.url.equals(url)) return i;
        return null;
    }
    private void trimRw() {
        while (rw.size() > RW_CAP) {
            RwItem evicted = rw.remove(rw.size() - 1);
            deleteShotFile(evicted);
            deleteEnrichment(evicted); // v1.7: poster file + sidecar
            if (evicted.shot != null) shotCache.remove(evicted.shot);
        }
    }

    private void loadRw() {
        rw.clear();
        String raw = prefs.getString(KEY_RW, "");
        if (raw == null || raw.isEmpty()) return;
        for (String line : raw.split("\n")) {
            String[] p = line.split(SEP, -1);
            if (p.length >= 4) {
                try {
                    double seek = p.length >= 5 ? Double.parseDouble(p[4]) : 0;
                    RwItem it = new RwItem(p[0], p[1], p[2], Long.parseLong(p[3]), seek);
                    if (p.length >= 6) it.shot = p[5]; // v1.3 trailing field; older records load fine without it
                    rw.add(it);
                } catch (Exception ignored) {}
            }
        }
    }
    private void saveRw() {
        StringBuilder sb = new StringBuilder();
        for (RwItem i : rw) {
            if (sb.length() > 0) sb.append('\n');
            sb.append(cleanField(i.title)).append(SEP)
              .append(i.url).append(SEP).append(cleanField(i.source)).append(SEP)
              .append(i.ts).append(SEP).append(i.seek)
              .append(SEP).append(i.shot == null ? "" : cleanField(i.shot));
        }
        prefs.edit().putString(KEY_RW, sb.toString()).apply();
    }

    // ---------------------------------------------------------- Catalog: load / parse / D live refresh

    private File cacheFile() { return new File(getFilesDir(), "catalog_cache.txt"); }

    private void loadCatalog() {
        catalog.clear();
        List<Entry> loaded = null;
        File f = cacheFile();
        if (f.isFile()) loaded = parsePipe(readAll(f));
        if (loaded == null || loaded.isEmpty()) {
            try { loaded = parsePipe(readAll(getAssets().open("catalog.txt"))); }
            catch (Exception e) { loaded = new ArrayList<>(); }
        }
        catalog.addAll(loaded);
    }

    private static String readAll(File f) {
        try { return readAll(new FileInputStream(f)); } catch (Exception e) { return ""; }
    }
    private static String readAll(InputStream in) {
        try {
            StringBuilder sb = new StringBuilder();
            BufferedReader br = new BufferedReader(new InputStreamReader(in, "UTF-8"));
            String line; while ((line = br.readLine()) != null) sb.append(line).append('\n');
            br.close(); return sb.toString();
        } catch (Exception e) { return ""; }
    }

    static List<Entry> parsePipe(String raw) {
        List<Entry> out = new ArrayList<>();
        if (raw == null) return out;
        for (String line : raw.split("\n")) {
            line = line.trim();
            if (line.isEmpty() || line.startsWith("#")) continue;
            String[] p = line.split("\\|", -1);
            if (p.length >= 6) out.add(new Entry(p[0].trim(), p[1].trim(), p[2].trim(), p[3].trim(),
                    p[4].trim(), "1".equals(p[5].trim()), p.length > 6 ? p[6].trim() : ""));
        }
        return out;
    }

    static String clean(String s) {
        return s.replace("|", "-").replace("\n", " ").replace("\r", " ").trim();
    }

    private static final Pattern LINK = Pattern.compile("\\[([^\\]]+)\\]\\((https?://[^)\\s]+)\\)");

    /** Parses one FMHY markdown page. ## = group, ### = shelf; bullet's first link is the tile. */
    static List<Entry> parseMarkdown(String catId, String md) {
        List<Entry> out = new ArrayList<>();
        String major = "", shelf = "";
        for (String rawLine : md.split("\n")) {
            String line = rawLine.trim();
            if (line.startsWith("### ")) { shelf = stripMd(line.substring(4)); continue; }
            if (line.startsWith("## ")) { major = stripMd(line.substring(3)); if (shelf.isEmpty()) shelf = major; else shelf = major; continue; }
            if (!(line.startsWith("- ") || line.startsWith("* "))) continue;
            Matcher m = LINK.matcher(line);
            if (!m.find()) continue;
            String name = stripMd(m.group(1));
            String url = m.group(2);
            if (name.isEmpty() || name.matches("\\d+") || "2".equals(name) || "3".equals(name)) continue;
            List<String> mirrors = new ArrayList<>();
            while (m.find()) {
                String u2 = m.group(2);
                if (!u2.equals(url) && !mirrors.contains(u2)) mirrors.add(u2);
            }
            String desc = LINK.matcher(line).replaceAll("$1");
            desc = desc.replaceFirst("^[\\-\\*]\\s*", "");
            int idx = desc.indexOf(name);
            if (idx >= 0) desc = desc.substring(idx + name.length());
            desc = stripMd(desc).replaceFirst("^[\\-:/\\s]+", "");
            if (desc.length() > 220) desc = desc.substring(0, 220);
            boolean rec = rawLine.contains("**");
            String sh = shelf.isEmpty() ? (major.isEmpty() ? "General" : major) : shelf;
            StringBuilder mb = new StringBuilder();
            for (String mm : mirrors) { if (mb.length() > 0) mb.append(','); mb.append(mm); }
            out.add(new Entry(catId, clean(sh), clean(name), url, clean(desc), rec, mb.toString()));
        }
        return out;
    }

    private static String stripMd(String s) {
        return s.replace("**", "").replace("`", "").replace("[", "").replace("]", "").trim();
    }

    private void startRefresh() {
        Toast.makeText(this, "Updating FMHY data from GitHub...", Toast.LENGTH_SHORT).show();
        settingsStatus.setText("Updating... fetching 12 category files");
        new Thread(() -> {
            List<Entry> all = new ArrayList<>();
            int ok = 0;
            for (String[] c : CATS) {
                try {
                    String url = "https://raw.githubusercontent.com/fmhy/edit/main/docs/" + c[3];
                    HttpURLConnection conn = (HttpURLConnection) new URL(url).openConnection();
                    conn.setConnectTimeout(20000); conn.setReadTimeout(30000);
                    conn.setRequestProperty("User-Agent", "HeckYeahTV/1.1");
                    if (conn.getResponseCode() == 200) {
                        List<Entry> part = parseMarkdown(c[0], readAll(conn.getInputStream()));
                        if (!part.isEmpty()) { all.addAll(part); ok++; }
                    }
                    conn.disconnect();
                } catch (Exception ignored) {}
            }
            final int okF = ok, total = all.size();
            if (ok >= 6 && total > catalog.size() / 2 && total > 100) {
                StringBuilder sb = new StringBuilder();
                for (Entry e : all) sb.append(e.cat).append('|').append(e.shelf).append('|').append(e.name)
                        .append('|').append(e.url).append('|').append(e.desc).append('|')
                        .append(e.rec ? "1" : "0").append('|').append(e.mirrors).append('\n');
                try {
                    FileOutputStream fos = new FileOutputStream(cacheFile());
                    fos.write(sb.toString().getBytes("UTF-8")); fos.close();
                } catch (Exception ignored) {}
                ui.post(() -> {
                    loadCatalog();
                    prefs.edit().putLong(KEY_UPDATED, System.currentTimeMillis()).apply();
                    refreshSettingsLabels(); buildHome();
                    Toast.makeText(this, "Updated: " + total + " sites from " + okF + "/12 categories", Toast.LENGTH_LONG).show();
                });
            } else {
                ui.post(() -> {
                    refreshSettingsLabels();
                    Toast.makeText(this, "Update failed (" + okF + "/12 fetched) - keeping current catalog", Toast.LENGTH_LONG).show();
                });
            }
        }).start();
    }

    // ---------------------------------------------------------- Browser control modes + key interception

    /** Mouse sub-mode lives ONLY inside the browser now. */
    private void setBrowserMouse(boolean on, boolean announce) {
        browserMouse = on;
        prefs.edit().putBoolean(KEY_MOUSE, on).apply();
        if (tbMode != null) updateModeBtn(); // v1.25: icon, not text
        if (on) {
            cursorX = getResources().getDisplayMetrics().widthPixels / 2f;
            cursorY = getResources().getDisplayMetrics().heightPixels / 2f;
            if (announce) Toast.makeText(this, "Browser mouse: D-pad moves, tap = click, HOLD center = long-press", Toast.LENGTH_LONG).show();
        } else {
            cancelFsFade();
            toolbarFocus = false; styleToolbarFocus();
            if (announce) Toast.makeText(this, "Browser D-pad: arrows scroll, \u25B2 at page top opens the toolbar, OK taps the page / activates a button", Toast.LENGTH_LONG).show();
        }
        refreshCursor("mode"); // v1.32: self-healing assert on BOTH toggle directions (Kurt's dead-cursor path)
        refreshSettingsLabels();
    }

    private static boolean isBrowserKey(int code) {
        switch (code) {
            case KeyEvent.KEYCODE_DPAD_UP: case KeyEvent.KEYCODE_DPAD_DOWN:
            case KeyEvent.KEYCODE_DPAD_LEFT: case KeyEvent.KEYCODE_DPAD_RIGHT:
            case KeyEvent.KEYCODE_DPAD_CENTER: case KeyEvent.KEYCODE_ENTER:
            case KeyEvent.KEYCODE_NUMPAD_ENTER:
            case KeyEvent.KEYCODE_PAGE_UP: case KeyEvent.KEYCODE_PAGE_DOWN:
            case KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE: case KeyEvent.KEYCODE_MEDIA_PLAY:
            case KeyEvent.KEYCODE_MEDIA_PAUSE:
            case KeyEvent.KEYCODE_MEDIA_FAST_FORWARD: case KeyEvent.KEYCODE_MEDIA_REWIND:
            case KeyEvent.KEYCODE_MEDIA_NEXT: case KeyEvent.KEYCODE_MEDIA_PREVIOUS:
                return true;
            default: return false;
        }
    }

    @Override
    public boolean dispatchKeyEvent(KeyEvent e) {
        int code = e.getKeyCode();

        if (code == KeyEvent.KEYCODE_MENU) {
            if (e.getAction() == KeyEvent.ACTION_DOWN && e.getRepeatCount() == 0
                    && currentScreen == browserPane && browserDialogDepth == 0) {
                // v1.4: MENU opens the Browser Menu (nav-bar replica);
                // the mouse/D-pad toggle lives inside it (+ toolbar/gear).
                // In fullscreen MENU keeps its old mode-toggle role.
                if (fsView == null) openBrowserNavMenu();
                else setBrowserMouse(!browserMouse, true);
            }
            return true; // MENU never falls through
        }

        // v1.28: while the auto pairing CARD is visible (browser,
        // not fullscreen, no dialog), it owns two keys at the
        // activity level - the card itself is non-focusable, so
        // this interception is the only way it can be driven with
        // the remote. BACK dismisses the card and is consumed (it
        // must not ALSO go back in the page via onBackPressed,
        // which is where an unconsumed BACK lands, BACK not being
        // a browser key). In D-PAD sub-mode, CENTER/OK performs
        // the card action (fired on UP, like the other CENTER
        // handlers); in mouse sub-mode the CENTER key goes through
        // dispatchTap's coordinate claim instead.
        if (pairCardVisible && currentScreen == browserPane && fsView == null
                && browserDialogDepth == 0) {
            if (code == KeyEvent.KEYCODE_BACK) {
                if (e.getAction() == KeyEvent.ACTION_DOWN && e.getRepeatCount() == 0)
                    hidePairCard();
                return true;
            }
            if (!browserMouse && (code == KeyEvent.KEYCODE_DPAD_CENTER
                    || code == KeyEvent.KEYCODE_ENTER
                    || code == KeyEvent.KEYCODE_NUMPAD_ENTER)) {
                if (e.getAction() == KeyEvent.ACTION_UP) pairCardAction();
                return true;
            }
        }

        // v1.25 (Part D): our own injected play/pause fallback key
        // must NOT be re-swallowed by the browser branch below -
        // handleBrowserKey would call togglePlayPause again, whose JS
        // would say "none" again, forever. mediaKeyPlayPause arms
        // the flag; the key's DOWN clears it and the pair falls
        // through to the WebView/Chromium media handling. Real
        // fullscreen is exempt (fsView != null): there the injected
        // key keeps the FS center-tap behavior in
        // handleFullscreenKey.
        if (code == KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE && mediaKeyPassThrough
                && fsView == null && currentScreen == browserPane
                && browserDialogDepth == 0) {
            if (e.getAction() == KeyEvent.ACTION_DOWN) mediaKeyPassThrough = false;
            return super.dispatchKeyEvent(e);
        }

        // FULLSCREEN FIRST (v1.2): player controls must work over the
        // video, and keys must never be forwarded into the WebView or
        // the custom view. (Browser dialogs cannot be open while
        // fullscreen; the depth check is just belt-and-braces.) In
        // v1.1 this case was handled inside handleBrowserKey AFTER the
        // browser checks and consisted of "return" - the cursor never
        // moved and the D-pad did nothing.
        if (fsView != null && browserDialogDepth == 0 && isBrowserKey(code)) {
            handleFullscreenKey(e, code);
            return true;
        }

        // BROWSER: swallow every D-pad / media key so the web page (and its
        // video player) never sees them - Left/Right would seek, Up/Down
        // would change volume, etc. Dialogs (URL editor, gear, link menu)
        // are exempt: they need native key handling.
        if (currentScreen == browserPane && browserDialogDepth == 0 && isBrowserKey(code)) {
            handleBrowserKey(e, code);
            return true;
        }
        return super.dispatchKeyEvent(e);
    }

    private void handleBrowserKey(KeyEvent e, int code) {
        boolean down = e.getAction() == KeyEvent.ACTION_DOWN;
        boolean up = e.getAction() == KeyEvent.ACTION_UP;
        boolean center = code == KeyEvent.KEYCODE_DPAD_CENTER || code == KeyEvent.KEYCODE_ENTER
                || code == KeyEvent.KEYCODE_NUMPAD_ENTER;

        // Fullscreen is handled first in dispatchKeyEvent (v1.2); this
        // is only a safety net if some other path lands here.
        if (fsView != null) { handleFullscreenKey(e, code); return; }

        if (browserMouse) {
            if (center) {
                if (down) {
                    if (e.getRepeatCount() == 0) centerDownAt = SystemClock.uptimeMillis();
                } else if (up) {
                    long held = SystemClock.uptimeMillis() - centerDownAt;
                    if (held >= 500) {
                        // Long-press: a hold ON a link keeps the link menu
                        // (WebView long-click listener stamps
                        // lastMenuShownAt); a hold anywhere else falls
                        // back to the v1.4 Browser Menu once the long-tap
                        // has had time to resolve.
                        final long mark = lastMenuShownAt;
                        dispatchLongTap(cursorX, cursorY);
                        ui.postDelayed(() -> {
                            if (lastMenuShownAt == mark && browserDialogDepth == 0
                                    && fsView == null && currentScreen == browserPane)
                                openBrowserNavMenu();
                        }, 950);
                    } else {
                        dispatchTap(cursorX, cursorY);
                        if (fsLikeActive()) pokeFsCursor(); // v1.17: a tap is interaction - restore + re-arm the fade (covers the play toggle in pseudo-FS)
                    }
                }
                return;
            }
            if (down) {
                int step = prefs.getInt(KEY_CURSOR_STEP, 52);
                if (e.isShiftPressed()) step = 12;
                switch (code) {
                    case KeyEvent.KEYCODE_DPAD_UP: moveCursor(0, -step); break;
                    case KeyEvent.KEYCODE_DPAD_DOWN: moveCursor(0, step); break;
                    case KeyEvent.KEYCODE_DPAD_LEFT: moveCursor(-step, 0); break;
                    case KeyEvent.KEYCODE_DPAD_RIGHT: moveCursor(step, 0); break;
                    case KeyEvent.KEYCODE_PAGE_UP: case KeyEvent.KEYCODE_MEDIA_REWIND: doScrollBy(0, -pageScrollStep()); break;
                    case KeyEvent.KEYCODE_PAGE_DOWN: case KeyEvent.KEYCODE_MEDIA_FAST_FORWARD: doScrollBy(0, pageScrollStep()); break;
                    case KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE:
                        if (e.getRepeatCount() == 0) togglePlayPause("key");
                        break;
                    default: break;
                }
            }
            return;
        }

        // ---- Browser D-pad sub-mode ----
        // v1.31: play/pause is a page action, not a toolbar action:
        // route it to the shared toggle even while toolbar focus is
        // up (previously the toolbar branch swallowed the key).
        if (code == KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE) {
            if (down && e.getRepeatCount() == 0) togglePlayPause("key");
            return;
        }
        if (toolbarFocus) {
            if (down && e.getRepeatCount() == 0) {
                switch (code) {
                    case KeyEvent.KEYCODE_DPAD_LEFT:
                        toolbarIndex = (toolbarIndex + toolbarViews.size() - 1) % toolbarViews.size();
                        styleToolbarFocus(); break;
                    case KeyEvent.KEYCODE_DPAD_RIGHT:
                        toolbarIndex = (toolbarIndex + 1) % toolbarViews.size();
                        styleToolbarFocus(); break;
                    case KeyEvent.KEYCODE_DPAD_DOWN:
                        toolbarFocus = false; styleToolbarFocus(); break;
                    default: break;
                }
            }
            if (up && center) activateToolbarItem();
            return;
        }

        // Page focus in D-pad mode: arrows scroll, never reach the page.
        // v1.4 CENTER hold detection: arm a 500ms timer on key-down; if
        // it fires, the nav menu opens and the key-up tap is suppressed
        // via dpadCenterHoldFired. A short press never arms long enough
        // to fire, so taps are untouched.
        if (center && down && e.getRepeatCount() == 0) {
            dpadCenterDown = true; dpadCenterHoldFired = false;
            ui.removeCallbacks(dpadHoldRunnable);
            ui.postDelayed(dpadHoldRunnable, 500);
        }
        if (down) {
            switch (code) {
                case KeyEvent.KEYCODE_DPAD_UP:
                    if (headerHidden) { showHeader(true); toolbarFocus = true; styleToolbarFocus(); }
                    else if (webView.getScrollY() <= 0) { toolbarFocus = true; styleToolbarFocus(); }
                    else doScrollBy(0, -dp(300)); // v1.15: bounded scroll area
                    break;
                case KeyEvent.KEYCODE_DPAD_DOWN: doScrollBy(0, dp(300)); break;
                case KeyEvent.KEYCODE_DPAD_LEFT: doScrollBy(-dp(160), 0); break;
                case KeyEvent.KEYCODE_DPAD_RIGHT: doScrollBy(dp(160), 0); break;
                case KeyEvent.KEYCODE_PAGE_UP: case KeyEvent.KEYCODE_MEDIA_REWIND: doScrollBy(0, -pageScrollStep()); break;
                case KeyEvent.KEYCODE_PAGE_DOWN: case KeyEvent.KEYCODE_MEDIA_FAST_FORWARD: doScrollBy(0, pageScrollStep()); break;
                case KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE:
                    if (e.getRepeatCount() == 0) togglePlayPause("key");
                    break;
                default: break;
            }
        }
        if (up && center) {
            dpadCenterDown = false;
            ui.removeCallbacks(dpadHoldRunnable);
            if (dpadCenterHoldFired) { dpadCenterHoldFired = false; return; } // hold already opened the menu
            // Tap the page where the cursor last was (or page center).
            int[] loc = new int[2]; webView.getLocationInWindow(loc);
            float tx = cursorX, ty = cursorY;
            if (tx < loc[0] || tx > loc[0] + webView.getWidth()
                    || ty < loc[1] || ty > loc[1] + webView.getHeight()) {
                tx = loc[0] + webView.getWidth() / 2f;
                ty = loc[1] + webView.getHeight() / 2f;
            }
            dispatchTap(tx, ty);
        }
    }

    /**
     * Keys while HTML5 fullscreen video is showing (v1.2). Everything
     * reaching here is consumed by dispatchKeyEvent and NEVER
     * dispatched into the WebView/custom view, so Left/Right seeking
     * etc. stays impossible. Simulated taps go straight to
     * fsContainer so the player shows its controls / toggles play,
     * exactly as if the screen had been touched. BACK is not a
     * browser key: it falls through to onBackPressed, which exits
     * fullscreen.
     */
    private void handleFullscreenKey(KeyEvent e, int code) {
        pokeFsCursor(); // any handled key = interaction: cursor back + 10s timer restarts
        boolean down = e.getAction() == KeyEvent.ACTION_DOWN;
        boolean up = e.getAction() == KeyEvent.ACTION_UP;
        boolean center = code == KeyEvent.KEYCODE_DPAD_CENTER || code == KeyEvent.KEYCODE_ENTER
                || code == KeyEvent.KEYCODE_NUMPAD_ENTER;

        if (browserMouse) {
            if (center) {
                if (down) {
                    if (e.getRepeatCount() == 0) centerDownAt = SystemClock.uptimeMillis();
                } else if (up) {
                    dispatchFsTap(cursorX, cursorY); // tap at cursor: controls / play toggle
                }
                return;
            }
            if (down) {
                int step = prefs.getInt(KEY_CURSOR_STEP, 52);
                if (e.isShiftPressed()) step = 12;
                switch (code) {
                    case KeyEvent.KEYCODE_DPAD_UP: moveCursor(0, -step); break;
                    case KeyEvent.KEYCODE_DPAD_DOWN: moveCursor(0, step); break;
                    case KeyEvent.KEYCODE_DPAD_LEFT: moveCursor(-step, 0); break;
                    case KeyEvent.KEYCODE_DPAD_RIGHT: moveCursor(step, 0); break;
                    case KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE:
                        if (e.getRepeatCount() == 0) dispatchFsTap(fsCenterX(), fsCenterY());
                        break;
                    default: break; // PAGE_UP/DOWN, FF/REW, NEXT/PREV: consumed silently, no seeking
                }
            }
            return;
        }

        // D-pad sub-mode in fullscreen: CENTER / PLAY_PAUSE taps screen
        // center (toggle play / show controls); any arrow also just
        // taps to surface the player's controls - it is never
        // forwarded to the page.
        if (down && e.getRepeatCount() == 0) {
            switch (code) {
                case KeyEvent.KEYCODE_DPAD_UP: case KeyEvent.KEYCODE_DPAD_DOWN:
                case KeyEvent.KEYCODE_DPAD_LEFT: case KeyEvent.KEYCODE_DPAD_RIGHT:
                case KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE:
                    dispatchFsTap(fsCenterX(), fsCenterY());
                    break;
                default:
                    if (center) dispatchFsTap(fsCenterX(), fsCenterY());
                    break;
            }
        }
    }

    private float fsCenterX() {
        return fsContainer != null && fsContainer.getWidth() > 0
                ? fsContainer.getWidth() / 2f
                : getResources().getDisplayMetrics().widthPixels / 2f;
    }

    private float fsCenterY() {
        return fsContainer != null && fsContainer.getHeight() > 0
                ? fsContainer.getHeight() / 2f
                : getResources().getDisplayMetrics().heightPixels / 2f;
    }

    // -------------------------------------------------- v1.23 input injection
    //
    // Browser taps used to be MotionEvents dispatched straight to
    // a view in OUR window. That can never reach a WebView native
    // popup: tapping an HTML <select> opens a framework list in
    // its OWN window on top of the page, and view dispatch lands
    // on the page underneath it. So taps (and phone-remote keys,
    // see injectKey) now go through the system input queue via
    // Instrumentation: the queue delivers to the TOP window at the
    // coordinates, exactly like a real finger - the select popup
    // when it is open, the page otherwise. The popup window is
    // created by the WebView in our own process/UID, so injection
    // is permitted; if Fire OS still refuses (any exception), the
    // gesture falls back to the previous view-dispatch behavior
    // and the failure is logged under HeckYeahFS.
    //
    // sendPointerSync/sendKeySync BLOCK until the event is
    // delivered, so every injected gesture runs on one dedicated
    // background thread, which also serializes all gestures in
    // submission order. Nothing here ever runs on the UI thread
    // beyond enqueueing.

    private ExecutorService injectExec;   // single-thread, created lazily
    private Instrumentation injectIns;    // only ever touched on that thread
    private boolean injectOkLogged;       // one success line per session

    private ExecutorService injectExecutor() {
        if (injectExec == null) injectExec = Executors.newSingleThreadExecutor();
        return injectExec;
    }

    private void logInjectOk() {
        if (!injectOkLogged) {
            injectOkLogged = true;
            Log.d("HeckYeahFS", "inject mode=instrumentation");
        }
    }

    /**
     * Inject one pointer gesture in DISPLAY coordinates (sx, sy):
     * DOWN, an optional hold, UP, as a touchscreen source. On any
     * failure, fallbackAll runs on the UI thread if the DOWN never
     * landed, or fallbackUp if it did (so a half-delivered gesture
     * is completed, never duplicated).
     */
    private void injectPointer(final float sx, final float sy, final long holdMs,
                               final String what, final Runnable fallbackAll,
                               final Runnable fallbackUp) {
        injectExecutor().execute(() -> {
            boolean downSent = false;
            try {
                if (injectIns == null) injectIns = new Instrumentation();
                long downTime = SystemClock.uptimeMillis();
                MotionEvent d = MotionEvent.obtain(downTime, downTime,
                        MotionEvent.ACTION_DOWN, sx, sy, 0);
                d.setSource(InputDevice.SOURCE_TOUCHSCREEN);
                try { injectIns.sendPointerSync(d); } finally { d.recycle(); }
                downSent = true;
                if (holdMs > 0) {
                    try { Thread.sleep(holdMs); }
                    catch (InterruptedException ie) { Thread.currentThread().interrupt(); }
                }
                long upTime = SystemClock.uptimeMillis();
                MotionEvent u = MotionEvent.obtain(downTime, upTime,
                        MotionEvent.ACTION_UP, sx, sy, 0);
                u.setSource(InputDevice.SOURCE_TOUCHSCREEN);
                try { injectIns.sendPointerSync(u); } finally { u.recycle(); }
                logInjectOk();
            } catch (Exception e) {
                Log.d("HeckYeahFS", "inject fallback " + what + ": "
                        + e.getClass().getSimpleName());
                ui.post(downSent ? fallbackUp : fallbackAll);
            }
        });
    }

    /** Pre-v1.23 behavior, kept as the injection fallback. */
    private void viewDispatchTap(View target, float x, float y) {
        if (target == null) return;
        long now = SystemClock.uptimeMillis();
        MotionEvent down = MotionEvent.obtain(now, now, MotionEvent.ACTION_DOWN, x, y, 0);
        MotionEvent up = MotionEvent.obtain(now, now + 50, MotionEvent.ACTION_UP, x, y, 0);
        target.dispatchTouchEvent(down);
        target.dispatchTouchEvent(up);
        down.recycle(); up.recycle();
    }

    /** Completes a gesture whose injected DOWN landed but whose UP failed. */
    private void viewDispatchUp(View target, float x, float y) {
        if (target == null) return;
        long now = SystemClock.uptimeMillis();
        MotionEvent up = MotionEvent.obtain(now, now, MotionEvent.ACTION_UP, x, y, 0);
        target.dispatchTouchEvent(up);
        up.recycle();
    }

    /** The pre-v1.23 dispatchLongTap body, kept as its fallback. */
    private void viewDispatchLongTap(float x, float y) {
        long now = SystemClock.uptimeMillis();
        MotionEvent down = MotionEvent.obtain(now, now, MotionEvent.ACTION_DOWN, x, y, 0);
        windowRoot.dispatchTouchEvent(down); down.recycle();
        ui.postDelayed(() -> {
            long upNow = SystemClock.uptimeMillis();
            MotionEvent up = MotionEvent.obtain(upNow, upNow, MotionEvent.ACTION_UP, x, y, 0);
            windowRoot.dispatchTouchEvent(up); up.recycle();
        }, 650);
    }

    /**
     * Tap on the fullscreen container at container-local (x, y).
     * Instrumentation needs display coordinates: the container is
     * fullscreen, but convert through its on-screen location
     * rather than assuming it.
     */
    private void dispatchFsTap(float x, float y) {
        hideToolbarTip(); // v1.25: a click dismisses any name bubble
        if (fsContainer == null) return;
        int[] loc = new int[2];
        fsContainer.getLocationOnScreen(loc);
        injectPointer(x + loc[0], y + loc[1], 40, "fstap",
                () -> viewDispatchTap(fsContainer, x, y),
                () -> viewDispatchUp(fsContainer, x, y));
    }

    private void activateToolbarItem() {
        hideToolbarTip(); // v1.25: activation is a click - bubble goes away
        View v = toolbarViews.get(toolbarIndex);
        if (v == addrCapsule) { openUrlDialog(); return; }
        if (v instanceof Button) ((Button) v).performClick();
    }

    // ------------------------------------------------ v1.31: play/pause

    /**
     * One-shot pass-through for OUR OWN injected fallback key:
     * armed by mediaKeyPlayPause(), cleared by the first injected
     * MEDIA_PLAY_PAUSE reaching dispatchKeyEvent (which then lets
     * it fall through to the WebView instead of re-entering the
     * toggle - that would loop: JS "none" -> inject -> toggle ->
     * JS "none" ...). A 2s sweeper clears a flag whose key never
     * arrived (injection failure), so it can't eat a later
     * genuine press.
     */
    private boolean mediaKeyPassThrough = false;

    /** v1.31: one timestamp guards the shared toggle against a
     * phone press and a hardware press (or a duplicate delivery)
     * landing inside the same ~300ms and toggling twice. */
    private long lastPlayPauseAt = 0;

    /**
     * L1 toggle JS. Strict ES5 only (var/function/try-catch; no
     * arrow functions, let/const, template literals, optional
     * chaining or Promise-dependent flow): the v1.25 audit found
     * no ES6 syntax in the old script either, but every script in
     * this layered path is kept ES5 so the Stick's old Chromium
     * cannot silently kill it. Recursive scan of the top document
     * and same-origin iframes (same approach as VIDEO_POLL_JS).
     * Among videos with a visible rect of meaningful size, pause
     * the LARGEST playing one ("js-paused"); else play the
     * largest paused one ("js-played"); no reachable/controllable
     * video -> a structured "none:..." result that also reports
     * how many video elements were reachable and how many iframes
     * were cross-origin (unreachable).
     */
    private static final String TOGGLE_PLAY_JS =
            "(function(){try{"
            + "var IW=window.innerWidth,IH=window.innerHeight;"
            + "var bestPlay=null,playArea=0,bestPause=null,pauseArea=0,seen=0,xFrames=0;"
            + "function scan(doc,ox,oy){"
            + "var vs;try{vs=doc.querySelectorAll('video');}catch(e){return;}"
            + "for(var i=0;i<vs.length;i++){var v=vs[i];"
            + "try{"
            + "var r=v.getBoundingClientRect();"
            + "if(r.width<160||r.height<90)continue;"
            + "var L=r.left+ox,T=r.top+oy,R=r.right+ox,B=r.bottom+oy;"
            + "var ix=Math.max(0,Math.min(R,IW)-Math.max(L,0));"
            + "var iy=Math.max(0,Math.min(B,IH)-Math.max(T,0));"
            + "if(ix*iy<=0)continue;"
            + "seen++;"
            + "var area=r.width*r.height;"
            + "var playing=!v.paused&&!v.ended&&(v.readyState>=2||v.currentTime>0);"
            + "if(playing){if(area>playArea){playArea=area;bestPlay=v;}}"
            + "else if(v.paused&&!v.ended){if(area>pauseArea){pauseArea=area;bestPause=v;}}"
            + "}catch(e){}}"
            + "var fs;try{fs=doc.querySelectorAll('iframe');}catch(e){return;}"
            + "for(var j=0;j<fs.length;j++){var f=fs[j],cd=null;"
            + "try{cd=f.contentDocument;}catch(e){}"
            + "if(!cd){xFrames++;continue;}"
            + "var fr;try{fr=f.getBoundingClientRect();}catch(e){continue;}"
            + "scan(cd,ox+fr.left,oy+fr.top);}"
            + "}"
            + "scan(document,0,0);"
            + "if(bestPlay){try{bestPlay.pause();return 'js-paused';}catch(e){return 'none:pause-threw,seen='+seen+',xframes='+xFrames;}}"
            + "if(bestPause){try{var pr=bestPause.play();if(pr&&pr.catch){try{pr.catch(function(){});}catch(e){}}return 'js-played';}catch(e){return 'none:play-threw,seen='+seen+',xframes='+xFrames;}}"
            + "return 'none:seen='+seen+',xframes='+xFrames;"
            + "}catch(e){return 'none:error';}})()";

    /**
     * L2 click JS (strict ES5): the same reachable scan as L1,
     * but instead of calling play()/pause() it dispatches a click
     * on the largest playing (else largest paused) video element.
     * Some players ignore direct method calls from page JS and
     * only honor their own click handler. Returns
     * "click-paused"/"click-played" (the state the click was
     * aimed at) or the same structured "none:..." as L1.
     */
    private static final String CLICK_PLAY_JS =
            "(function(){try{"
            + "var IW=window.innerWidth,IH=window.innerHeight;"
            + "var bestPlay=null,playArea=0,bestPause=null,pauseArea=0,seen=0,xFrames=0;"
            + "function scan(doc,ox,oy){"
            + "var vs;try{vs=doc.querySelectorAll('video');}catch(e){return;}"
            + "for(var i=0;i<vs.length;i++){var v=vs[i];"
            + "try{"
            + "var r=v.getBoundingClientRect();"
            + "if(r.width<160||r.height<90)continue;"
            + "var L=r.left+ox,T=r.top+oy,R=r.right+ox,B=r.bottom+oy;"
            + "var ix=Math.max(0,Math.min(R,IW)-Math.max(L,0));"
            + "var iy=Math.max(0,Math.min(B,IH)-Math.max(T,0));"
            + "if(ix*iy<=0)continue;"
            + "seen++;"
            + "var area=r.width*r.height;"
            + "var playing=!v.paused&&!v.ended&&(v.readyState>=2||v.currentTime>0);"
            + "if(playing){if(area>playArea){playArea=area;bestPlay=v;}}"
            + "else if(v.paused&&!v.ended){if(area>pauseArea){pauseArea=area;bestPause=v;}}"
            + "}catch(e){}}"
            + "var fs;try{fs=doc.querySelectorAll('iframe');}catch(e){return;}"
            + "for(var j=0;j<fs.length;j++){var f=fs[j],cd=null;"
            + "try{cd=f.contentDocument;}catch(e){}"
            + "if(!cd){xFrames++;continue;}"
            + "var fr;try{fr=f.getBoundingClientRect();}catch(e){continue;}"
            + "scan(cd,ox+fr.left,oy+fr.top);}"
            + "}"
            + "scan(document,0,0);"
            + "var target=bestPlay?bestPlay:bestPause;"
            + "if(!target)return 'none:seen='+seen+',xframes='+xFrames;"
            + "try{target.click();}catch(e){return 'none:click-threw,seen='+seen+',xframes='+xFrames;}"
            + "return bestPlay?'click-paused':'click-played';"
            + "}catch(e){return 'none:error';}})()";

    /**
     * Post-action state probe (strict ES5): the largest visible,
     * non-ended video reachable from the top document / same-
     * origin iframes, reported as "playing", "paused" or "none".
     * Unlike VIDEO_POLL_JS (playing videos only), this can name
     * the paused state, so the ~450ms sample after a toggle can
     * log - and flash - the state that actually resulted.
     * Cross-origin players stay "none" here by design.
     */
    private static final String PLAY_STATE_JS =
            "(function(){try{"
            + "var IW=window.innerWidth,IH=window.innerHeight;"
            + "var best=null,bestArea=0,bestPlaying=false;"
            + "function scan(doc,ox,oy){"
            + "var vs;try{vs=doc.querySelectorAll('video');}catch(e){return;}"
            + "for(var i=0;i<vs.length;i++){var v=vs[i];"
            + "try{"
            + "if(v.ended)continue;"
            + "var r=v.getBoundingClientRect();"
            + "if(r.width<160||r.height<90)continue;"
            + "var L=r.left+ox,T=r.top+oy,R=r.right+ox,B=r.bottom+oy;"
            + "var ix=Math.max(0,Math.min(R,IW)-Math.max(L,0));"
            + "var iy=Math.max(0,Math.min(B,IH)-Math.max(T,0));"
            + "if(ix*iy<=0)continue;"
            + "var area=r.width*r.height;"
            + "if(area>bestArea){bestArea=area;best=v;bestPlaying=!v.paused;}"
            + "}catch(e){}}"
            + "var fs;try{fs=doc.querySelectorAll('iframe');}catch(e){return;}"
            + "for(var j=0;j<fs.length;j++){var f=fs[j],cd=null;"
            + "try{cd=f.contentDocument;}catch(e){}"
            + "if(!cd)continue;"
            + "var fr;try{fr=f.getBoundingClientRect();}catch(e){continue;}"
            + "scan(cd,ox+fr.left,oy+fr.top);}"
            + "}"
            + "scan(document,0,0);"
            + "if(!best)return 'none';"
            + "return bestPlaying?'playing':'paused';"
            + "}catch(e){return 'none';}})()";

    /**
     * v1.31: THE play/pause implementation for every entry point
     * - the physical remote's MEDIA_PLAY_PAUSE (browser mouse and
     * D-pad sub-modes; callers filter key repeats) and the phone
     * remote's playpause command. One layered path:
     *
     *   L1  ES5 JS toggle (play()/pause() on the largest video).
     *   L2  JS click on that video (players that only honor
     *       their click handler).
     *   L3  a REAL injected tap at the video's center - a genuine
     *       touch reaches even a cross-origin player's own
     *       handlers, exactly like tapping with the cursor. Only
     *       attempted with a known target: the fresh poll rect,
     *       or the WebView center while fullscreen-like. Never
     *       after L1/L2 landed (that would toggle straight back).
     *   L4  the media-key injection fallback.
     *
     * Real fullscreen (fsView != null) is the one exception: it
     * keeps its existing behavior - the L4 key is injected and
     * lands in handleFullscreenKey as the FS center tap.
     * Every layer logs its result verbatim under HeckYeahFS, and
     * exactly one final "playpause via=..." line names the layer
     * that carried the press.
     */
    private void togglePlayPause(final String source) {
        final String src = "phone".equals(source) ? "phone" : "key";
        Log.d("HeckYeahFS", "playpause src=" + src);
        final long actionAt = SystemClock.uptimeMillis();
        if (actionAt - lastPlayPauseAt < 300) {
            Log.d("HeckYeahFS", "playpause src=" + src + " result=ignored-guard");
            return;
        }
        lastPlayPauseAt = actionAt;
        if (webView == null || currentScreen != browserPane || browserDialogDepth != 0) {
            Log.d("HeckYeahFS", "playpause L1 js result=skip:not-browser");
            Log.d("HeckYeahFS", "playpause via=none");
            return;
        }
        if (fsView != null) { // real fullscreen: existing tap behavior via the key
            mediaKeyPlayPause(src);
            return;
        }
        try {
            webView.evaluateJavascript(TOGGLE_PLAY_JS, value -> {
                if (currentScreen != browserPane || fsView != null) {
                    Log.d("HeckYeahFS", "playpause via=none");
                    return;
                }
                String s = unquoteJs(value);
                if (s == null || s.isEmpty()) s = "none:empty";
                Log.d("HeckYeahFS", "playpause L1 js result=" + s);
                if ("js-paused".equals(s) || "js-played".equals(s)) {
                    Log.d("HeckYeahFS", "playpause via=js");
                    // v1.26: flash the RESULTING state.
                    showActionFeedback(fbBitmap("js-paused".equals(s) ? "pause" : "play"));
                    schedulePlayPausePostSample(src, "js", true);
                } else {
                    runPlayPauseClickLayer(src);
                }
            });
        } catch (Exception e) {
            Log.d("HeckYeahFS", "playpause L1 js result=exception:"
                    + e.getClass().getSimpleName());
            runPlayPauseClickLayer(src);
        }
    }

    /** L2: JS-dispatched click on the largest reachable video. */
    private void runPlayPauseClickLayer(final String src) {
        if (webView == null || currentScreen != browserPane || fsView != null) {
            Log.d("HeckYeahFS", "playpause via=none");
            return;
        }
        try {
            webView.evaluateJavascript(CLICK_PLAY_JS, value -> {
                if (currentScreen != browserPane || fsView != null) {
                    Log.d("HeckYeahFS", "playpause via=none");
                    return;
                }
                String s = unquoteJs(value);
                if (s == null || s.isEmpty()) s = "none:empty";
                Log.d("HeckYeahFS", "playpause L2 click result=" + s);
                if ("click-paused".equals(s) || "click-played".equals(s)) {
                    Log.d("HeckYeahFS", "playpause via=click");
                    schedulePlayPausePostSample(src, "click", false);
                } else if (!runPlayPauseTapLayer(src)) {
                    mediaKeyPlayPause(src);
                }
            });
        } catch (Exception e) {
            Log.d("HeckYeahFS", "playpause L2 click result=exception:"
                    + e.getClass().getSimpleName());
            if (!runPlayPauseTapLayer(src)) mediaKeyPlayPause(src);
        }
    }

    /**
     * L3: a genuine tap through the existing Instrumentation path
     * (dispatchTap). Returns false - and taps NOTHING - when no
     * target is known: no fresh poll rect and not fullscreen-like.
     */
    private boolean runPlayPauseTapLayer(final String src) {
        float[] pt = playPauseTapPoint();
        if (pt == null) {
            Log.d("HeckYeahFS", "playpause L3 tap result=skip:no-target");
            return false;
        }
        int x = Math.round(pt[0]), y = Math.round(pt[1]);
        Log.d("HeckYeahFS", "playpause L3 tap result=tap x=" + x + " y=" + y);
        dispatchTap(pt[0], pt[1]);
        if (fsLikeActive()) pokeFsCursor(); // the tap is interaction, like a cursor tap
        Log.d("HeckYeahFS", "playpause via=tap x=" + x + " y=" + y);
        schedulePlayPausePostSample(src, "tap", false);
        return true;
    }

    /**
     * The L3 target, in WINDOW coordinates (dispatchTap's space).
     * Fresh poll data (<2s) wins: the poll's viewport rect is CSS
     * px in the top page, so its center converts with
     * webView.getScale() - the same conversion the scroll-lock
     * math uses - plus the WebView's window offset. With no rect
     * but a fullscreen-like state (pseudo-FS), the WebView center
     * is the video by definition. Otherwise null: never tap
     * blindly.
     */
    private float[] playPauseTapPoint() {
        if (webView == null) return null;
        int[] loc = new int[2];
        webView.getLocationInWindow(loc);
        boolean fresh = videoDataAt > 0
                && System.currentTimeMillis() - videoDataAt < 2000;
        if (fresh && videoFound && vidViewR > vidViewL && vidViewB > vidViewT) {
            float scale = webView.getScale();
            if (scale <= 0f) scale = 1f;
            float cx = (vidViewL + vidViewR) / 2f * scale;
            float cy = (vidViewT + vidViewB) / 2f * scale;
            float x = loc[0] + clampF(cx, 0f, webView.getWidth());
            float y = loc[1] + clampF(cy, 0f, webView.getHeight());
            return new float[]{x, y};
        }
        if (fsLikeActive()) {
            return new float[]{loc[0] + webView.getWidth() / 2f,
                    loc[1] + webView.getHeight() / 2f};
        }
        return null;
    }

    /**
     * L4 (and the real-fullscreen route): inject MEDIA_PLAY_PAUSE
     * through the v1.23 instrumentation path. dispatchKeyEvent is
     * armed to let this one key fall through to the WebView (see
     * mediaKeyPassThrough), where Chromium's media-session
     * handling may toggle the active media element. In real
     * fullscreen the key instead lands in handleFullscreenKey,
     * whose center tap is the FS toggle, so the same injection
     * serves both states.
     */
    private void mediaKeyPlayPause(final String src) {
        Log.d("HeckYeahFS", "playpause L4 mediakey result=inject src=" + src);
        mediaKeyPassThrough = true;
        ui.postDelayed(() -> mediaKeyPassThrough = false, 2000);
        injectKey(KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE);
        Log.d("HeckYeahFS", "playpause via=mediakey");
        schedulePlayPausePostSample(src, "mediakey", false);
    }

    /**
     * v1.31: ~450ms after a toggle lands, probe the resulting
     * state once and log it ("playpause post src=... via=...
     * state=playing|paused|none"). For the click/tap/mediakey
     * layers this sample is also the feedback source (v1.26's
     * rule, kept): flash play/pause only when the probe provably
     * names the new state - cross-origin players answer "none"
     * and stay silent rather than flashing a guess. The JS layer
     * already flashed its known result, so it only logs. In real
     * fullscreen the probe is not run (the FS view owns the
     * video; v1.26 stayed silent there too).
     */
    private void schedulePlayPausePostSample(final String src, final String via,
                                             final boolean iconShown) {
        ui.postDelayed(() -> {
            if (currentScreen != browserPane || webView == null) {
                Log.d("HeckYeahFS", "playpause post src=" + src + " via=" + via
                        + " state=unknown:screen");
                return;
            }
            if (fsView != null) {
                Log.d("HeckYeahFS", "playpause post src=" + src + " via=" + via
                        + " state=unknown:real-fullscreen");
                return;
            }
            try {
                webView.evaluateJavascript(PLAY_STATE_JS, value -> {
                    String s = unquoteJs(value);
                    if (s == null || s.isEmpty()) s = "none";
                    Log.d("HeckYeahFS", "playpause post src=" + src + " via=" + via
                            + " state=" + s);
                    if (iconShown) return;
                    if ("playing".equals(s)) showActionFeedback(fbBitmap("play"));
                    else if ("paused".equals(s)) showActionFeedback(fbBitmap("pause"));
                });
            } catch (Exception e) {
                Log.d("HeckYeahFS", "playpause post src=" + src + " via=" + via
                        + " state=unknown:exception");
            }
        }, 450);
    }

    private void moveCursor(float dx, float dy) {
        // v1.32 watchdog: movement is the one input that ALWAYS
        // flows while the cursor should exist (D-pad keys and the
        // phone's remoteMove both funnel through here). If the
        // cursor should be visible but isn't - GONE, or alpha 0
        // outside a legitimate fsLike fade - heal it on the spot
        // instead of trusting that some transition restored it.
        if (cursorView != null && currentScreen == browserPane && browserMouse
                && (cursorView.getVisibility() != View.VISIBLE
                    || (!(fsLikeActive() && cursorFaded)
                        && cursorView.getAlpha() < 0.99f)))
            refreshCursor("watchdog-move");
        // v1.17: any cursor movement is interaction for the
        // fullscreen cursor fade. Outside fullscreen-like state
        // this is a no-op by construction: cursorFaded is false so
        // no alpha is touched, and pokeFsCursor schedules nothing
        // while fsLikeActive() is false.
        pokeFsCursor();
        // Edge-push scrolling over the browser page (Fetch v1.1 pattern).
        boolean onBrowser = currentScreen == browserPane && fsView == null;
        int pageTop = -1;
        boolean transitScrolledUp = false;
        if (onBrowser) {
            int[] loc = new int[2]; webView.getLocationInWindow(loc);
            int wr = loc[0] + webView.getWidth(), wb = loc[1] + webView.getHeight();
            pageTop = loc[1];
            if (cursorX >= loc[0] && cursorX <= wr && cursorY >= loc[1] && cursorY <= wb) {
                if (dy > 0 && cursorY + dy > wb) doScrollBy(0, dp(140));
                if (dy < 0 && cursorY + dy < loc[1]) {
                    // v1.15: the transit counts as scrolled only if
                    // the (possibly clamped) scroll actually moved
                    // the page - otherwise the top-edge rule below
                    // still gets its turn on this same move.
                    int beforeY = webView.getScrollY();
                    doScrollBy(0, -dp(140));
                    transitScrolledUp = webView.getScrollY() != beforeY;
                }
                if (dx > 0 && cursorX + dx > wr) doScrollBy(dp(80), 0);
                if (dx < 0 && cursorX + dx < loc[0]) doScrollBy(-dp(80), 0);
            }
        }
        int maxX = windowRoot.getWidth() > 0 ? windowRoot.getWidth() : getResources().getDisplayMetrics().widthPixels;
        int maxY = windowRoot.getHeight() > 0 ? windowRoot.getHeight() : getResources().getDisplayMetrics().heightPixels;
        cursorX = Math.max(0, Math.min(maxX, cursorX + dx));
        cursorY = Math.max(0, Math.min(maxY, cursorY + dy));
        if (dy < 0 && onBrowser) {
            // v1.4: pushing the cursor against the TOP edge of the screen
            // while the header is auto-hidden brings the bar back.
            if (cursorY <= 0 && headerHidden) showHeader(true);
            // v1.11: the reveal must not eat the gesture. Once the header
            // shows, the WebView's top edge drops to below the cursor
            // (the cursor stays parked at screen y=0, over the bar), so
            // the in-page edge-push check above — which requires the
            // cursor to be INSIDE the WebView's window rect — can never
            // fire again while UP is held, and the page stops scrolling.
            // Keep scrolling from the top edge itself: cursor pinned to
            // the screen top, or parked above the page's top edge, while
            // pushing up = scroll up. This is keyed to the cursor, not
            // to page geometry, so the header's show/hide layout change
            // cannot disarm it; the transit check's own scroll on the
            // crossing move is not doubled.
            if (!transitScrolledUp
                    && (cursorY <= 0 || (pageTop >= 0 && cursorY < pageTop)))
                doScrollBy(0, -dp(140)); // v1.15: the bounded scroll area applies here too
        }
        cursorView.invalidate();
        updateToolbarHover(); // v1.25: name bubble follows the cursor over the toolbar
    }

    /**
     * Tap at window coordinates (x, y) - the cursor's own space.
     * Callers pass cursor or WebView-window coordinates; the
     * window is fullscreen, so display coordinates are these plus
     * the root view's on-screen offset (converted explicitly).
     */
    private void dispatchTap(float x, float y) {
        hideToolbarTip(); // v1.25: a click dismisses any name bubble
        // v1.28: the pairing CARD claims taps before the page sees
        // them - inside its rect runs the card action and NO page
        // tap is injected; a tap anywhere else dismisses the card
        // and then proceeds normally (not consumed).
        if (pairCardVisible && currentScreen == browserPane && fsView == null) {
            if (pairCardContains(x, y)) { pairCardAction(); return; }
            hidePairCard();
        }
        float sx = x, sy = y;
        if (windowRoot != null) {
            int[] loc = new int[2];
            windowRoot.getLocationOnScreen(loc);
            sx += loc[0]; sy += loc[1];
        }
        injectPointer(sx, sy, 40, "tap",
                () -> viewDispatchTap(windowRoot, x, y),
                () -> viewDispatchUp(windowRoot, x, y));
    }

    /**
     * Long-press at window coordinates: a genuine injected hold
     * (DOWN, ~550ms on the injection thread, UP), so the WebView
     * sees a real long press and its link menu flow works as with
     * a finger - and a native popup item can be long-pressed too.
     */
    private void dispatchLongTap(final float x, final float y) {
        hideToolbarTip(); // v1.25: a click dismisses any name bubble
        // v1.28: same pairing-card claim as dispatchTap - a long
        // press on the card is the card action, never a page
        // long-press under it.
        if (pairCardVisible && currentScreen == browserPane && fsView == null) {
            if (pairCardContains(x, y)) { pairCardAction(); return; }
            hidePairCard();
        }
        float sx = x, sy = y;
        if (windowRoot != null) {
            int[] loc = new int[2];
            windowRoot.getLocationOnScreen(loc);
            sx += loc[0]; sy += loc[1];
        }
        injectPointer(sx, sy, 550, "longtap",
                () -> viewDispatchLongTap(x, y),
                () -> viewDispatchUp(windowRoot, x, y));
    }

    private class CursorView extends View {
        private final Paint ring = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final Paint dot = new Paint(Paint.ANTI_ALIAS_FLAG);
        CursorView(Context ctx) {
            super(ctx);
            ring.setStyle(Paint.Style.STROKE); ring.setStrokeWidth(dp(3)); ring.setColor(ACCENT);
            dot.setStyle(Paint.Style.FILL); dot.setColor(Color.WHITE);
            setClickable(false); setFocusable(false);
        }
        @Override protected void onDraw(Canvas canvas) {
            super.onDraw(canvas);
            if (!browserMouse || currentScreen != browserPane) return;
            canvas.drawCircle(cursorX, cursorY, dp(16), ring);
            canvas.drawCircle(cursorX, cursorY, dp(3), dot);
        }
    }

    // ---------------------------------------------------------- Ad blocking

    static class AdBlock {
        static volatile boolean enabled = true;
        private static final String[] HOSTS = {
            "doubleclick.net", "googlesyndication.com", "googleadservices.com",
            "adservice.google.com", "ads.google.com", "pagead2.googlesyndication.com",
            "amazon-adsystem.com", "ads.yahoo.com", "advertising.com",
            "adnxs.com", "adsrvr.org", "criteo.com", "criteo.net",
            "outbrain.com", "taboola.com", "mgid.com", "revcontent.com",
            "smartadserver.com", "pubmatic.com", "rubiconproject.com",
            "openx.net", "casalemedia.com", "contextweb.com", "bidswitch.net",
            "moatads.com", "scorecardresearch.com", "quantserve.com",
            "zedo.com", "adform.net", "flashtalking.com", "sizmek.com",
            "unityads.unity3d.com", "applovin.com", "mopub.com",
            "chartboost.com", "inmobi.com", "vungle.com", "adcolony.com",
            "supersonicads.com", "ironsrc.com", "tapjoy.com",
            "adsafeprotected.com", "2mdn.net", "imasdk.googleapis.com",
            "an.facebook.com", "ads-twitter.com", "analytics.twitter.com",
            "ads.pinterest.com", "ads.linkedin.com", "bat.bing.com", "ads.bing.com",
            "popads.net", "popcash.net", "adcash.com", "exoclick.com",
            "trafficjunky.com", "juicyads.com", "adsterra.com", "propellerads.com",
            "hilltopads.net", "clickadu.com", "adnium.com", "tsyndicate.com",
            "a-ads.com", "coinzilla.com", "bitmedia.io", "adbtc.top",
            "zeroredirect1.com", "zeroredirect2.com", "redirectvoluum.com",
            "trackvoluum.com", "voluumtrk.com", "bemob.com", "propeller-tracking.com",
            "onclickmax.com", "onclickgenius.com", "push.house", "pushwoosh.com",
            "onesignal.com", "webpushr.com", "subscribers.com", "pushengage.com",
            "srvtrck.com", "yllix.com", "ad-maven.com", "admaven.com",
            "popmyads.com", "adtng.com", "trafficfactory.com", "exosrv.com",
            "realsrv.com", "magsrv.com", "bngpt.com", "tiktok.com/ads"
        };
        static boolean isAd(String url) {
            if (!enabled || url == null) return false;
            try {
                String host = new URL(url).getHost();
                if (host == null) return false;
                host = host.toLowerCase(Locale.US);
                for (String h : HOSTS) {
                    if (h.contains("/")) { if (url.contains(h)) return true; }
                    else if (host.equals(h) || host.endsWith("." + h)) return true;
                }
            } catch (Exception ignored) {}
            return false;
        }
        static WebResourceResponse empty() {
            return new WebResourceResponse("text/plain", "UTF-8", 204, "No Content",
                    null, new ByteArrayInputStream(new byte[0]));
        }
    }
}
