//! Network devices: mDNS discovery of SMB hosts, share listing, and
//! mounting shares for browsing inside fex. Also lists local drives, so
//! the Network view (`G`) is one combined "Drives & network" page.
//!
//! - Discovery uses mDNS/Bonjour (`_smb._tcp.local.`), the same mechanism
//!   Finder and Windows Network use. Pure Rust, no system services needed.
//! - macOS mounts shares with `mount_smbfs` into a temp dir and unmounts
//!   them when the tab closes.
//! - Windows browses `\\host\share` UNC paths directly — no mount needed.
//! - Linux is not supported for mounting (clear error); mount the share
//!   with the OS and browse it as a local folder instead.

use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const SMB_SERVICE: &str = "_smb._tcp.local.";
const CMD_TIMEOUT: u64 = 15;
const MOUNT_TIMEOUT: u64 = 30;

/// A device on the LAN advertising SMB file sharing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetDevice {
    /// Pretty name from the mDNS record, e.g. "Kurt's MacBook".
    pub name: String,
    /// IPv4 address (or hostname) to connect to.
    pub host: String,
    /// True when typed by hand rather than discovered.
    pub manual: bool,
}

impl NetDevice {
    pub fn manual(host: &str) -> Self {
        Self {
            name: host.to_string(),
            host: host.to_string(),
            manual: true,
        }
    }
}

/// A local drive/volume, e.g. "Macintosh HD" or a USB stick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drive {
    /// Volume name ("Macintosh HD"); falls back to the mount point when
    /// the OS reports no name.
    pub name: String,
    /// Where it is mounted, e.g. `/` or `/Volumes/USB STICK`.
    pub mount_point: PathBuf,
    pub available: u64,
    pub total: u64,
    pub removable: bool,
}

/// List mounted drives/volumes via `sysinfo` (cross-platform). Entries
/// with no reported size are skipped; duplicates by mount point are
/// dropped. Sorted by name.
pub fn list_drives() -> Vec<Drive> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let mut drives: Vec<Drive> = disks
        .list()
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| {
            let mount_point = d.mount_point().to_path_buf();
            let name = d.name().to_string_lossy().trim().to_string();
            let name = if name.is_empty() {
                mount_point.to_string_lossy().into_owned()
            } else {
                name
            };
            Drive {
                name,
                mount_point,
                available: d.available_space(),
                total: d.total_space(),
                removable: d.is_removable(),
            }
        })
        .collect();
    drives.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    drives.dedup_by(|a, b| a.mount_point == b.mount_point);
    drives.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    drives
}

/// One-line summary of a drive's space: "120 GB free of 500 GB".
pub fn drive_space(d: &Drive) -> String {
    format!(
        "{} free of {}",
        format_size(d.available),
        format_size(d.total)
    )
}

/// Human byte size: 1000-based, like macOS/Finder ("120 GB", "850 MB").
pub fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1000.0 && u < UNITS.len() - 1 {
        v /= 1000.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.0} {}", UNITS[u])
    }
}

/// A mounted share ready to browse: the local path plus whether fex owns
/// the mount (and must unmount it later). Windows UNC paths need no
/// unmounting.
pub struct MountedShare {
    pub path: PathBuf,
    pub needs_unmount: bool,
}

/// Background mDNS browser. The daemon and its thread live as long as this
/// does; drop it to stop discovering.
pub struct Discovery {
    rx: mpsc::Receiver<Vec<NetDevice>>,
}

impl Discovery {
    pub fn start() -> io::Result<Discovery> {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || run_discovery(tx));
        Ok(Discovery { rx })
    }

    /// Newest pending device-list snapshot, if any (non-blocking).
    pub fn poll(&self) -> Option<Vec<NetDevice>> {
        let mut last = None;
        while let Ok(devs) = self.rx.try_recv() {
            last = Some(devs);
        }
        last
    }
}

fn run_discovery(tx: mpsc::Sender<Vec<NetDevice>>) {
    let Ok(daemon) = ServiceDaemon::new() else {
        return;
    };
    let Ok(recv) = daemon.browse(SMB_SERVICE) else {
        return;
    };
    let mut map: HashMap<String, NetDevice> = HashMap::new();
    let mut last: Vec<NetDevice> = Vec::new();
    let push = |map: &HashMap<String, NetDevice>, last: &mut Vec<NetDevice>| {
        let mut v: Vec<NetDevice> = map.values().cloned().collect();
        v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        if v != *last {
            *last = v.clone();
            let _ = tx.send(v);
        }
    };
    for ev in recv.iter() {
        match ev {
            ServiceEvent::ServiceResolved(info) => {
                let fullname = info.get_fullname().to_string();
                let name = fullname
                    .strip_suffix(SMB_SERVICE.trim_end_matches('.'))
                    .unwrap_or(&fullname)
                    .trim_matches('.')
                    .replace("\\.", ".");
                let host = info
                    .get_addresses_v4()
                    .iter()
                    .next()
                    .map(|ip| ip.to_string())
                    .unwrap_or_else(|| info.get_hostname().trim_end_matches('.').to_string());
                map.insert(
                    fullname,
                    NetDevice {
                        name,
                        host,
                        manual: false,
                    },
                );
                push(&map, &mut last);
            }
            ServiceEvent::ServiceRemoved(_, fullname) => {
                map.remove(&fullname);
                push(&map, &mut last);
            }
            _ => {}
        }
    }
}

