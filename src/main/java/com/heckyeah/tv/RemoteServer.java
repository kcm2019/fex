package com.heckyeah.tv;

import android.content.Context;
import android.net.nsd.NsdManager;
import android.net.nsd.NsdServiceInfo;
import android.net.wifi.WifiManager;
import android.os.Handler;
import android.util.Base64;
import android.util.Log;

import org.json.JSONObject;

import java.io.BufferedInputStream;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.Inet4Address;
import java.net.InetAddress;
import java.net.NetworkInterface;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.SecureRandom;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Enumeration;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.TreeMap;
import java.util.UUID;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * v1.19: Phone Remote server. A hand-rolled HTTP + WebSocket
 * (RFC 6455) server on the LAN port 18765: serves the phone page
 * (assets/remote.html), pairs phones with a PIN (max 2 sessions),
 * and forwards control commands to the activity through Host.
 * Framework APIs only, no dependencies.
 *
 * Threading: 1 accept thread + a bounded pool of connection
 * threads (max 6); each WebSocket client additionally gets one
 * writer thread fed by a queue. Every Host callback is posted
 * to the activity's UI handler - no UI/WebView work ever happens
 * on a server thread. Everything is exception-guarded: this
 * server must never be able to crash the app.
 *
 * Security model: LAN-only convenience, OFF by default. The PIN
 * is regenerated at every server start, 5 failed pair attempts
 * lock pairing for 60s, and session tokens live in memory only
 * (an app restart or server restart invalidates every phone).
 */
public class RemoteServer {

    public static final int PORT = 18765;
    private static final int MAX_SESSIONS = 2;
    private static final int MAX_CONN_THREADS = 6;
    private static final int MAX_PAIR_FAILS = 5;
    private static final long PAIR_LOCK_MS = 60000;
    private static final String WS_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
    private static final float MOVE_GAIN = 1.6f; // trackpad feel

    /** All methods are invoked on the activity's UI thread. */
    public interface Host {
        void remoteMove(float dx, float dy);
        void remoteTap();
        void remoteLongTap();
        void remoteScroll(int dyPx);
        void remoteBack();
        void remoteForward();
        void remoteReload();
        void remoteDone();
        void remotePlayPause();
        void remoteUrl(String u);
        void remoteType(String text);
        void remoteEnterKey();
        /** v1.22: D-pad mode key: up|down|left|right|ok|back|menu. */
        void remoteKey(String k);
        void remoteVolume(int dir);
        void remoteSeek(int seconds);
        void remoteSessionsChanged();
    }

    private final Context ctx;
    private final Handler ui;
    private final Host host;
    private final SecureRandom random = new SecureRandom();

    private volatile boolean running = false;
    private ServerSocket serverSocket;
    private Thread acceptThread;
    private final AtomicInteger connThreads = new AtomicInteger(0);

    private volatile String pin = "";
    private final Set<String> sessions =
            Collections.synchronizedSet(new HashSet<String>());
    private final List<WsConn> wsClients =
            Collections.synchronizedList(new ArrayList<WsConn>());
    private int pairFails = 0;
    private long pairLockUntil = 0;

    private NsdManager nsdManager;
    private NsdManager.RegistrationListener nsdListener;

    private String pageHtmlCache;

    public RemoteServer(Context ctx, Handler ui, Host host) {
        this.ctx = ctx.getApplicationContext();
        this.ui = ui;
        this.host = host;
    }

    // ---------------------------------------------------------- lifecycle

    public synchronized void start() {
        if (running) return;
        pin = String.format("%04d", random.nextInt(10000));
        pairFails = 0;
        pairLockUntil = 0;
        try {
            serverSocket = new ServerSocket(PORT);
        } catch (Exception e) {
            Log.d("HeckYeahFS", "remote start FAILED: " + e);
            return;
        }
        running = true;
        acceptThread = new Thread(this::acceptLoop, "hy-remote-accept");
        acceptThread.setDaemon(true);
        acceptThread.start();
        registerNsd();
        Log.d("HeckYeahFS", "remote server start port=" + PORT);
    }

