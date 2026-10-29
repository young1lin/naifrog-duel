//! The game's two languages, and the characters its canvas text can draw.
//! Canvas text is Bevy's built-in Fira Mono, falling back to the name font
//! (`assets/processed/name_font.otf`) for Han characters and Chinese punctuation; the page
//! renders its own text. The browser build takes the page's language (web/main.js: the
//! visitor's choice, else the browser's language); the desktop takes `--lang`,
//! `NAIFROG_LANG` or the system locale, and F2 switches.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    /// Simplified Chinese.
    #[default]
    Zh,
    En,
}

impl Lang {
    /// A language tag the way browsers, `LANG` and `--lang` write it: `zh`, `zh-CN`,
    /// `zh_CN.UTF-8`, `zh-Hans-CN`, `en`, `en-US`. Other languages are `None`; callers show
    /// English for them.
    pub fn from_tag(tag: &str) -> Option<Self> {
        let primary = tag.trim().split(['-', '_', '.', '@']).next()?;
        if primary.eq_ignore_ascii_case("zh") {
            Some(Self::Zh)
        } else if primary.eq_ignore_ascii_case("en") {
            Some(Self::En)
        } else {
            None
        }
    }
    pub fn code(self) -> &'static str {
        self.pick("zh", "en")
    }
    pub fn other(self) -> Self {
        self.pick(Self::En, Self::Zh)
    }
    /// The Chinese or the English of one message.
    pub fn pick<T>(self, zh: T, en: T) -> T {
        match self {
            Self::Zh => zh,
            Self::En => en,
        }
    }
}

/// The 8105 Han characters of 通用规范汉字表 (Unihan `kTGH`): what the name font draws, and
/// what player names may use (`net::valid_name`). Written by scripts/prepare-name-font.mjs;
/// the line breaks are not characters of the list.
pub const HAN: &str = include_str!("name_han.txt");

/// What else the name font draws besides ASCII and `HAN`: quotes, dashes, the ellipsis,
/// arrows, CJK punctuation and full-width forms. scripts/prepare-name-font.mjs cuts the font
/// to exactly these ranges and records them in name_font.json (a test compares).
pub const FONT_EXTRA: [(char, char); 9] = [
    ('\u{00B7}', '\u{00B7}'),
    ('\u{00D7}', '\u{00D7}'),
    ('\u{2013}', '\u{2014}'),
    ('\u{2018}', '\u{2019}'),
    ('\u{201C}', '\u{201D}'),
    ('\u{2026}', '\u{2026}'),
    ('\u{2190}', '\u{2193}'),
    ('\u{3000}', '\u{303F}'),
    ('\u{FF01}', '\u{FF5E}'),
];

/// Whether the canvas can draw `c`: printable ASCII (Fira Mono, and the name font too), or
/// a character of the name font.
pub fn drawn(c: char) -> bool {
    matches!(c, ' '..='~')
        || (!c.is_ascii() && HAN.contains(c))
        || FONT_EXTRA.iter().any(|(lo, hi)| (*lo..=*hi).contains(&c))
}

/// Width in em: Fira Mono draws everything below the CJK blocks 0.6 em wide, and the name
/// font's Han characters and full-width punctuation are 1 em.
pub fn em_width(text: &str) -> f32 {
    text.chars()
        .map(|c| if c < '\u{2E80}' { 0.6 } else { 1.0 })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_tags_pick_chinese_or_english() {
        for tag in ["zh", "zh-CN", "ZH_cn", "zh_CN.UTF-8", "zh-Hans-CN", "zh-TW"] {
            assert_eq!(Lang::from_tag(tag), Some(Lang::Zh), "{tag}");
        }
        for tag in ["en", "en-US", "en_GB.UTF-8", " EN "] {
            assert_eq!(Lang::from_tag(tag), Some(Lang::En), "{tag}");
        }
        for tag in ["", "fr-FR", "ja", "C", "POSIX", "zhx", "english"] {
            assert_eq!(Lang::from_tag(tag), None, "{tag}");
        }
        assert_eq!(Lang::Zh.other(), Lang::En);
        assert_eq!(Lang::En.other().code(), "zh");
    }
    #[test]
    fn the_han_list_is_the_8105_common_characters() {
        // Line breaks are LF or, after a CRLF checkout, CR LF.
        let han: Vec<char> = HAN.chars().filter(|c| !c.is_ascii_whitespace()).collect();
        assert_eq!(han.len(), 8105);
        assert!(han.windows(2).all(|w| w[0] < w[1]), "sorted and unique");
        assert!(han.iter().all(|c| !c.is_ascii()));
        assert!(han.iter().all(|c| drawn(*c)));
    }
    #[test]
    fn the_font_record_lists_the_same_extra_ranges() {
        let spec = FONT_EXTRA
            .iter()
            .map(|(lo, hi)| {
                if lo == hi {
                    format!("U+{:04X}", *lo as u32)
                } else {
                    format!("U+{:04X}..U+{:04X}", *lo as u32, *hi as u32)
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let record = include_str!("../assets/processed/name_font.json");
        assert!(
            record.contains(&format!("\"extra\": \"{spec}\"")),
            "name_font.json must list {spec}; rerun scripts/prepare-name-font.mjs"
        );
    }
    #[test]
    fn drawable_and_widths() {
        assert!("奶蛙对打：第一局！（P1）……“好”".chars().all(drawn));
        assert!(!drawn('\n') && !drawn('龘') && !drawn('😀') && !drawn('é'));
        assert_eq!(em_width("P1"), 1.2);
        assert_eq!(em_width("你好！"), 3.0);
        assert!(
            (em_width("P1 · 你") - 4.0).abs() < 1e-6,
            "five 0.6 em, one 1 em"
        );
    }
}
