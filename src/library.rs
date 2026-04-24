use crate::asset::{AssetResolver, DictionaryAssetSource, FileAssetSource};
use crate::dictionary::Dictionary;
use crate::{MdxError, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionarySpec {
    pub id: String,
    pub mdx_path: PathBuf,
    pub mdd_paths: Vec<PathBuf>,
}

impl DictionarySpec {
    pub fn new(id: impl Into<String>, mdx_path: impl Into<PathBuf>) -> Self {
        Self {
            id: id.into(),
            mdx_path: mdx_path.into(),
            mdd_paths: Vec::new(),
        }
    }
    pub fn with_mdd(mut self, mdd_path: impl Into<PathBuf>) -> Self {
        self.mdd_paths.push(mdd_path.into());
        self
    }
}

#[derive(Default)]
pub struct DictionaryLibrary {
    specs: HashMap<String, DictionarySpec>,
    dictionaries: HashMap<String, Dictionary>,
    companions: HashMap<String, Vec<Arc<Dictionary>>>,
}

impl DictionaryLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, spec: DictionarySpec) -> Result<()> {
        if spec.id.trim().is_empty() {
            return Err(MdxError::InvalidInput("dictionary id is required".into()));
        }
        if spec.mdx_path.as_os_str().is_empty() {
            return Err(MdxError::InvalidInput("mdx path is required".into()));
        }
        self.specs.insert(spec.id.clone(), spec);
        Ok(())
    }

    pub fn register_dictionary(
        &mut self,
        id: impl Into<String>,
        dictionary: Dictionary,
    ) -> Result<()> {
        let id = id.into();
        if id.trim().is_empty() {
            return Err(MdxError::InvalidInput("dictionary id is required".into()));
        }
        self.dictionaries.insert(id, dictionary);
        Ok(())
    }

    pub fn open(&mut self, id: &str) -> Result<&Dictionary> {
        if !self.dictionaries.contains_key(id) {
            let spec = self
                .specs
                .get(id)
                .cloned()
                .ok_or_else(|| MdxError::DictionaryNotFound(id.to_string()))?;
            let mut mdx = Dictionary::open(&spec.mdx_path)?;
            mdx.build_index()?;

            let mut resolver = AssetResolver::new();
            if let Some(parent) = spec.mdx_path.parent() {
                resolver.add_source(FileAssetSource::new(parent));
            }

            let mut mdds = Vec::new();
            for path in &spec.mdd_paths {
                let mut mdd = Dictionary::open(path)?;
                mdd.build_index()?;
                let shared = Arc::new(mdd);
                resolver.add_source(DictionaryAssetSource::new(shared.clone()));
                mdds.push(shared);
            }
            mdx.set_asset_resolver(resolver);
            self.companions.insert(id.to_string(), mdds);
            self.dictionaries.insert(id.to_string(), mdx);
        }
        self.dictionaries
            .get(id)
            .ok_or_else(|| MdxError::DictionaryNotFound(id.to_string()))
    }

    pub fn get(&self, id: &str) -> Option<&Dictionary> {
        self.dictionaries.get(id)
    }
    pub fn companions(&self, id: &str) -> &[Arc<Dictionary>] {
        self.companions.get(id).map_or(&[], Vec::as_slice)
    }
    pub fn specs(&self) -> impl Iterator<Item = &DictionarySpec> {
        self.specs.values()
    }
}
