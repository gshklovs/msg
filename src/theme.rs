//! The five looks the popup can wear.
//!
//! A theme is data, not code: a [`Palette`] of colors and a [`Metrics`] of
//! sizes, plus the font family names its text uses. `draw.rs` reads those and
//! branches only where the layouts genuinely differ.

use eframe::egui;
use egui::{Color32, FontFamily, FontId};
use std::sync::Arc;

/// Named after the look, not the mechanism, because that is how the config
/// file reads: `theme = "brutalist"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// Frosted dark panel with rounded corners, in the spirit of Spotlight.
    #[default]
    Spotlight,
    /// A terminal: monospace, square, prompt at the bottom, list growing up.
    Terminal,
    /// Warm paper-dark serif, initials in tinted discs, one line of prose per row.
    Editorial,
    /// Black and acid yellow, hard rules, everything shouting in uppercase.
    Brutalist,
    /// Dense product rows: avatar, name, chips, subtitle, timestamp.
    Rich,
}

pub const THEME_NAMES: [&str; 5] = ["spotlight", "terminal", "editorial", "brutalist", "rich"];

impl std::str::FromStr for Theme {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "spotlight" => Ok(Self::Spotlight),
            "terminal" => Ok(Self::Terminal),
            "editorial" => Ok(Self::Editorial),
            "brutalist" => Ok(Self::Brutalist),
            "rich" => Ok(Self::Rich),
            other => Err(format!(
                "unknown theme `{other}`; expected one of {}",
                THEME_NAMES.join(", ")
            )),
        }
    }
}

impl std::fmt::Display for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(THEME_NAMES[*self as usize])
    }
}

/// Every color any theme paints. Fields a theme does not use are set to
/// something harmless rather than made optional, which keeps `draw.rs` flat.
pub struct Palette {
    pub window_bg: Color32,
    pub window_border: Color32,
    pub header_bg: Option<Color32>,
    /// The hairline under the header and above the footer.
    pub rule: Color32,
    pub text: Color32,
    /// Secondary text: timestamps, subtitles, key hints.
    pub dim: Color32,
    /// The caret, and whatever else the theme wants to shout with.
    pub accent: Color32,
    pub match_fg: Color32,
    pub row_hover: Color32,
    pub sel_bg: Color32,
    pub sel_fg: Color32,
    pub sel_match_fg: Color32,
    pub sel_dim: Color32,
    pub sel_outline: Option<Color32>,
    /// Disc / square colors for initials, picked by a hash of the name.
    pub avatars: &'static [Color32],
    pub avatar_fg: Color32,
    /// Small pill backgrounds ("group", "3", the All/People/Groups chips).
    pub chip_bg: Color32,
    pub chip_fg: Color32,
}