    public synchronized void stop() {
        if (!running) return;
        running = false;
        unregisterNsd();
        try { if (serverSocket != null) serverSocket.close(); } catch (Exception ignored) {}
        serverSocket = null;
        synchronized (wsClients) {
            for (WsConn c : new ArrayList<>(wsClients)) c.closeQuietly();
            wsClients.clear();
        }
        sessions.clear();
        Log.d("HeckYeahFS", "remote server stop");
    }

    public boolean isRunning() { return running; }
    public String pin() { return pin; }
    public int sessionCount() { return sessions.size(); }

    /** New PIN button: fresh PIN, every paired phone dropped. */
    public void regeneratePinAndDropSessions() {
        pin = String.format("%04d", random.nextInt(10000));
        pairFails = 0;
        pairLockUntil = 0;
        dropAllSessions();
    }

    public void dropAllSessions() {
        sessions.clear();
        synchronized (wsClients) {
            for (WsConn c : new ArrayList<>(wsClients)) c.closeQuietly();
            wsClients.clear();
        }
        notifySessionsChanged();
    }

    private void notifySessionsChanged() {
        ui.post(() -> {
            try { host.remoteSessionsChanged(); } catch (Exception ignored) {}
        });
    }

    /** Sends a state JSON text frame to every connected phone. */
    public void broadcast(String msg) {
        if (!running) return;
        byte[] frame = buildTextFrame(msg);
        synchronized (wsClients) {
            for (WsConn c : wsClients) c.enqueue(frame);
        }
    }

    // ---------------------------------------------------------- mDNS (extra B)

    private void registerNsd() {
        try {
            nsdManager = (NsdManager) ctx.getSystemService(Context.NSD_SERVICE);
            if (nsdManager == null) return;
            NsdServiceInfo info = new NsdServiceInfo();
            info.setServiceName("HeckYeah TV");
            info.setServiceType("_http._tcp.");
            info.setPort(PORT);
            nsdListener = new NsdManager.RegistrationListener() {
                @Override public void onServiceRegistered(NsdServiceInfo si) {
                    Log.d("HeckYeahFS", "remote nsd registered: " + si.getServiceName());
                }
                @Override public void onRegistrationFailed(NsdServiceInfo si, int err) {
                    Log.d("HeckYeahFS", "remote nsd registration failed: " + err);
                }
                @Override public void onServiceUnregistered(NsdServiceInfo si) {}
                @Override public void onUnregistrationFailed(NsdServiceInfo si, int err) {}
            };
            nsdManager.registerService(info, NsdManager.PROTOCOL_DNS_SD, nsdListener);
        } catch (Exception e) {
            Log.d("HeckYeahFS", "remote nsd unavailable: " + e);
            nsdManager = null;
            nsdListener = null;
        }
    }

    private void unregisterNsd() {
        try {
            if (nsdManager != null && nsdListener != null)
                nsdManager.unregisterService(nsdListener);
        } catch (Exception ignored) {}
        nsdManager = null;
        nsdListener = null;
    }

    // ---------------------------------------------------------- IP helper

    /** WiFi IPv4 of this device (for the pairing panel). */
    public static String localIpAddress(Context ctx) {
        try {
            WifiManager wm = (WifiManager) ctx.getApplicationContext()
                    .getSystemService(Context.WIFI_SERVICE);
            if (wm != null && wm.getConnectionInfo() != null) {
                int ip = wm.getConnectionInfo().getIpAddress();
                if (ip != 0) {
                    return (ip & 0xff) + "." + ((ip >> 8) & 0xff) + "."
                            + ((ip >> 16) & 0xff) + "." + ((ip >> 24) & 0xff);
                }
            }
        } catch (Exception ignored) {}
        try {
            Enumeration<NetworkInterface> ifs = NetworkInterface.getNetworkInterfaces();
            while (ifs != null && ifs.hasMoreElements()) {
                NetworkInterface ni = ifs.nextElement();
                try { if (!ni.isUp() || ni.isLoopback()) continue; }
                catch (Exception e) { continue; }
                Enumeration<InetAddress> addrs = ni.getInetAddresses();
                while (addrs.hasMoreElements()) {
                    InetAddress a = addrs.nextElement();
                    if (!a.isLoopbackAddress() && a instanceof Inet4Address)
                        return a.getHostAddress();
                }
            }
        } catch (Exception ignored) {}
        return "127.0.0.1";
    }

