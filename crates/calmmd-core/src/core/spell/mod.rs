//! Native spellchecking service backed by zspell and permissive Hunspell dictionaries.
//!
//! The OS checker in `spell.rs` stays as a fallback for languages without a bundled or locally
//! installed dictionary. Dictionaries are opt-in: bundled ones work offline, and extra ones are
//! downloaded only when the user asks for them.

mod catalog;
mod dictionary;
mod utf16;

use std::fs;
use std::io::ErrorKind;
use std::sync::OnceLock;
use std::time::Duration;

use serde::Serialize;
use crate::app::AppContext;

pub use catalog::{entry_for_lang, DictionaryInfo};
use dictionary::{is_installed, parts_for};

#[derive(Serialize, Debug, PartialEq)]
pub struct SpellCheckDto {
    pub misspelled: bool,
    pub suggestions: Vec<String>,
}

const MAX_SUGGESTIONS: usize = 6;
const MAX_DICTIONARY_BYTES: usize = 24 * 1024 * 1024;

static CACHE: OnceLock<DictionaryCache> = OnceLock::new();

use dictionary::DictionaryCache;

fn cache() -> &'static DictionaryCache {
    CACHE.get_or_init(DictionaryCache::default)
}

fn personal_key(id: &str) -> String {
    format!("spell/personal/{id}")
}

fn read_personal(app: &AppContext, id: &str) -> Result<String, String> {
    Ok(crate::kv::read(app, &personal_key(id))?.unwrap_or_default())
}

/// Adds a word to a Hunspell personal dictionary, keeping the file stable and duplicate-free.
fn add_personal_word(current: &str, word: &str) -> String {
    let word = word.trim();
    if word.is_empty() || !word.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '\'' | '’')) {
        return current.to_string();
    }
    let mut words: Vec<String> = current
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect();
    // zspell checks the exact word and its lowercase form; storing lowercase makes a learned
    // capitalised word accepted in any case, matching the old OS dictionary behaviour.
    let word = word.to_lowercase();
    if !words.iter().any(|w| w.eq_ignore_ascii_case(&word)) {
        words.push(word);
    }
    words.sort();
    words.dedup();
    words.join("\n")
}

/// Resolves a language to a dictionary that is actually available on this machine.
fn available_id(app: &AppContext, lang: Option<&str>) -> Option<&'static str> {
    let entry = entry_for_lang(lang)?;
    if entry.bundled || is_installed(app, entry.id) {
        Some(entry.id)
    } else {
        None
    }
}

fn check_with_parts(word: &str, id: &str, aff: &str, dic: &str, personal_words: &str) -> Result<Option<SpellCheckDto>, String> {
    let word = word.trim();
    if word.is_empty() {
        return Ok(None);
    }
    let dict = cache().get(id, aff, dic, personal_words)?;
    let misspelled = !dict.check_word(word);
    let suggestions = if misspelled {
        dict.entry(word)
            .suggest()
            .unwrap_or_default()
            .into_iter()
            .filter(|s| *s != word)
            .take(MAX_SUGGESTIONS)
            .map(str::to_string)
            .collect()
    } else {
        Vec::new()
    };
    Ok(Some(SpellCheckDto { misspelled, suggestions }))
}

fn check_text_with_parts(text: &str, id: &str, aff: &str, dic: &str, personal_words: &str) -> Result<Option<Vec<(u32, u32)>>, String> {
    let dict = cache().get(id, aff, dic, personal_words)?;
    let mut ranges = Vec::new();
    for (start, word) in dict.check_indices(text) {
        let end = start + word.len();
        ranges.push((utf16::byte_to_utf16(text, start), utf16::byte_to_utf16(text, end)));
    }
    Ok(Some(ranges))
}

fn check_word_with_ctx(app: &AppContext, word: &str, lang: Option<&str>) -> Result<Option<SpellCheckDto>, String> {
    let Some(id) = available_id(app, lang) else { return Ok(None) };
    let (aff, dic) = parts_for(app, id)?.expect("available dictionary has files");
    let personal = read_personal(app, id)?;
    check_with_parts(word, id, &aff, &dic, &personal)
}

fn check_text_with_ctx(app: &AppContext, text: &str, lang: Option<&str>) -> Result<Option<Vec<(u32, u32)>>, String> {
    let Some(id) = available_id(app, lang) else { return Ok(None) };
    let (aff, dic) = parts_for(app, id)?.expect("available dictionary has files");
    let personal = read_personal(app, id)?;
    check_text_with_parts(text, id, &aff, &dic, &personal)
}

pub fn check(app: &AppContext, word: &str, lang: Option<&str>) -> Result<Option<SpellCheckDto>, String> {
    check_word_with_ctx(app, word, lang)
}

pub fn check_text(app: &AppContext, text: &str, lang: Option<&str>) -> Result<Option<Vec<(u32, u32)>>, String> {
    check_text_with_ctx(app, text, lang)
}

/// Returns `true` when the Rust service handled the word; `false` asks the webview to fall back
/// to the OS user dictionary (unknown language).
pub fn learn(app: &AppContext, word: &str, lang: Option<&str>) -> Result<bool, String> {
    let Some(id) = available_id(app, lang) else { return Ok(false) };
    let word = word.trim();
    if word.is_empty() || !word.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '\'' | '’')) {
        return Ok(true);
    }
    let current = read_personal(app, id)?;
    let updated = add_personal_word(&current, word);
    if updated != current {
        crate::kv::write(app, &personal_key(id), &updated)?;
        cache().clear();
    }
    Ok(true)
}

