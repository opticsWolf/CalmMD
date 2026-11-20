//! Hunspell dictionaries for zspell: compiled-in defaults and locally installed additions.
//!
//! The bundled files come from `wooorm/dictionaries` under permissive licences; each licence is
//! shipped beside its dictionary. Installed dictionaries live in `<app data>/dictionaries/<id>`
//! and are only fetched after an explicit user request.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::app::AppContext;
use zspell::{DictBuilder, Dictionary};

use super::catalog;

const EN_US_AFF: &str = include_str!("dictionaries/en_us/index.aff");
const EN_US_DIC: &str = include_str!("dictionaries/en_us/index.dic");
const EN_GB_AFF: &str = include_str!("dictionaries/en_GB/index.aff");
const EN_GB_DIC: &str = include_str!("dictionaries/en_GB/index.dic");
const DE_AFF: &str = include_str!("dictionaries/de_DE/index.aff");
const DE_DIC: &str = include_str!("dictionaries/de_DE/index.dic");
const FR_AFF: &str = include_str!("dictionaries/fr_FR/index.aff");
const FR_DIC: &str = include_str!("dictionaries/fr_FR/index.dic");
const ES_AFF: &str = include_str!("dictionaries/es_ES/index.aff");
const ES_DIC: &str = include_str!("dictionaries/es_ES/index.dic");

pub fn dictionaries_dir(app: &AppContext) -> Result<PathBuf, String> {
    let dir = app.data_dir().join("dictionaries");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

pub fn installed_dir(app: &AppContext, id: &str) -> Result<PathBuf, String> {
    let entry = catalog::by_id(id).ok_or_else(|| format!("unknown dictionary: {id}"))?;
    if entry.bundled || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("dictionary {id} cannot be installed locally"));
    }
    Ok(dictionaries_dir(app)?.join(id))
}

pub fn is_installed(app: &AppContext, id: &str) -> bool {
    installed_dir(app, id)
        .map(|dir| dir.join("index.aff").is_file() && dir.join("index.dic").is_file())
        .unwrap_or(false)
}

pub fn parts_for_bundled(id: &str) -> Option<(&'static str, &'static str)> {
    match id {
        "en_US" => Some((EN_US_AFF, EN_US_DIC)),
        "en_GB" => Some((EN_GB_AFF, EN_GB_DIC)),
        "de_DE" => Some((DE_AFF, DE_DIC)),
        "fr_FR" => Some((FR_AFF, FR_DIC)),
        "es_ES" => Some((ES_AFF, ES_DIC)),
        _ => None,
    }
}

fn read_parts(path: &Path, name: &str) -> Result<String, String> {
    let p = path.join(name);
    fs::read_to_string(&p).map_err(|e| format!("could not read {}: {e}", p.display()))
}

/// Source files for a dictionary: bundled first, then the locally installed copy.
pub fn parts_for(app: &AppContext, id: &str) -> Result<Option<(Cow<'static, str>, Cow<'static, str>)>, String> {
    if let Some((aff, dic)) = parts_for_bundled(id) {
        return Ok(Some((Cow::Borrowed(aff), Cow::Borrowed(dic))));
    }
    let dir = installed_dir(app, id)?;
    if !is_installed(app, id) {
        return Ok(None);
    }
    Ok(Some((Cow::Owned(read_parts(&dir, "index.aff")?), Cow::Owned(read_parts(&dir, "index.dic")?))))
}

fn cache_key(id: &str, personal_words: &str) -> String {
    format!("{id}\0{personal_words}")
}

fn build(aff: &str, dic: &str, personal_words: &str) -> Result<Dictionary, String> {
    DictBuilder::new()
        .config_str(aff)
        .dict_str(dic)
        .personal_str(personal_words)
        .build()
        .map_err(|e| format!("failed to build dictionary: {e}"))
}

/// Lazily builds dictionaries once per id + personal word list.
pub struct DictionaryCache {
    entries: RwLock<HashMap<String, Arc<Dictionary>>>,
}

impl Default for DictionaryCache {
    fn default() -> Self {
        Self { entries: RwLock::new(HashMap::new()) }
    }
}

impl DictionaryCache {
    pub fn get(
        &self,
        id: &str,
        aff: &str,
        dic: &str,
        personal_words: &str,
    ) -> Result<Arc<Dictionary>, String> {
        let key = cache_key(id, personal_words);
        if let Some(found) = self.entries.read().expect("spell dictionary cache poisoned").get(&key) {
            return Ok(found.clone());
        }
        let built = Arc::new(build(aff, dic, personal_words)?);
        let mut entries = self.entries.write().expect("spell dictionary cache poisoned");
        Ok(entries.entry(key).or_insert(built).clone())
    }

    pub fn clear(&self) {
        self.entries.write().expect("spell dictionary cache poisoned").clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_english_tags_to_a_bundled_dictionary() {
        assert_eq!(catalog::entry_for_lang(None).unwrap().id, "en_US");
        assert_eq!(catalog::entry_for_lang(Some("en")).unwrap().id, "en_US");
        assert_eq!(catalog::entry_for_lang(Some("en-GB")).unwrap().id, "en_GB");
        assert_eq!(catalog::entry_for_lang(Some("de-DE")).unwrap().id, "de_DE");
    }

    #[test]
    fn builds_every_bundled_dictionary() {
        for id in ["en_US", "en_GB", "de_DE", "fr_FR", "es_ES"] {
            let (aff, dic) = parts_for_bundled(id).expect("bundled dictionary");
            let dict = build(aff, dic, "").unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(!dict.check_word("qzxqzx"), "{id} accepted a nonsense word");
        }
    }

    #[test]
    fn personal_words_are_accepted() {
        let cache = DictionaryCache::default();
        let dict = cache.get("en_US", EN_US_AFF, EN_US_DIC, "calmmd\n").unwrap();
        assert!(dict.check_word("calmmd"));
        assert!(!cache.get("en_US", EN_US_AFF, EN_US_DIC, "").unwrap().check_word("calmmd"));
    }
}