    // ---------------------------------------------------------- accept + HTTP

    private void acceptLoop() {
        while (running) {
            final Socket s;
            try {
                s = serverSocket.accept();
            } catch (Exception e) {
                if (running) Log.d("HeckYeahFS", "remote accept error: " + e);
                break;
            }
            if (connThreads.incrementAndGet() > MAX_CONN_THREADS) {
                connThreads.decrementAndGet();
                try { s.close(); } catch (Exception ignored) {}
                continue;
            }
            Thread t = new Thread(() -> {
                try { handleConnection(s); }
                catch (Exception ignored) {}
                finally {
                    connThreads.decrementAndGet();
                    try { s.close(); } catch (Exception ignored) {}
                }
            }, "hy-remote-conn");
            t.setDaemon(true);
            t.start();
        }
    }

    private void handleConnection(Socket s) throws Exception {
        BufferedInputStream in = new BufferedInputStream(s.getInputStream());
        OutputStream out = s.getOutputStream();
        byte[] headBytes = readUntilHeaderEnd(in);
        if (headBytes == null) return;
        String head = new String(headBytes, StandardCharsets.ISO_8859_1);
        String[] lines = head.split("\r\n");
        if (lines.length == 0) return;
        String[] requestLine = lines[0].split(" ");
        if (requestLine.length < 2) return;
        String method = requestLine[0];
        String path = requestLine[1];
        int qi = path.indexOf('?');
        if (qi >= 0) path = path.substring(0, qi);
        TreeMap<String, String> headers = new TreeMap<>(String.CASE_INSENSITIVE_ORDER);
        for (int i = 1; i < lines.length; i++) {
            int c = lines[i].indexOf(':');
            if (c > 0) headers.put(lines[i].substring(0, c).trim(),
                    lines[i].substring(c + 1).trim());
        }

        if ("GET".equals(method) && "/ws".equals(path)) {
            handleWebSocket(s, in, out, headers);
            return;
        }
        if ("GET".equals(method) && ("/".equals(path) || "/index.html".equals(path))) {
            byte[] body = pageHtml().getBytes(StandardCharsets.UTF_8);
            writeResponse(out, 200, "OK", "text/html; charset=utf-8", body, null);
            return;
        }
        if ("POST".equals(method) && "/pair".equals(path)) {
            int len = 0;
            try { len = Integer.parseInt(headers.getOrDefault("Content-Length", "0")); }
            catch (Exception ignored) {}
            byte[] body = readFully(in, Math.min(Math.max(len, 0), 4096));
            handlePair(out, body);
            return;
        }
        writeResponse(out, 404, "Not Found", "text/plain", "not found".getBytes(StandardCharsets.UTF_8), null);
    }