/// List the SMB shares on a host. macOS uses `smbutil view`, Windows
/// `net view`, Linux `smbclient` (if installed).
pub fn list_shares(host: &str) -> io::Result<Vec<String>> {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("net");
        c.args(["view", &format!(r"\\{host}")]);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = Command::new("smbutil");
        c.args(["view", &format!("//guest@{host}")]);
        c
    } else {
        let mut c = Command::new("smbclient");
        c.args(["-N", "-L", host]);
        c
    };
    let out = run_with_timeout(&mut cmd, CMD_TIMEOUT).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            io::Error::new(
                io::ErrorKind::NotFound,
                "share-listing tool not found on this system",
            )
        } else {
            e
        }
    })?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let err = if err.is_empty() {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        } else {
            err
        };
        return Err(io::Error::new(
            io::ErrorKind::Other,
            if err.is_empty() {
                "could not list shares".to_string()
            } else {
                err
            },
        ));
    }
    Ok(parse_share_list(&String::from_utf8_lossy(&out.stdout)))
}

/// Parse `smbutil view`, `net view`, or `smbclient -L` output. Every line
/// under the dashed header contributes its first token as a share name;
/// administrative shares (IPC$, ADMIN$) are dropped.
pub fn parse_share_list(text: &str) -> Vec<String> {
    let mut shares = Vec::new();
    let mut in_list = false;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if !in_list {
            // The dashed separator line opens the listing.
            if t.starts_with("---") {
                in_list = true;
            }
            continue;
        }
        let low = t.to_lowercase();
        // Footers end the listing.
        if low.starts_with("the command completed")
            || low.ends_with("shares listed")
            || low.contains("no workgroup available")
        {
            break;
        }
        let name = first_column(t);
        if name.is_empty() {
            continue;
        }
        match name.to_uppercase().as_str() {
            "IPC$" | "ADMIN$" | "PRINT$" => continue,
            _ => shares.push(name.to_string()),
        }
    }
    shares
}

/// The first column of a columnar listing: columns are separated by two or
/// more spaces (or a tab), so names containing single spaces survive.
fn first_column(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\t' {
            return line[..i].trim_end();
        }
        if bytes[i] == b' ' && bytes.get(i + 1) == Some(&b' ') {
            return line[..i].trim_end();
        }
        i += 1;
    }
    line.trim_end()
}

/// Mount `\\host\share` for browsing inside fex. `user`/`pass` may be
/// empty for a guest attempt; when the server rejects it, the error
/// message mentions authentication (see [`is_auth_error`]).
pub fn mount_share(host: &str, share: &str, user: &str, pass: &str) -> io::Result<MountedShare> {
    if cfg!(windows) {
        // No mount needed: Rust's std::fs reads UNC paths directly.
        let path = PathBuf::from(format!(r"\\{host}\{share}"));
        // Probe now so auth/connection failures surface here, not later.
        std::fs::read_dir(&path).map(|_| ())?;
        return Ok(MountedShare {
            path,
            needs_unmount: false,
        });
    }
    if cfg!(target_os = "macos") {
        let node = mount_point(host, share)?;
        std::fs::create_dir_all(&node)?;
        let url = if user.is_empty() {
            format!("//{host}/{share}")
        } else {
            format!("//{}:{}@{host}/{share}", url_encode(user), url_encode(pass))
        };
        let mut cmd = Command::new("mount_smbfs");
        cmd.arg(&url).arg(&node);
        match run_with_timeout(&mut cmd, MOUNT_TIMEOUT) {
            Ok(out) if out.status.success() => Ok(MountedShare {
                path: node,
                needs_unmount: true,
            }),
            Ok(out) => {
                let _ = std::fs::remove_dir(&node);
                Err(io::Error::new(io::ErrorKind::Other, mount_err(&out)))
            }
            Err(e) => {
                let _ = std::fs::remove_dir(&node);
                Err(e)
            }
        }
    } else {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "network mounting is supported on macOS and Windows; on Linux, mount the share with your OS and browse it as a local folder",
        ))
    }
}

