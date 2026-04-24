use crate::dictionary::{DictionaryInfo, MDictKeywordEntry};
use crate::index::{fuzzy_score, SearchHit};
use crate::{MdxError, Result};
use std::collections::HashMap;

pub trait IndexStore {
    fn put(&mut self, info: DictionaryInfo, entries: Vec<MDictKeywordEntry>) -> Result<()>;
    fn get_exact(&self, dictionary_name: &str, keyword: &str) -> Result<MDictKeywordEntry>;
    fn prefix_search(
        &self,
        dictionary_name: &str,
        prefix: &str,
        limit: usize,
    ) -> Result<Vec<MDictKeywordEntry>>;
    fn delete_dictionary(&mut self, dictionary_name: &str) -> Result<()>;
}

#[derive(Default)]
pub struct MemoryIndexStore {
    entries_by_dict: HashMap<String, Vec<MDictKeywordEntry>>,
    exact_by_dict: HashMap<String, HashMap<String, MDictKeywordEntry>>,
    infos: HashMap<String, DictionaryInfo>,
}

impl MemoryIndexStore {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn dictionary_info(&self, name: &str) -> Option<&DictionaryInfo> {
        self.infos.get(name)
    }
}

impl IndexStore for MemoryIndexStore {
    fn put(&mut self, info: DictionaryInfo, entries: Vec<MDictKeywordEntry>) -> Result<()> {
        if info.name.trim().is_empty() {
            return Err(MdxError::InvalidInput("dictionary name is required".into()));
        }
        let mut exact = HashMap::new();
        for entry in &entries {
            exact
                .entry(entry.lookup_key().to_string())
                .or_insert_with(|| entry.clone());
        }
        self.exact_by_dict.insert(info.name.clone(), exact);
        self.entries_by_dict.insert(info.name.clone(), entries);
        self.infos.insert(info.name.clone(), info);
        Ok(())
    }

    fn get_exact(&self, dictionary_name: &str, keyword: &str) -> Result<MDictKeywordEntry> {
        self.exact_by_dict
            .get(dictionary_name)
            .and_then(|exact| exact.get(keyword))
            .cloned()
            .ok_or_else(|| MdxError::IndexMiss {
                dictionary: dictionary_name.to_string(),
                keyword: keyword.to_string(),
            })
    }

    fn prefix_search(
        &self,
        dictionary_name: &str,
        prefix: &str,
        limit: usize,
    ) -> Result<Vec<MDictKeywordEntry>> {
        let entries =
            self.entries_by_dict
                .get(dictionary_name)
                .ok_or_else(|| MdxError::IndexMiss {
                    dictionary: dictionary_name.to_string(),
                    keyword: prefix.to_string(),
                })?;
        let prefix = prefix.trim().to_lowercase();
        let mut results = Vec::new();
        for entry in entries {
            if prefix.is_empty() || entry.lookup_key().to_lowercase().starts_with(&prefix) {
                results.push(entry.clone());
                if limit > 0 && results.len() >= limit {
                    break;
                }
            }
        }
        if results.is_empty() {
            return Err(MdxError::IndexMiss {
                dictionary: dictionary_name.to_string(),
                keyword: prefix,
            });
        }
        Ok(results)
    }

    fn delete_dictionary(&mut self, dictionary_name: &str) -> Result<()> {
        self.entries_by_dict.remove(dictionary_name);
        self.exact_by_dict.remove(dictionary_name);
        self.infos.remove(dictionary_name);
        Ok(())
    }
}

pub trait FuzzyIndexStore {
    fn put(&mut self, info: DictionaryInfo, entries: Vec<MDictKeywordEntry>) -> Result<()>;
    fn search(&self, dictionary_name: &str, query: &str, limit: usize) -> Result<Vec<SearchHit>>;
}

#[derive(Default)]
pub struct MemoryFuzzyIndexStore {
    entries_by_dict: HashMap<String, Vec<MDictKeywordEntry>>,
}

impl MemoryFuzzyIndexStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl FuzzyIndexStore for MemoryFuzzyIndexStore {
    fn put(&mut self, info: DictionaryInfo, entries: Vec<MDictKeywordEntry>) -> Result<()> {
        if info.name.trim().is_empty() {
            return Err(MdxError::InvalidInput("dictionary name is required".into()));
        }
        self.entries_by_dict.insert(info.name, entries);
        Ok(())
    }

    fn search(&self, dictionary_name: &str, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let entries =
            self.entries_by_dict
                .get(dictionary_name)
                .ok_or_else(|| MdxError::IndexMiss {
                    dictionary: dictionary_name.to_string(),
                    keyword: query.to_string(),
                })?;
        let query = query.trim();
        if query.is_empty() {
            return Err(MdxError::IndexMiss {
                dictionary: dictionary_name.to_string(),
                keyword: query.to_string(),
            });
        }
        let mut hits = Vec::new();
        for entry in entries {
            if let Some((score, source)) = fuzzy_score(query, entry.lookup_key()) {
                hits.push(SearchHit {
                    entry: entry.clone(),
                    score,
                    source,
                });
            }
        }
        if hits.is_empty() {
            return Err(MdxError::IndexMiss {
                dictionary: dictionary_name.to_string(),
                keyword: query.to_string(),
            });
        }
        hits.sort();
        if limit > 0 && hits.len() > limit {
            hits.truncate(limit);
        }
        Ok(hits)
    }
}