    private void handlePair(OutputStream out, byte[] body) throws Exception {
        long now = System.currentTimeMillis();
        if (now < pairLockUntil) {
            writeJson(out, 429, "{\"ok\":false,\"err\":\"locked\"}", null);
            return;
        }
        String given = "";
        try {
            JSONObject o = new JSONObject(new String(body, StandardCharsets.UTF_8));
            given = o.optString("pin", "");
        } catch (Exception ignored) {}
        if (!given.isEmpty() && given.equals(pin)) {
            if (sessions.size() >= MAX_SESSIONS) {
                writeJson(out, 403, "{\"ok\":false,\"err\":\"full\"}", null);
                return;
            }
            String token = UUID.randomUUID().toString();
            sessions.add(token);
            pairFails = 0;
            Log.d("HeckYeahFS", "remote paired n=" + sessions.size());
            writeJson(out, 200, "{\"ok\":true}",
                    "Set-Cookie: hy=" + token + "; Path=/; Max-Age=31536000\r\n");
            notifySessionsChanged();
            return;
        }
        pairFails++;
        if (pairFails >= MAX_PAIR_FAILS) {
            pairLockUntil = now + PAIR_LOCK_MS;
            pairFails = 0;
            Log.d("HeckYeahFS", "remote pairing locked 60s");
        }
        writeJson(out, 401, "{\"ok\":false,\"err\":\"pin\"}", null);
    }

    private String pageHtml() {
        if (pageHtmlCache != null) return pageHtmlCache;
        try {
            InputStream is = ctx.getAssets().open("remote.html");
            ByteArrayOutputStream buf = new ByteArrayOutputStream();
            byte[] chunk = new byte[8192];
            int n;
            while ((n = is.read(chunk)) > 0) buf.write(chunk, 0, n);
            is.close();
            pageHtmlCache = new String(buf.toByteArray(), StandardCharsets.UTF_8);
        } catch (Exception e) {
            pageHtmlCache = "<html><body>remote page missing</body></html>";
        }
        return pageHtmlCache;
    }

    private static void writeResponse(OutputStream out, int code, String reason,
                                      String contentType, byte[] body,
                                      String extraHeaders) throws Exception {
        StringBuilder sb = new StringBuilder();
        sb.append("HTTP/1.1 ").append(code).append(' ').append(reason).append("\r\n");
        sb.append("Content-Type: ").append(contentType).append("\r\n");
        sb.append("Content-Length: ").append(body.length).append("\r\n");
        // v1.32: no-store + must-revalidate. The pairing URL is
        // versioned (?v=<versionCode>) so a phone tab restored
        // from a snapshot at the OLD address cannot silently keep
        // running a stale copy of the remote page.
        sb.append("Cache-Control: no-store, must-revalidate\r\n");
        sb.append("Connection: close\r\n");
        if (extraHeaders != null) sb.append(extraHeaders);
        sb.append("\r\n");
        out.write(sb.toString().getBytes(StandardCharsets.ISO_8859_1));
        out.write(body);
        out.flush();
    }

    private static void writeJson(OutputStream out, int code, String json,
                                  String extraHeaders) throws Exception {
        String reason = code == 200 ? "OK" : code == 401 ? "Unauthorized"
                : code == 403 ? "Forbidden" : code == 429 ? "Too Many Requests" : "Error";
        writeResponse(out, code, reason, "application/json",
                json.getBytes(StandardCharsets.UTF_8), extraHeaders);
    }

    /** Reads until the CRLFCRLF header terminator (16 KB cap). */
    private static byte[] readUntilHeaderEnd(BufferedInputStream in) throws Exception {
        ByteArrayOutputStream buf = new ByteArrayOutputStream();
        int state = 0; // bytes of the \r\n\r\n terminator matched so far
        int b;
        while ((b = in.read()) != -1) {
            buf.write(b);
            if (buf.size() > 16384) return null;
            if ((state == 0 || state == 2) && b == '\r') state++;
            else if ((state == 1 || state == 3) && b == '\n') state++;
            else state = (b == '\r') ? 1 : 0;
            if (state == 4) return buf.toByteArray();
        }
        return buf.size() > 0 ? buf.toByteArray() : null;
    }

    private static byte[] readFully(InputStream in, int len) throws Exception {
        byte[] out = new byte[len];
        int off = 0;
        while (off < len) {
            int n = in.read(out, off, len - off);
            if (n < 0) break;
            off += n;
        }
        return out;
    }

    // ---------------------------------------------------------- WebSocket (RFC 6455)