pub fn dictionaries(app: &AppContext) -> Result<Vec<catalog::DictionaryInfo>, String> {
    Ok(catalog::DICTIONARIES
        .iter()
        .map(|entry| catalog::DictionaryInfo {
            id: entry.id.to_string(),
            label: entry.label.to_string(),
            languages: entry.languages.iter().map(|tag| tag.to_string()).collect(),
            license: entry.license.to_string(),
            installed: entry.bundled || is_installed(app, entry.id),
            bundled: entry.bundled,
            source: format!("{}/{}", entry.base, catalog::remote_dir(entry)),
        })
        .collect())
}

async fn download(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
    if parsed.scheme() != "https" || parsed.host_str() != Some("raw.githubusercontent.com") {
        return Err(format!("refusing to download from {url}"));
    }
    let response = client
        .get(parsed)
        .send()
        .await
        .map_err(|e| format!("dictionary download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("dictionary download failed: {e}"))?;
    if let Some(length) = response.content_length() {
        if length as usize > MAX_DICTIONARY_BYTES {
            return Err("dictionary file is too large".into());
        }
    }
    let bytes = response.bytes().await.map_err(|e| format!("dictionary download failed: {e}"))?;
    if bytes.is_empty() || bytes.len() > MAX_DICTIONARY_BYTES {
        return Err("dictionary file has an unexpected size".into());
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| "dictionary file is not UTF-8 text".into())
}

/// Installs one curated dictionary. No network access happens unless this command is called.
pub async fn install_dictionary(app: &AppContext, id: &str) -> Result<(), String> {
    let entry = catalog::by_id(id).ok_or_else(|| format!("unknown dictionary: {id}"))?;
    if entry.bundled {
        return Ok(());
    }
    let dir = dictionary::installed_dir(app, id)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    // wooorm ships `license`; the generated repository assets ship `LICENSE`.
    let licence_file = if entry.base == catalog::GENERATED { "LICENSE" } else { "license" };
    for (target, upstream) in [
        ("index.aff", "index.aff"),
        ("index.dic", "index.dic"),
        ("LICENSE", licence_file),
    ] {
        let url = format!("{}/{}/{}", entry.base, catalog::remote_dir(entry), upstream);
        let text = download(&client, &url).await?;
        if target != "LICENSE" && text.trim().len() < 20 {
            return Err(format!("downloaded {target} is empty"));
        }
        crate::fsops::write_atomic(&dir.join(target), text.as_bytes()).map_err(|e| e.to_string())?;
    }
    cache().clear();
    Ok(())
}

pub fn remove_dictionary(app: &AppContext, id: &str) -> Result<(), String> {
    let dir = dictionary::installed_dir(app, id)?;
    for name in ["index.aff", "index.dic", "LICENSE"] {
        match fs::remove_file(dir.join(name)) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    let _ = fs::remove_dir(&dir);
    cache().clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_misspellings_and_offers_suggestions() {
        let (aff, dic) = dictionary::parts_for_bundled("en_US").unwrap();
        let result = check_with_parts("mispelled", "en_US", aff, dic, "").unwrap().unwrap();
        assert!(result.misspelled);
        assert!(result.suggestions.iter().any(|s| s == "misspelled"), "{:?}", result.suggestions);

        let good = check_with_parts("receive", "en_US", aff, dic, "").unwrap().unwrap();
        assert_eq!(good, SpellCheckDto { misspelled: false, suggestions: Vec::new() });
    }

    #[test]
    fn uses_the_selected_bundled_language() {
        for id in ["en_GB", "de_DE", "fr_FR", "es_ES"] {
            let (aff, dic) = dictionary::parts_for_bundled(id).unwrap();
            assert!(check_with_parts("qzxqzx", id, aff, dic, "").unwrap().unwrap().misspelled);
        }
        let (aff, dic) = dictionary::parts_for_bundled("de_DE").unwrap();
        assert!(!check_with_parts("Informationen", "de_DE", aff, dic, "").unwrap().unwrap().misspelled);
        let (aff, dic) = dictionary::parts_for_bundled("fr_FR").unwrap();
        assert!(!check_with_parts("bonjour", "fr_FR", aff, dic, "").unwrap().unwrap().misspelled);
        let (aff, dic) = dictionary::parts_for_bundled("es_ES").unwrap();
        assert!(!check_with_parts("información", "es_ES", aff, dic, "").unwrap().unwrap().misspelled);
    }

    #[test]
    fn reports_utf16_ranges_after_multibyte_characters() {
        let text = "Café 😀 I mispelled teh mail.";
        // "Café" is a proper noun absent from the bundled dictionary; teach it to keep the test
        // focused on UTF-16 offset conversion.
        let (aff, dic) = dictionary::parts_for_bundled("en_US").unwrap();
        let found = check_text_with_parts(text, "en_US", aff, dic, "Café\n").unwrap().unwrap();
        let utf16: Vec<u16> = text.encode_utf16().collect();
        let words: Vec<String> = found
            .iter()
            .map(|&(a, b)| String::from_utf16_lossy(&utf16[a as usize..b as usize]))
            .collect();
        assert_eq!(words, vec!["mispelled", "teh"]);
    }

    #[test]
    fn personal_words_are_accepted_and_sorted() {
        let stored = add_personal_word("", "calmmd");
        let stored = add_personal_word(&stored, "Zettel");
        let stored = add_personal_word(&stored, "calmmd");
        assert_eq!(stored, "calmmd\nzettel");

        let (aff, dic) = dictionary::parts_for_bundled("en_US").unwrap();
        let dict = cache().get("en_US", aff, dic, &stored).unwrap();
        assert!(dict.check_word("calmmd"));
        assert!(dict.check_word("zettel"));
    }

    #[test]
    fn unknown_languages_have_no_bundled_dictionary() {
        assert!(entry_for_lang(Some("sw-KE")).is_none());
    }
}
