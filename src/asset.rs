use crate::dictionary::Dictionary;
use crate::{MdxError, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Component, PathBuf};
use std::sync::Arc;

pub trait AssetSource: Send + Sync {
    fn read_asset(&self, reference: &str) -> Result<Vec<u8>>;
}

#[derive(Default, Clone)]
pub struct MemoryAssetSource {
    assets: HashMap<String, Vec<u8>>,
}

impl MemoryAssetSource {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_insert(
        mut self,
        reference: impl Into<String>,
        data: impl Into<Vec<u8>>,
    ) -> Result<Self> {
        let normalized = normalize_asset_ref(&reference.into())?;
        self.assets.insert(normalized, data.into());
        Ok(self)
    }

    pub fn insert(self, reference: impl Into<String>, data: impl Into<Vec<u8>>) -> Self {
        self.try_insert(reference, data)
            .expect("MemoryAssetSource::insert requires a safe, non-empty asset reference")
    }
}

impl AssetSource for MemoryAssetSource {
    fn read_asset(&self, reference: &str) -> Result<Vec<u8>> {
        for candidate in asset_lookup_candidates(reference) {
            let normalized = normalize_asset_ref(&candidate)?;
            if let Some(bytes) = self.assets.get(&normalized) {
                return Ok(bytes.clone());
            }
        }
        Err(MdxError::AssetNotFound(reference.to_string()))
    }
}

#[derive(Debug, Clone)]
pub struct FileAssetSource {
    root: PathBuf,
}

impl FileAssetSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl AssetSource for FileAssetSource {
    fn read_asset(&self, reference: &str) -> Result<Vec<u8>> {
        for candidate in asset_lookup_candidates(reference) {
            let normalized = normalize_asset_ref(&candidate)?;
            let path = self
                .root
                .join(normalized.replace('\\', std::path::MAIN_SEPARATOR_STR));
            match std::fs::read(&path) {
                Ok(bytes) => return Ok(bytes),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                Err(err) => return Err(MdxError::io(Some(path), err)),
            }
        }
        Err(MdxError::AssetNotFound(reference.to_string()))
    }
}

#[derive(Clone)]
pub struct DictionaryAssetSource {
    dictionary: Arc<Dictionary>,
}

impl DictionaryAssetSource {
    pub fn new(dictionary: Arc<Dictionary>) -> Self {
        Self { dictionary }
    }
}

impl AssetSource for DictionaryAssetSource {
    fn read_asset(&self, reference: &str) -> Result<Vec<u8>> {
        for candidate in asset_lookup_candidates(reference) {
            if let Some(entry) = self
                .dictionary
                .get_keyword_entries()
                .iter()
                .find(|entry| entry.lookup_key().eq_ignore_ascii_case(&candidate))
            {
                return self.dictionary.resolve(entry);
            }
            if let Some(entry) = self.dictionary.find_comparable_entry(&candidate) {
                return self.dictionary.resolve(entry);
            }
        }
        Err(MdxError::AssetNotFound(reference.to_string()))
    }
}

#[derive(Default, Clone)]
pub struct AssetResolver {
    sources: Vec<Arc<dyn AssetSource>>,
}

impl AssetResolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_source(mut self, source: impl AssetSource + 'static) -> Self {
        self.sources.push(Arc::new(source));
        self
    }

    pub fn add_source(&mut self, source: impl AssetSource + 'static) {
        self.sources.push(Arc::new(source));
    }

    pub fn read(&self, reference: &str) -> Result<Vec<u8>> {
        self.read_inner(reference, &mut HashSet::new())
    }

    fn read_inner(&self, reference: &str, seen: &mut HashSet<String>) -> Result<Vec<u8>> {
        let key = normalize_asset_resolver_ref(reference)?;
        if !seen.insert(key) {
            return Err(MdxError::AssetNotFound(reference.to_string()));
        }
        for source in &self.sources {
            match source.read_asset(reference) {
                Ok(bytes) => {
                    if let Some(target) = parse_mdict_resource_redirect(&bytes) {
                        return self.read_inner(&target, seen);
                    }
                    return Ok(bytes);
                }
                Err(MdxError::AssetNotFound(_)) | Err(MdxError::IndexMiss { .. }) => continue,
                Err(err) => return Err(err),
            }
        }
        Err(MdxError::AssetNotFound(reference.to_string()))
    }
}

pub fn normalize_asset_ref(reference: &str) -> Result<String> {
    let cleaned =
        trim_leading_resource_separators(trim_resource_scheme(reference).trim()).replace('/', "\\");
    if cleaned.is_empty() {
        return Err(MdxError::InvalidInput("asset reference is empty".into()));
    }
    for component in std::path::Path::new(&cleaned).components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err(MdxError::UnsafeAssetPath(reference.to_string()));
        }
    }
    if cleaned
        .split('\\')
        .any(|part| part == ".." || part.is_empty())
    {
        return Err(MdxError::UnsafeAssetPath(reference.to_string()));
    }
    Ok(cleaned)
}

pub fn trim_resource_scheme(value: &str) -> &str {
    for prefix in ["file://", "sound://", "snd://", "img://", "css://", "js://"] {
        if value.len() >= prefix.len() && value[..prefix.len()].eq_ignore_ascii_case(prefix) {
            return &value[prefix.len()..];
        }
    }
    value
}