/// Every measurement any theme lays out with, in logical pixels.
pub struct Metrics {
    pub header_h: f32,
    pub footer_h: f32,
    pub window_radius: f32,
    pub border_width: f32,
    pub row_h: f32,
    pub row_gap: f32,
    pub row_pad_x: f32,
    pub row_radius: f32,
    /// Padding around the whole list block.
    pub list_pad_x: f32,
    pub list_pad_y: f32,
    pub side_pad: f32,
    pub query_size: f32,
    pub name_size: f32,
    pub meta_size: f32,
    pub caret_w: f32,
    pub caret_h: f32,
    pub avatar: f32,
    /// A transparent window lets us paint our own rounded corners.
    pub transparent: bool,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const SPOTLIGHT_AVATARS: &[Color32] = &[rgb(0x2f6fed)];
const EDITORIAL_AVATARS: &[Color32] = &[
    rgb(0xd8a35a),
    rgb(0xc7c2b8),
    rgb(0xa9b8a6),
    rgb(0xb7a9c9),
    rgb(0xc9a58f),
];
const RICH_AVATARS: &[Color32] = &[
    rgb(0x2f6fed),
    rgb(0x7a5af5),
    rgb(0xe0655a),
    rgb(0x2c9a6b),
    rgb(0x3a4160),
];

impl Theme {
    pub fn palette(self) -> Palette {
        match self {
            Theme::Spotlight => Palette {
                window_bg: Color32::from_rgba_unmultiplied(0x1c, 0x1d, 0x23, 235),
                window_border: Color32::from_rgba_unmultiplied(0xff, 0xff, 0xff, 20),
                header_bg: None,
                rule: Color32::from_rgba_unmultiplied(0xff, 0xff, 0xff, 18),
                text: rgb(0xe6e7ea),
                dim: rgb(0x7c8190),
                accent: rgb(0x2f6fed),
                match_fg: Color32::WHITE,
                row_hover: rgb(0x24262e),
                sel_bg: rgb(0x2f6fed),
                sel_fg: Color32::WHITE,
                sel_match_fg: Color32::WHITE,
                sel_dim: Color32::from_rgba_unmultiplied(0xff, 0xff, 0xff, 191),
                sel_outline: None,
                avatars: SPOTLIGHT_AVATARS,
                avatar_fg: Color32::WHITE,
                chip_bg: Color32::TRANSPARENT,
                chip_fg: rgb(0x6c7180),
            },
            Theme::Terminal => Palette {
                window_bg: rgb(0x0d1117),
                window_border: rgb(0x30363d),
                header_bg: None,
                rule: rgb(0x30363d),
                text: rgb(0xb9c0cf),
                dim: rgb(0x8b949e),
                accent: rgb(0x58a6ff),
                match_fg: rgb(0x58a6ff),
                row_hover: rgb(0x161b26),
                sel_bg: rgb(0x1c2230),
                sel_fg: rgb(0xf2f4f8),
                sel_match_fg: rgb(0x58a6ff),
                sel_dim: rgb(0x8b949e),
                sel_outline: None,
                avatars: SPOTLIGHT_AVATARS,
                avatar_fg: Color32::WHITE,
                chip_bg: Color32::TRANSPARENT,
                chip_fg: rgb(0x8b949e),
            },
            Theme::Editorial => Palette {
                window_bg: rgb(0x1a1714),
                window_border: Color32::TRANSPARENT,
                header_bg: None,
                rule: rgb(0x33291f),
                text: rgb(0xf3ede4),
                dim: rgb(0x9a8f82),
                accent: rgb(0xd8a35a),
                match_fg: rgb(0xd8a35a),
                row_hover: rgb(0x221d19),
                sel_bg: rgb(0x2a2420),
                sel_fg: rgb(0xf3ede4),
                sel_match_fg: rgb(0xd8a35a),
                sel_dim: rgb(0x9a8f82),
                sel_outline: None,
                avatars: EDITORIAL_AVATARS,
                avatar_fg: rgb(0x1a1714),
                chip_bg: Color32::TRANSPARENT,
                chip_fg: rgb(0x9a8f82),
            },
            Theme::Brutalist => Palette {
                window_bg: rgb(0x0a0a0a),
                window_border: rgb(0xe6ff2a),
                header_bg: None,
                rule: rgb(0xe6ff2a),
                text: rgb(0xf5f5f5),
                dim: rgb(0x7a7a7a),
                accent: rgb(0xe6ff2a),
                match_fg: rgb(0xe6ff2a),
                row_hover: rgb(0x1a1a12),
                sel_bg: rgb(0xe6ff2a),
                sel_fg: rgb(0x0a0a0a),
                sel_match_fg: rgb(0x0a0a0a),
                sel_dim: rgb(0x0a0a0a),
                sel_outline: None,
                avatars: SPOTLIGHT_AVATARS,
                avatar_fg: Color32::WHITE,
                chip_bg: Color32::TRANSPARENT,
                chip_fg: rgb(0x7a7a7a),
            },
            Theme::Rich => Palette {
                window_bg: rgb(0x12151d),
                window_border: rgb(0x262b3a),
                header_bg: Some(rgb(0x171b26)),
                rule: rgb(0x262b3a),
                text: rgb(0xeef0f5),
                dim: rgb(0x8a92a8),
                accent: rgb(0x2f6fed),
                match_fg: rgb(0x7aa2ff),
                row_hover: rgb(0x1a1f2e),
                sel_bg: rgb(0x1f2a44),
                sel_fg: rgb(0xeef0f5),
                sel_match_fg: rgb(0x7aa2ff),
                sel_dim: rgb(0x8a92a8),
                sel_outline: Some(rgb(0x2f6fed)),
                avatars: RICH_AVATARS,
                avatar_fg: Color32::WHITE,
                chip_bg: rgb(0x232838),
                chip_fg: rgb(0xaab2c8),
            },
        }
    }