/// Unmount a path previously returned by [`mount_share`].
pub fn unmount_path(path: &Path) -> io::Result<()> {
    if cfg!(windows) {
        return Ok(()); // UNC paths are never mounted
    }
    if cfg!(target_os = "macos") {
        let mut cmd = Command::new("umount");
        cmd.arg(path);
        let out = run_with_timeout(&mut cmd, CMD_TIMEOUT)?;
        if out.status.success() {
            Ok(())
        } else {
            Err(io::Error::new(io::ErrorKind::Other, mount_err(&out)))
        }
    } else {
        Ok(())
    }
}

/// True when a mount error looks like bad/missing credentials.
pub fn is_auth_error(e: &io::Error) -> bool {
    let m = e.to_string().to_lowercase();
    m.contains("auth") || m.contains("permission") || m.contains("access denied")
}

fn mount_point(host: &str, share: &str) -> io::Result<PathBuf> {
    let safe: String = format!("{host}-{share}")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    Ok(std::env::temp_dir().join("fex-mnt").join(safe))
}

fn mount_err(out: &std::process::Output) -> String {
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if err.is_empty() {
        format!("mount failed (exit {})", out.status)
    } else {
        err
    }
}

/// Percent-encode credentials for the `//user:pass@host/share` URL.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Run a command, killing it if it exceeds `secs` seconds.
fn run_with_timeout(cmd: &mut Command, secs: u64) -> io::Result<std::process::Output> {
    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(Duration::from_secs(secs)) {
        Ok(r) => r,
        Err(_) => {
            kill_pid(pid);
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("command timed out after {secs}s"),
            ))
        }
    }
}

#[cfg(unix)]
fn kill_pid(pid: u32) {
    let _ = Command::new("kill").arg("-9").arg(pid.to_string()).output();
}

#[cfg(windows)]
fn kill_pid(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .output();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_smbutil_view() {
        let out = "\
Share                                           Type       Comments
-------------------------------                 ----       --------
IPC$                                            Disk
TimeMachine                                     Disk
Shared Files                                    Disk       shared stuff
3 shares listed
";
        assert_eq!(parse_share_list(out), vec!["TimeMachine", "Shared Files"]);
    }

    #[test]
    fn parses_net_view() {
        let out = "\
Shared resources at \\\\NAS

Share name             Type       Used as  Comment
-------------------------------------------------
Media                  Disk
Backups                Disk                 nightly
IPC$                   IPC       Remote IPC
The command completed successfully.
";
        assert_eq!(parse_share_list(out), vec!["Media", "Backups"]);
    }

    #[test]
    fn parses_smbclient_list() {
        let out = "\
\tSharename       Type      Comment
\t---------       ----      -------
\tShared          Disk
\tADMIN$          Disk      Remote Admin
\tIPC$            IPC       IPC Service
SMB1 disabled -- no workgroup available
";
        assert_eq!(parse_share_list(out), vec!["Shared"]);
    }

    #[test]
    fn parses_empty_listing() {
        assert!(parse_share_list("nothing here\n").is_empty());
        assert!(parse_share_list("").is_empty());
    }

    #[test]
    fn url_encodes_credentials() {
        assert_eq!(url_encode("user"), "user");
        assert_eq!(url_encode("p@ss:word/1"), "p%40ss%3Aword%2F1");
    }

    #[test]
    fn auth_error_detection() {
        let e = io::Error::new(
            io::ErrorKind::Other,
            "mount_smbfs: server rejected the connection: Authentication error",
        );
        assert!(is_auth_error(&e));
        let e2 = io::Error::new(io::ErrorKind::Other, "No route to host");
        assert!(!is_auth_error(&e2));
    }

    #[test]
    fn formats_byte_sizes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1500), "2 KB");
        assert_eq!(format_size(850_000_000), "850 MB");
        assert_eq!(format_size(120_000_000_000), "120 GB");
        assert_eq!(format_size(2_500_000_000_000), "2 TB");
    }

    #[test]
    fn lists_at_least_one_drive() {
        let drives = list_drives();
        assert!(!drives.is_empty(), "expected at least the root filesystem");
        for d in &drives {
            assert!(d.total > 0);
            assert!(d.available <= d.total);
            assert!(!d.name.is_empty());
        }
        // No duplicate mount points, sorted by name.
        let pts: Vec<_> = drives.iter().map(|d| &d.mount_point).collect();
        let mut uniq = pts.clone();
        uniq.dedup();
        assert_eq!(uniq.len(), pts.len());
        let names: Vec<_> = drives.iter().map(|d| d.name.to_lowercase()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn drive_space_summary() {
        let d = Drive {
            name: String::from("Macintosh HD"),
            mount_point: PathBuf::from("/"),
            available: 120_000_000_000,
            total: 500_000_000_000,
            removable: false,
        };
        assert_eq!(drive_space(&d), "120 GB free of 500 GB");
    }
}