fn trim_leading_resource_separators(mut value: &str) -> &str {
    while value.starts_with('/') || value.starts_with('\\') {
        value = &value[1..];
    }
    value
}

fn normalize_asset_resolver_ref(reference: &str) -> Result<String> {
    Ok(
        trim_leading_resource_separators(trim_resource_scheme(reference).trim())
            .replace('\\', "/")
            .to_lowercase(),
    )
}

pub fn normalize_mdd_key(name: &str) -> String {
    let normalized = name.trim().replace('/', "\\");
    if normalized.is_empty() {
        return "\\".to_string();
    }
    if normalized.starts_with('\\') {
        normalized
    } else {
        format!("\\{normalized}")
    }
}

pub fn asset_lookup_candidates(reference: &str) -> Vec<String> {
    let reference = reference.trim();
    if reference.is_empty() {
        return Vec::new();
    }
    let mut candidates = vec![reference.to_string()];
    if let Some(idx) = reference.find("://") {
        if idx + 3 < reference.len() {
            candidates.push(reference[idx + 3..].to_string());
        }
    }
    if reference.len() >= "sound://".len()
        && reference[.."sound://".len()].eq_ignore_ascii_case("sound://")
    {
        candidates.push(format!("snd://{}", &reference["sound://".len()..]));
    }
    if reference.len() >= "file://".len()
        && reference[.."file://".len()].eq_ignore_ascii_case("file://")
    {
        candidates.push(reference["file://".len()..].to_string());
    }

    let mut expanded = Vec::new();
    for candidate in candidates {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            continue;
        }
        expanded.push(trimmed.to_string());
        let without_leading = trimmed.trim_start_matches(['/', '\\']);
        if without_leading != trimmed {
            expanded.push(without_leading.to_string());
        }
        expanded.push(normalize_mdd_key(trimmed));
        if let Some(dot) = without_leading.rfind('.') {
            let without_ext = &without_leading[..dot];
            if !without_ext.is_empty() {
                expanded.push(without_ext.to_string());
                expanded.push(normalize_mdd_key(without_ext));
            }
        }
    }

    let mut seen = HashSet::new();
    expanded
        .into_iter()
        .filter(|candidate| !candidate.trim().is_empty() && seen.insert(candidate.clone()))
        .collect()
}

pub fn is_resource_ref(reference: &str) -> bool {
    let reference = reference.trim();
    if reference.is_empty() {
        return false;
    }
    let lower = reference.to_lowercase();
    if ["snd://", "sound://", "file://", "img://", "css://", "js://"]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return true;
    }
    if [
        "http://",
        "https://",
        "data:",
        "mailto:",
        "help:",
        "entry:",
        "mdxentry:",
        "dict:",
        "d:",
        "x:",
        "#",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
    {
        return false;
    }
    matches!(
        lower.rsplit('.').next().map(|ext| format!(".{ext}")),
        Some(ext) if matches!(ext.as_str(), ".css" | ".js" | ".jpg" | ".jpeg" | ".png" | ".gif" | ".svg" | ".webp" | ".spx" | ".snd" | ".mp3" | ".wav" | ".ogg" | ".mp4")
    )
}

pub fn extract_resource_refs(content: &[u8]) -> Vec<String> {
    let Ok(text) = std::str::from_utf8(content) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut refs = Vec::new();

    let mut append = |value: &str| {
        let value = value.trim();
        if is_resource_ref(value) && seen.insert(value.to_string()) {
            refs.push(value.to_string());
        }
    };

    for attr in ["src=", "href="] {
        let mut rest = text;
        while let Some(pos) = rest.find(attr) {
            let after = &rest[pos + attr.len()..];
            let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                rest = &after[after
                    .char_indices()
                    .nth(1)
                    .map_or(after.len(), |(idx, _)| idx)..];
                continue;
            };
            let value_start = quote.len_utf8();
            if let Some(end) = after[value_start..].find(quote) {
                append(&after[value_start..value_start + end]);
                rest = &after[value_start + end + quote.len_utf8()..];
            } else {
                break;
            }
        }
    }

    for scheme in ["snd://", "sound://", "file://", "img://", "css://", "js://"] {
        let mut start = 0;
        let lower = text.to_lowercase();
        while let Some(pos) = lower[start..].find(scheme) {
            let absolute = start + pos;
            let token = text[absolute..]
                .split(|c: char| {
                    c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')' | '(')
                })
                .next()
                .unwrap_or("");
            append(token);
            start = absolute + scheme.len();
        }
    }
    refs
}

pub fn parse_mdict_resource_redirect(data: &[u8]) -> Option<String> {
    let prefix: Vec<u8> = "@@@LINK="
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    if !data.starts_with(&prefix) {
        return None;
    }
    let payload = &data[prefix.len()..data.len() - ((data.len() - prefix.len()) % 2)];
    let mut units = Vec::new();
    for chunk in payload.chunks_exact(2) {
        let unit = u16::from_le_bytes([chunk[0], chunk[1]]);
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    let target = String::from_utf16(&units).ok()?.trim().to_string();
    (!target.is_empty()).then_some(target)
}