    private void handleWebSocket(Socket s, BufferedInputStream in, OutputStream out,
                                 TreeMap<String, String> headers) throws Exception {
        String cookie = headers.getOrDefault("Cookie", "");
        String token = null;
        for (String part : cookie.split(";")) {
            String p = part.trim();
            if (p.startsWith("hy=")) token = p.substring(3).trim();
        }
        if (token == null || !sessions.contains(token)) {
            writeResponse(out, 401, "Unauthorized", "text/plain",
                    "pair first".getBytes(StandardCharsets.UTF_8), null);
            return;
        }
        String key = headers.getOrDefault("Sec-WebSocket-Key", "");
        String accept = wsAccept(key);
        String resp = "HTTP/1.1 101 Switching Protocols\r\n"
                + "Upgrade: websocket\r\n"
                + "Connection: Upgrade\r\n"
                + "Sec-WebSocket-Accept: " + accept + "\r\n\r\n";
        out.write(resp.getBytes(StandardCharsets.ISO_8859_1));
        out.flush();

        WsConn conn = new WsConn(s, out);
        wsClients.add(conn);
        conn.startWriter();
        notifySessionsChanged();
        try {
            wsReadLoop(conn, in);
        } finally {
            wsClients.remove(conn);
            conn.closeQuietly();
            notifySessionsChanged();
        }
    }

    private static String wsAccept(String key) {
        try {
            MessageDigest sha1 = MessageDigest.getInstance("SHA-1");
            byte[] digest = sha1.digest((key + WS_GUID).getBytes(StandardCharsets.ISO_8859_1));
            return Base64.encodeToString(digest, Base64.NO_WRAP);
        } catch (Exception e) {
            return "";
        }
    }

    private void wsReadLoop(WsConn conn, BufferedInputStream in) throws Exception {
        ByteArrayOutputStream frag = new ByteArrayOutputStream();
        boolean inFrag = false;
        while (running) {
            int b0 = in.read();
            if (b0 < 0) return;
            int b1 = in.read();
            if (b1 < 0) return;
            boolean fin = (b0 & 0x80) != 0;
            int opcode = b0 & 0x0F;
            boolean masked = (b1 & 0x80) != 0;
            long len = b1 & 0x7F;
            if (len == 126) {
                len = ((long) readByte(in) << 8) | readByte(in);
            } else if (len == 127) {
                len = 0;
                for (int i = 0; i < 8; i++) len = (len << 8) | readByte(in);
            }
            if (len > 65536) return; // absurd frame: drop the connection
            byte[] maskKey = masked ? readFully(in, 4) : null;
            byte[] payload = readFully(in, (int) len);
            if (masked && maskKey != null) {
                for (int i = 0; i < payload.length; i++)
                    payload[i] = (byte) (payload[i] ^ maskKey[i % 4]);
            }
            switch (opcode) {
                case 0x8: // close
                    conn.enqueue(buildFrame(0x8, new byte[0]));
                    return;
                case 0x9: // ping -> pong
                    conn.enqueue(buildFrame(0xA, payload));
                    break;
                case 0xA: // pong: ignore
                    break;
                case 0x1: // text
                    if (fin) {
                        dispatchCommand(new String(payload, StandardCharsets.UTF_8));
                    } else {
                        frag.reset();
                        frag.write(payload);
                        inFrag = true;
                    }
                    break;
                case 0x0: // continuation
                    if (inFrag) {
                        frag.write(payload);
                        if (fin) {
                            dispatchCommand(new String(frag.toByteArray(), StandardCharsets.UTF_8));
                            inFrag = false;
                            frag.reset();
                        }
                    }
                    break;
                default: // binary and others: ignore payload
                    break;
            }
        }
    }

    private static int readByte(InputStream in) throws Exception {
        int b = in.read();
        if (b < 0) throw new java.io.EOFException();
        return b;
    }