    pub fn metrics(self) -> Metrics {
        match self {
            Theme::Spotlight => Metrics {
                header_h: 60.0,
                footer_h: 34.0,
                window_radius: 14.0,
                border_width: 1.0,
                row_h: 36.0,
                row_gap: 2.0,
                row_pad_x: 12.0,
                row_radius: 8.0,
                list_pad_x: 10.0,
                list_pad_y: 10.0,
                side_pad: 18.0,
                query_size: 24.0,
                name_size: 15.0,
                meta_size: 12.0,
                caret_w: 2.0,
                caret_h: 26.0,
                avatar: 0.0,
                transparent: true,
            },
            Theme::Terminal => Metrics {
                header_h: 0.0,
                footer_h: 56.0,
                window_radius: 0.0,
                border_width: 1.0,
                row_h: 26.0,
                row_gap: 0.0,
                row_pad_x: 8.0,
                row_radius: 0.0,
                list_pad_x: 8.0,
                list_pad_y: 10.0,
                side_pad: 8.0,
                query_size: 15.0,
                name_size: 14.0,
                meta_size: 14.0,
                caret_w: 9.0,
                caret_h: 18.0,
                avatar: 0.0,
                transparent: false,
            },
            Theme::Editorial => Metrics {
                header_h: 68.0,
                footer_h: 0.0,
                window_radius: 18.0,
                border_width: 0.0,
                row_h: 52.0,
                row_gap: 2.0,
                row_pad_x: 16.0,
                row_radius: 10.0,
                list_pad_x: 12.0,
                list_pad_y: 10.0,
                side_pad: 26.0,
                query_size: 34.0,
                name_size: 21.0,
                meta_size: 12.0,
                caret_w: 2.0,
                caret_h: 30.0,
                avatar: 34.0,
                transparent: true,
            },
            Theme::Brutalist => Metrics {
                header_h: 74.0,
                footer_h: 0.0,
                window_radius: 0.0,
                border_width: 2.0,
                row_h: 44.0,
                row_gap: 0.0,
                row_pad_x: 16.0,
                row_radius: 0.0,
                list_pad_x: 0.0,
                list_pad_y: 0.0,
                side_pad: 16.0,
                query_size: 44.0,
                name_size: 22.0,
                meta_size: 11.0,
                caret_w: 14.0,
                caret_h: 40.0,
                avatar: 0.0,
                transparent: false,
            },
            Theme::Rich => Metrics {
                header_h: 54.0,
                footer_h: 0.0,
                window_radius: 12.0,
                border_width: 1.0,
                row_h: 54.0,
                row_gap: 2.0,
                row_pad_x: 12.0,
                row_radius: 10.0,
                list_pad_x: 8.0,
                list_pad_y: 8.0,
                side_pad: 16.0,
                query_size: 18.0,
                name_size: 15.0,
                meta_size: 12.0,
                caret_w: 2.0,
                caret_h: 20.0,
                avatar: 36.0,
                transparent: true,
            },
        }
    }
}

// Font family handles. Spotlight deliberately uses egui's own proportional
// font, which on macOS is the closest thing we have to the system UI face.
pub fn fam(name: &'static str) -> FontFamily {
    FontFamily::Name(Arc::from(name))
}

impl Theme {
    /// Regular body face.
    pub fn font(self, size: f32) -> FontId {
        FontId::new(size, self.family())
    }

    pub fn family(self) -> FontFamily {
        match self {
            Theme::Spotlight => FontFamily::Proportional,
            Theme::Terminal => fam("jetbrains"),
            Theme::Editorial => fam("iserif"),
            Theme::Brutalist => fam("grotesk"),
            Theme::Rich => fam("plex"),
        }
    }

    /// The heavier face used for names and matched letters. Serif themes have
    /// no bold cut, so they return the same family and lean on color instead.
    pub fn family_strong(self) -> FontFamily {
        match self {
            Theme::Spotlight => FontFamily::Proportional,
            Theme::Terminal => fam("jetbrains-bold"),
            Theme::Editorial => fam("iserif"),
            Theme::Brutalist => fam("grotesk-bold"),
            Theme::Rich => fam("plex-sb"),
        }
    }

