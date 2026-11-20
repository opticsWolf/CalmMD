//! Curated Hunspell dictionary catalogue.
//!
//! Bundled dictionaries are compiled into the binary and work offline. Optional dictionaries are
//! downloaded from a fixed, permissively licensed source only when the user asks for them.

use serde::Serialize;

pub const WOOORM: &str = "https://raw.githubusercontent.com/wooorm/dictionaries/main/dictionaries";
/// Generated dictionaries are downloaded from the repository assets, never compiled into the app.
pub const GENERATED: &str = "https://raw.githubusercontent.com/opticsWolf/CalmMD/dev_rust/crates/calmmd-core/src/core/spell/dictionaries";

#[derive(Debug, Clone, Copy)]
pub struct DictionaryEntry {
    pub id: &'static str,
    pub label: &'static str,
    /// BCP 47 tags this dictionary answers, first tag is the canonical one.
    pub languages: &'static [&'static str],
    pub license: &'static str,
    /// Base URL used only for downloadable dictionaries.
    pub base: &'static str,
    /// Upstream directory; empty for generated dictionaries stored under `id`.
    pub upstream: &'static str,
    pub bundled: bool,
}

pub const DICTIONARIES: &[DictionaryEntry] = &[
    DictionaryEntry {
        id: "en_US",
        label: "English (US)",
        languages: &["en-US", "en"],
        license: "MIT AND BSD",
        base: WOOORM,
        upstream: "en",
        bundled: true,
    },
    DictionaryEntry {
        id: "en_GB",
        label: "English (UK)",
        languages: &["en-GB"],
        license: "MIT AND BSD",
        base: WOOORM,
        upstream: "en-GB",
        bundled: true,
    },
    DictionaryEntry {
        id: "de_DE",
        label: "German",
        languages: &["de-DE", "de"],
        license: "Apache-2.0",
        base: GENERATED,
        upstream: "",
        bundled: true,
    },
    DictionaryEntry {
        id: "fr_FR",
        label: "French",
        languages: &["fr-FR", "fr-BE", "fr"],
        license: "Apache-2.0",
        base: GENERATED,
        upstream: "",
        bundled: true,
    },
    DictionaryEntry {
        id: "es_ES",
        label: "Spanish",
        languages: &["es-ES", "es"],
        license: "Apache-2.0",
        base: GENERATED,
        upstream: "",
        bundled: true,
    },
    DictionaryEntry {
        id: "it_IT",
        label: "Italian",
        languages: &["it-IT", "it"],
        license: "Apache-2.0",
        base: GENERATED,
        upstream: "",
        bundled: false,
    },
    DictionaryEntry {
        id: "pt_PT",
        label: "Portuguese",
        languages: &["pt-PT", "pt-BR", "pt"],
        license: "Apache-2.0",
        base: GENERATED,
        upstream: "",
        bundled: false,
    },
    DictionaryEntry {
        id: "nl_NL",
        label: "Dutch / Flemish",
        languages: &["nl-NL", "nl-BE", "nl"],
        license: "Apache-2.0",
        base: GENERATED,
        upstream: "",
        bundled: false,
    },
    DictionaryEntry {
        id: "fi_FI",
        label: "Finnish",
        languages: &["fi-FI", "fi"],
        license: "Apache-2.0",
        base: GENERATED,
        upstream: "",
        bundled: false,
    },
    DictionaryEntry {
        id: "nb_NO",
        label: "Norwegian (Bokmål)",
        languages: &["nb-NO", "nb", "no"],
        license: "Apache-2.0",
        base: GENERATED,
        upstream: "",
        bundled: false,
    },
    DictionaryEntry {
        id: "da_DK",
        label: "Danish",
        languages: &["da-DK", "da"],
        license: "Apache-2.0",
        base: GENERATED,
        upstream: "",
        bundled: false,
    },
    DictionaryEntry {
        id: "pl_PL",
        label: "Polish",
        languages: &["pl-PL", "pl"],
        license: "Apache-2.0",
        base: GENERATED,
        upstream: "",
        bundled: false,
    },
    DictionaryEntry {
        id: "zh_CN",
        label: "Chinese",
        languages: &["zh-CN", "zh"],
        license: "Apache-2.0",
        base: GENERATED,
        upstream: "",
        bundled: false,
    },
    DictionaryEntry {
        id: "ja_JP",
        label: "Japanese",
        languages: &["ja-JP", "ja"],
        license: "Apache-2.0",
        base: GENERATED,
        upstream: "",
        bundled: false,
    },
    DictionaryEntry {
        id: "ko_KR",
        label: "Korean",
        languages: &["ko-KR", "ko"],
        license: "Apache-2.0",
        base: GENERATED,
        upstream: "",
        bundled: false,
    },
    DictionaryEntry {
        id: "ru_RU",
        label: "Russian",
        languages: &["ru-RU", "ru"],
        license: "BSD-3-Clause",
        base: WOOORM,
        upstream: "ru",
        bundled: false,
    },
    DictionaryEntry {
        id: "en_CA",
        label: "English (Canada)",
        languages: &["en-CA"],
        license: "MIT AND BSD",
        base: WOOORM,
        upstream: "en-CA",
        bundled: false,
    },
    DictionaryEntry {
        id: "en_AU",
        label: "English (Australia)",
        languages: &["en-AU"],
        license: "MIT AND BSD",
        base: WOOORM,
        upstream: "en-AU",
        bundled: false,
    },
    DictionaryEntry {
        id: "tr_TR",
        label: "Turkish",
        languages: &["tr-TR", "tr"],
        license: "MIT",
        base: WOOORM,
        upstream: "tr",
        bundled: false,
    },
];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryInfo {
    pub id: String,
    pub label: String,
    pub languages: Vec<String>,
    pub license: String,
    pub installed: bool,
    pub bundled: bool,
    pub source: String,
}