    private static byte[] buildFrame(int opcode, byte[] payload) {
        ByteArrayOutputStream f = new ByteArrayOutputStream();
        f.write(0x80 | opcode);
        int len = payload.length;
        if (len < 126) {
            f.write(len);
        } else if (len <= 0xFFFF) {
            f.write(126);
            f.write((len >>> 8) & 0xFF);
            f.write(len & 0xFF);
        } else {
            f.write(127);
            for (int i = 7; i >= 0; i--) f.write((int) ((len >>> (8 * i)) & 0xFF));
        }
        f.write(payload, 0, payload.length);
        return f.toByteArray();
    }

    private static byte[] buildTextFrame(String msg) {
        return buildFrame(0x1, msg.getBytes(StandardCharsets.UTF_8));
    }

    private final class WsConn {
        final Socket socket;
        final OutputStream out;
        final BlockingQueue<byte[]> queue = new LinkedBlockingQueue<>();
        volatile boolean open = true;

        WsConn(Socket socket, OutputStream out) {
            this.socket = socket;
            this.out = out;
        }

        void startWriter() {
            Thread t = new Thread(() -> {
                try {
                    while (open) {
                        byte[] frame = queue.take();
                        synchronized (out) {
                            out.write(frame);
                            out.flush();
                        }
                    }
                } catch (Exception ignored) {}
                closeQuietly();
            }, "hy-remote-ws-write");
            t.setDaemon(true);
            t.start();
        }

        void enqueue(byte[] frame) {
            if (open) queue.offer(frame);
        }

        void closeQuietly() {
            open = false;
            try { socket.close(); } catch (Exception ignored) {}
        }
    }

    // ---------------------------------------------------------- commands

    private void dispatchCommand(final String msg) {
        final JSONObject o;
        try {
            o = new JSONObject(msg);
        } catch (Exception e) {
            return;
        }
        final String t = o.optString("t", "");
        try {
            switch (t) {
                case "move": {
                    final float dx = (float) o.optDouble("dx", 0) * MOVE_GAIN;
                    final float dy = (float) o.optDouble("dy", 0) * MOVE_GAIN;
                    ui.post(() -> host.remoteMove(dx, dy));
                    break;
                }
                case "tap":
                    ui.post(() -> host.remoteTap());
                    break;
                case "longtap":
                    ui.post(() -> host.remoteLongTap());
                    break;
                case "scroll": {
                    final double dy = o.optDouble("dy", 0);
                    ui.post(() -> host.remoteScroll((int) Math.round(dy)));
                    break;
                }
                case "back":
                    ui.post(() -> host.remoteBack());
                    break;
                case "fwd":
                    ui.post(() -> host.remoteForward());
                    break;
                case "reload":
                    ui.post(() -> host.remoteReload());
                    break;
                case "done":
                    ui.post(() -> host.remoteDone());
                    break;
                case "playpause":
                    ui.post(() -> host.remotePlayPause());
                    break;
                case "url": {
                    final String u = o.optString("u", "");
                    ui.post(() -> host.remoteUrl(u));
                    break;
                }
                case "type": {
                    final String text = o.optString("text", "");
                    ui.post(() -> host.remoteType(text));
                    break;
                }
                case "key": {
                    // "enter" is the v1.19 type-field Enter (page JS);
                    // v1.22 adds physical-remote keys for D-pad mode.
                    final String k = o.optString("k", "");
                    if ("enter".equals(k)) {
                        ui.post(() -> host.remoteEnterKey());
                    } else if ("up".equals(k) || "down".equals(k)
                            || "left".equals(k) || "right".equals(k)
                            || "ok".equals(k) || "back".equals(k)
                            || "menu".equals(k)) {
                        ui.post(() -> host.remoteKey(k));
                    }
                    break;
                }
                case "vol": {
                    final int d = o.optInt("d", 0);
                    ui.post(() -> host.remoteVolume(d));
                    break;
                }
                case "seek": {
                    final int sec = o.optInt("s", 0);
                    ui.post(() -> host.remoteSeek(sec));
                    break;
                }
                default:
                    break;
            }
        } catch (Exception ignored) {}
    }
}