    /// The small sans face for labels and subtitles. Only the serif theme
    /// needs a different family here.
    pub fn family_meta(self) -> FontFamily {
        match self {
            Theme::Editorial => fam("isans"),
            _ => self.family(),
        }
    }

    pub fn family_meta_strong(self) -> FontFamily {
        match self {
            Theme::Editorial => fam("isans-sb"),
            _ => self.family_strong(),
        }
    }

    /// The face the query line is typed in.
    pub fn family_query(self) -> FontFamily {
        match self {
            Theme::Editorial => fam("iserif-italic"),
            Theme::Brutalist => fam("grotesk-bold"),
            _ => self.family(),
        }
    }
}

/// Load the bundled OFL faces. Called once, at startup: rebuilding these per
/// frame would re-rasterize every glyph atlas.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let faces: [(&str, &[u8]); 10] = [
        ("jetbrains", include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")),
        ("jetbrains-bold", include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf")),
        ("iserif", include_bytes!("../assets/fonts/InstrumentSerif-Regular.ttf")),
        ("iserif-italic", include_bytes!("../assets/fonts/InstrumentSerif-Italic.ttf")),
        ("isans", include_bytes!("../assets/fonts/InstrumentSans-Regular.ttf")),
        ("isans-sb", include_bytes!("../assets/fonts/InstrumentSans-SemiBold.ttf")),
        ("grotesk", include_bytes!("../assets/fonts/SpaceGrotesk-Medium.ttf")),
        ("grotesk-bold", include_bytes!("../assets/fonts/SpaceGrotesk-Bold.ttf")),
        ("plex", include_bytes!("../assets/fonts/IBMPlexSans-Regular.ttf")),
        ("plex-sb", include_bytes!("../assets/fonts/IBMPlexSans-SemiBold.ttf")),
    ];
    for (name, bytes) in faces {
        fonts
            .font_data
            .insert(name.to_owned(), egui::FontData::from_static(bytes));
        // Each face is its own family so a theme can ask for one cut exactly;
        // egui's defaults trail behind for glyphs the face lacks, such as the
        // emoji in a group chat's name.
        fonts.families.insert(
            fam(name),
            vec![
                name.to_owned(),
                "Ubuntu-Light".to_owned(),
                "NotoEmoji-Regular".to_owned(),
                "emoji-icon-font".to_owned(),
            ],
        );
    }
    ctx.set_fonts(fonts);
}

/// A stable index into a theme's avatar palette, so a person keeps their color
/// between popups. FNV-1a over the bytes of the name.
pub fn avatar_index(name: &str, len: usize) -> usize {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (hash % len.max(1) as u64) as usize
}

/// First letters of the first two words: `Grace Hopper` -> `GH`, `Ada` -> `A`.
pub fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .flat_map(|c| c.to_uppercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn parses_every_name_and_rejects_others() {
        for (i, name) in THEME_NAMES.iter().enumerate() {
            let t = Theme::from_str(name).unwrap();
            assert_eq!(t.to_string(), *name);
            assert_eq!(t as usize, i);
        }
        assert_eq!(Theme::from_str(" SPOTLIGHT ").unwrap(), Theme::Spotlight);
        let err = Theme::from_str("neon").unwrap_err();
        for name in THEME_NAMES {
            assert!(err.contains(name), "the error should list {name}: {err}");
        }
    }

    #[test]
    fn initials_take_the_first_two_words() {
        assert_eq!(initials("Grace Hopper"), "GH");
        assert_eq!(initials("Ada Lovelace, Grace Hopper, Alan Turing"), "AL");
        assert_eq!(initials("Ada"), "A");
        assert_eq!(initials(""), "");
        assert_eq!(initials("+15550100001"), "1");
        assert_eq!(initials("🎿 Ski Trip"), "ST", "a leading emoji is not a letter");
    }

    #[test]
    fn avatar_colors_are_stable_and_in_range() {
        let a = avatar_index("Grace Hopper", 5);
        assert_eq!(a, avatar_index("Grace Hopper", 5));
        assert!(a < 5);
        assert!(avatar_index("anything", 1) == 0);
    }
}