pub fn by_id(id: &str) -> Option<&'static DictionaryEntry> {
    DICTIONARIES.iter().find(|entry| entry.id == id)
}

/// Directory path under `base` for a downloadable dictionary.
pub fn remote_dir(entry: &DictionaryEntry) -> &str {
    if entry.upstream.is_empty() { entry.id } else { entry.upstream }
}

pub fn normalize_lang(lang: Option<&str>) -> String {
    lang.unwrap_or("en-US").trim().to_ascii_lowercase().replace('_', "-")
}

/// Exact tag, then primary language; unknown languages do not silently use English.
pub fn entry_for_lang(lang: Option<&str>) -> Option<&'static DictionaryEntry> {
    let tag = normalize_lang(lang);
    if let Some(entry) = DICTIONARIES.iter().find(|e| e.languages.iter().any(|l| l.to_ascii_lowercase() == tag)) {
        return Some(entry);
    }
    let primary = tag.split('-').next().unwrap_or("");
    DICTIONARIES
        .iter()
        .find(|e| e.languages.iter().any(|l| l.split('-').next().unwrap_or("") == primary && !l.contains('-')))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_language_tags_to_curated_dictionaries() {
        assert_eq!(entry_for_lang(None).unwrap().id, "en_US");
        assert_eq!(entry_for_lang(Some("en")).unwrap().id, "en_US");
        assert_eq!(entry_for_lang(Some("en_GB")).unwrap().id, "en_GB");
        assert_eq!(entry_for_lang(Some("de-DE")).unwrap().id, "de_DE");
        assert_eq!(entry_for_lang(Some("fr")).unwrap().id, "fr_FR");
        assert_eq!(entry_for_lang(Some("es-ES")).unwrap().id, "es_ES");
        assert_eq!(entry_for_lang(Some("it-IT")).unwrap().id, "it_IT");
        assert_eq!(entry_for_lang(Some("pt-BR")).unwrap().id, "pt_PT");
        assert_eq!(entry_for_lang(Some("nl-BE")).unwrap().id, "nl_NL");
        assert_eq!(entry_for_lang(Some("fi-FI")).unwrap().id, "fi_FI");
        assert_eq!(entry_for_lang(Some("nb-NO")).unwrap().id, "nb_NO");
        assert_eq!(entry_for_lang(Some("da-DK")).unwrap().id, "da_DK");
        assert_eq!(entry_for_lang(Some("pl-PL")).unwrap().id, "pl_PL");
        assert_eq!(entry_for_lang(Some("zh-CN")).unwrap().id, "zh_CN");
        assert_eq!(entry_for_lang(Some("ja-JP")).unwrap().id, "ja_JP");
        assert_eq!(entry_for_lang(Some("ko-KR")).unwrap().id, "ko_KR");
        assert_eq!(entry_for_lang(Some("ru-RU")).unwrap().id, "ru_RU");
    }

    #[test]
    fn generated_languages_are_optional_not_bundled() {
        for id in [
            "it_IT", "pt_PT", "nl_NL", "fi_FI", "nb_NO", "da_DK", "pl_PL", "zh_CN", "ja_JP", "ko_KR",
        ] {
            let entry = by_id(id).expect("catalogue entry");
            assert!(!entry.bundled, "{id} must not be bundled");
            assert_eq!(entry.base, GENERATED);
        }
    }

    #[test]
    fn catalogue_uses_only_permissive_licences() {
        for entry in DICTIONARIES {
            assert!(
                entry.license.contains("MIT")
                    || entry.license.contains("BSD")
                    || entry.license.contains("Apache")
                    || entry.license.contains("ISC"),
                "{} uses {}",
                entry.id,
                entry.license
            );
        }
    }
}
