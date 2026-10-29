//! Color themes and persisted UI settings.
//!
//! Four palettes, all designed for dark terminal backgrounds. The active
//! theme and the line-number gutter toggle are saved to
//! `~/.config/fex/settings` so they survive restarts.

use ratatui::style::Color;
use std::path::PathBuf;

/// One named palette. Roles keep the UI consistent: every accent-colored
/// element changes together when the theme changes.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub name: &'static str,
    /// Key hints, status messages, cursor markers.
    pub accent: Color,
    /// Hint bars, gutters, grid lines, scrollbars.
    pub dim: Color,
    /// Pane titles, markdown headings.
    pub heading: Color,
    /// CSV header row, links.
    pub header: Color,
    /// Markdown code spans.
    pub code: Color,
    /// Destructive confirmations, errors.
    pub danger: Color,
    /// Syntax highlighting: keywords, strings, comments, numbers, types,
    /// function calls.
    pub syn_keyword: Color,
    pub syn_string: Color,
    pub syn_comment: Color,
    pub syn_number: Color,
    pub syn_type: Color,
    pub syn_func: Color,
}

pub const THEMES: &[Theme] = &[
    Theme {
        name: "Dark",
        accent: Color::Rgb(189, 147, 249),
        dim: Color::DarkGray,
        heading: Color::Blue,
        header: Color::Cyan,
        code: Color::Green,
        danger: Color::Red,
        syn_keyword: Color::Blue,
        syn_string: Color::Green,
        syn_comment: Color::DarkGray,
        syn_number: Color::Magenta,
        syn_type: Color::Cyan,
        syn_func: Color::Yellow,
    },
    Theme {
        name: "Solarized",
        accent: Color::Rgb(181, 137, 0),
        dim: Color::Rgb(88, 110, 117),
        heading: Color::Rgb(38, 139, 210),
        header: Color::Rgb(42, 161, 152),
        code: Color::Rgb(133, 153, 0),
        danger: Color::Rgb(220, 50, 47),
        syn_keyword: Color::Rgb(38, 139, 210),
        syn_string: Color::Rgb(133, 153, 0),
        syn_comment: Color::Rgb(88, 110, 117),
        syn_number: Color::Rgb(211, 54, 130),
        syn_type: Color::Rgb(42, 161, 152),
        syn_func: Color::Rgb(203, 75, 22),
    },
    Theme {
        name: "Dracula",
        accent: Color::Rgb(255, 121, 198),
        dim: Color::Rgb(98, 114, 164),
        heading: Color::Rgb(189, 147, 249),
        header: Color::Rgb(139, 233, 253),
        code: Color::Rgb(80, 250, 123),
        danger: Color::Rgb(255, 85, 85),
        syn_keyword: Color::Rgb(255, 121, 198),
        syn_string: Color::Rgb(80, 250, 123),
        syn_comment: Color::Rgb(98, 114, 164),
        syn_number: Color::Rgb(255, 184, 108),
        syn_type: Color::Rgb(139, 233, 253),
        syn_func: Color::Rgb(80, 250, 123),
    },
    Theme {
        name: "Mono",
        accent: Color::White,
        dim: Color::DarkGray,
        heading: Color::White,
        header: Color::Gray,
        code: Color::Gray,
        danger: Color::White,
        syn_keyword: Color::White,
        syn_string: Color::Gray,
        syn_comment: Color::DarkGray,
        syn_number: Color::Gray,
        syn_type: Color::White,
        syn_func: Color::Gray,
    },
];

fn settings_path() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(|h| {
        PathBuf::from(h)
            .join(".config")
            .join("fex")
            .join("settings")
    })
}

/// Persisted UI and accessibility settings (`~/.config/fex/settings`).
#[derive(Debug, Clone)]
pub struct Settings {
    pub theme_idx: usize,
    pub show_line_numbers: bool,
    /// Debounce between keypresses in folder navigation, in milliseconds.
    /// 0 (the default) means off.
    pub key_delay_ms: u64,
    /// When true, folder-navigation keys need Alt held (except `?` and Esc).
    pub alt_nav: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme_idx: 0,
            show_line_numbers: true,
            key_delay_ms: 0,
            alt_nav: false,
        }
    }
}

/// Load the settings. Anything missing or malformed falls back to the
/// defaults: theme 0, line numbers on, no key delay, Alt mode off.
pub fn load_settings() -> Settings {
    let Some(path) = settings_path() else {
        return Settings::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Settings::default();
    };
    parse_settings(&text)
}

/// Parse settings text (one `key=value` per line). Split out so tests can
/// exercise the real loader logic without touching the home directory.
fn parse_settings(text: &str) -> Settings {
    let mut s = Settings::default();
    for line in text.lines() {
        let (k, v) = match line.split_once('=') {
            Some(p) => p,
            None => continue,
        };
        match k.trim() {
            "theme" => {
                if let Some(i) = THEMES.iter().position(|t| t.name == v.trim()) {
                    s.theme_idx = i;
                }
            }
            "line_numbers" => {
                s.show_line_numbers = v.trim() != "false";
            }
            "key_delay_ms" => {
                if let Ok(ms) = v.trim().parse::<u64>() {
                    s.key_delay_ms = ms.min(5000);
                }
            }
            "alt_nav" => {
                s.alt_nav = v.trim() == "true";
            }
            _ => {}
        }
    }
    s
}

/// Persist the settings. Best effort: failures are silently ignored.
pub fn save_settings(s: &Settings) {
    let Some(path) = settings_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let name = THEMES
        .get(s.theme_idx)
        .map(|t| t.name)
        .unwrap_or(THEMES[0].name);
    let text = format!(
        "theme={name}\nline_numbers={}\nkey_delay_ms={}\nalt_nav={}\n",
        s.show_line_numbers, s.key_delay_ms, s.alt_nav
    );
    let _ = std::fs::write(&path, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_names_are_unique() {
        let mut names: Vec<&str> = THEMES.iter().map(|t| t.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), THEMES.len());
    }

    #[test]
    fn settings_roundtrip_format() {
        // The writer output must parse back through the real loader logic.
        let text = format!(
            "theme={}\nline_numbers=false\nkey_delay_ms=250\nalt_nav=true\n",
            THEMES[2].name
        );
        let s = parse_settings(&text);
        assert_eq!(s.theme_idx, 2);
        assert!(!s.show_line_numbers);
        assert_eq!(s.key_delay_ms, 250);
        assert!(s.alt_nav);
    }

    #[test]
    fn settings_defaults_and_bad_input() {
        let s = parse_settings("");
        assert_eq!(s.key_delay_ms, 0);
        assert!(!s.alt_nav);
        assert!(s.show_line_numbers);
        // Malformed values fall back to defaults, not panics.
        let s = parse_settings("key_delay_ms=many\nalt_nav=yes\ntheme=nope\n");
        assert_eq!(s.key_delay_ms, 0);
        assert!(!s.alt_nav);
        assert_eq!(s.theme_idx, 0);
        // Delay is clamped so a typo can't freeze the UI for minutes.
        let s = parse_settings("key_delay_ms=999999\n");
        assert_eq!(s.key_delay_ms, 5000);
    }
}
