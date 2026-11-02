//! The families that stand behind every chosen font, by the script of the
//! locale.
//!
//! The list is of names, and the names are looked for among the installed
//! families: a machine has a few of them, and which few depends on its system.
//! The script of the locale stands first, because the scripts of China, Japan
//! and Korea share code points and draw them apart. The other scripts follow,
//! so a terminal draws what a device prints in any of them.

/// Families that carry the Latin alphabet.
const LATIN: &[&str] = &[
    "Noto Sans",
    "Segoe UI",
    "Arial",
    "DejaVu Sans",
    "Liberation Sans",
    "Roboto",
];

/// Families that carry the Cyrillic alphabet.
const CYRILLIC: &[&str] = &["Noto Sans", "DejaVu Sans", "Segoe UI", "Arial", "Tahoma"];

/// Families that carry Hangul.
const KOREAN: &[&str] = &[
    "Noto Sans KR",
    "Noto Sans CJK KR",
    "Malgun Gothic",
    "NanumGothic",
];

/// Families that draw the Han characters as they are written in mainland China.
const SIMPLIFIED_CHINESE: &[&str] = &[
    "Noto Sans SC",
    "Noto Sans CJK SC",
    "Microsoft YaHei",
    "SimHei",
    "SimSun",
];

/// Families that draw the Han characters as they are written in Taiwan and Hong
/// Kong.
const TRADITIONAL_CHINESE: &[&str] = &["Noto Sans TC", "Noto Sans CJK TC", "Microsoft JhengHei"];

/// Families that carry kana and draw the Han characters as they are written in
/// Japan.
const JAPANESE: &[&str] = &["Noto Sans JP", "Noto Sans CJK JP", "Yu Gothic", "Meiryo"];

/// Every script, in the order they follow the script of the locale.
const SCRIPTS: &[&[&str]] = &[
    LATIN,
    CYRILLIC,
    KOREAN,
    SIMPLIFIED_CHINESE,
    TRADITIONAL_CHINESE,
    JAPANESE,
];

/// Languages written in the Cyrillic alphabet.
const CYRILLIC_LANGUAGES: &[&str] = &["ru", "uk", "be", "bg", "sr", "mk", "kk", "ky", "mn", "tg"];

/// Regions and scripts of a Chinese locale that write the traditional
/// characters.
const TRADITIONAL_TAGS: &[&str] = &["tw", "hk", "mo", "hant"];

/// The families to look for behind the chosen ones, for a locale such as
/// `ja-JP` or `zh_TW.UTF-8`, each family once.
pub fn families(locale: &str) -> Vec<&'static str> {
    let mut names: Vec<&'static str> = Vec::new();
    for script in std::iter::once(script(locale)).chain(SCRIPTS.iter().copied()) {
        for name in script {
            if !names.contains(name) {
                names.push(name);
            }
        }
    }
    names
}

/// The families of the script a locale is written in.
fn script(locale: &str) -> &'static [&'static str] {
    let locale = locale.to_ascii_lowercase();
    let mut parts = locale.split(['-', '_', '.', '@']);
    let language = parts.next().unwrap_or_default();
    match language {
        "ko" => KOREAN,
        "ja" => JAPANESE,
        "zh" if parts.any(|part| TRADITIONAL_TAGS.contains(&part)) => TRADITIONAL_CHINESE,
        "zh" => SIMPLIFIED_CHINESE,
        _ if CYRILLIC_LANGUAGES.contains(&language) => CYRILLIC,
        _ => LATIN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The script of the locale leads, and every other script follows it, so
    /// Japanese draws a Han character in a Japanese face and still has Hangul.
    #[test]
    fn the_script_of_the_locale_leads() {
        let japanese = families("ja_JP.UTF-8");
        assert_eq!(japanese.first(), Some(&"Noto Sans JP"));
        assert!(japanese.contains(&"Noto Sans KR"));

        assert_eq!(families("zh-Hant-TW").first(), Some(&"Noto Sans TC"));
        assert_eq!(families("zh_CN").first(), Some(&"Noto Sans SC"));
        assert_eq!(families("ru-RU").first(), Some(&"Noto Sans"));
        assert_eq!(families("").first(), Some(&"Noto Sans"));
    }

    /// A family two scripts share is looked for once.
    #[test]
    fn a_family_is_named_once() {
        let names = families("ru_RU.UTF-8");
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
    }
}
