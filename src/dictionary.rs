use crate::asset::AssetResolver;
use crate::binary::{decode_utf16le, header_attr, read_meta_uint};
use crate::index::normalize_comparable_key;
use crate::range_tree::{RecordBlockInfo, RecordRangeTree};
use crate::{MdxError, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MDictType {
    Mdd,
    Mdx,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub title: String,
    pub description: String,
    pub creation_date: String,
    pub generated_by_engine_version: String,
    pub version: String,
    pub encoding: String,
    pub encrypted: u8,
    pub register_by: String,
}

impl Metadata {
    pub fn from_header(header: &str) -> Self {
        Self {
            title: header_attr(header, "Title").unwrap_or_default(),
            description: header_attr(header, "Description").unwrap_or_default(),
            creation_date: header_attr(header, "CreationDate").unwrap_or_default(),
            generated_by_engine_version: header_attr(header, "GeneratedByEngineVersion")
                .unwrap_or_default(),
            version: header_attr(header, "GeneratedByEngineVersion").unwrap_or_default(),
            encoding: header_attr(header, "Encoding").unwrap_or_else(|| "UTF-8".to_string()),
            encrypted: parse_encrypted_header(header_attr(header, "Encrypted").as_deref()),
            register_by: header_attr(header, "RegisterBy").unwrap_or_default(),
        }
    }
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            title: String::new(),
            description: String::new(),
            creation_date: String::new(),
            generated_by_engine_version: String::new(),
            version: String::new(),
            encoding: "UTF-8".to_string(),
            encrypted: 0,
            register_by: String::new(),
        }
    }
}

fn parse_encrypted_header(value: Option<&str>) -> u8 {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        None | Some("No") => 0,
        Some("Yes") => 1,
        Some(value) if value.starts_with('2') => 2,
        Some(value) if value.starts_with('1') => 1,
        Some(value) => value.parse::<u8>().unwrap_or(0),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MDictKeywordEntry {
    pub record_start_offset: u64,
    pub record_end_offset: u64,
    pub keyword: String,
    pub key_block_index: u64,
    pub is_resource: bool,
    pub normalized_keyword: Option<String>,
}

impl MDictKeywordEntry {
    pub fn new(keyword: impl Into<String>, start: u64, end: u64) -> Result<Self> {
        if end < start {
            return Err(MdxError::InvalidInput(format!(
                "record end offset {end} precedes start offset {start}"
            )));
        }
        Ok(Self {
            record_start_offset: start,
            record_end_offset: end,
            keyword: keyword.into(),
            key_block_index: 0,
            is_resource: false,
            normalized_keyword: None,
        })
    }

    pub fn lookup_key(&self) -> &str {
        if self.is_resource {
            self.normalized_keyword.as_deref().unwrap_or(&self.keyword)
        } else {
            &self.keyword
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MDictKeywordIndexRecordBlock {
    pub data_start_offset: u64,
    pub compressed_size: u64,
    pub decompressed_size: u64,
    pub keyword_part_start_offset: u64,
    pub keyword_part_data_end_offset: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MDictKeywordIndex {
    pub keyword_entry: MDictKeywordEntry,
    pub record_block: MDictKeywordIndexRecordBlock,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryInfo {
    pub name: String,
    pub title: String,
    pub entry_count: usize,
    pub path: Option<PathBuf>,
    pub dict_type: MDictType,
}

#[derive(Clone)]
pub struct Dictionary {
    path: Option<PathBuf>,
    name: String,
    dict_type: MDictType,
    metadata: Metadata,
    entries: Vec<MDictKeywordEntry>,
    records: Vec<u8>,
    exact_lookup: HashMap<String, usize>,
    comparable_lookup: HashMap<String, usize>,
    range_tree: Option<RecordRangeTree>,
    raw_record_bytes: bool,
    asset_resolver: Option<Arc<AssetResolver>>,
}

pub fn metadata_from_mdict_header_bytes(data: &[u8]) -> Result<Metadata> {
    let header_len_bytes = data.get(..4).ok_or_else(|| {
        MdxError::InvalidFormat("MDict header is missing 4-byte header length".into())
    })?;
    let header_len = u32::from_be_bytes(
        header_len_bytes
            .try_into()
            .expect("slice length checked for header length"),
    ) as usize;
    let header_start = 4usize;
    let header_end = header_start
        .checked_add(header_len)
        .ok_or_else(|| MdxError::InvalidFormat("MDict header length overflow".into()))?;
    let raw_header = data
        .get(header_start..header_end)
        .ok_or_else(|| MdxError::InvalidFormat("MDict header bytes are truncated".into()))?;
    let header = crate::binary::decode_utf16le(raw_header)?.replace("Library_Data", "Dictionary");
    Ok(Metadata::from_header(&header))
}

impl Dictionary {
    pub fn builder(name: impl Into<String>) -> DictionaryBuilder {
        DictionaryBuilder::new(name)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let data =
            std::fs::read(path).map_err(|err| MdxError::io(Some(path.to_path_buf()), err))?;
        Self::from_bytes(
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| {
                    MdxError::InvalidInput(format!(
                        "path {} has no UTF-8 file stem",
                        path.display()
                    ))
                })?,
            Some(path.to_path_buf()),
            &data,
        )
    }

    pub fn from_bytes(name: impl Into<String>, path: Option<PathBuf>, data: &[u8]) -> Result<Self> {
        let name = name.into();
        if data.starts_with(b"MDXRS\0") {
            return Self::parse_synthetic(name, path, data);
        }
        let dict_type = path
            .as_ref()
            .and_then(|p| p.extension())
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                if ext.eq_ignore_ascii_case("mdd") {
                    MDictType::Mdd
                } else {
                    MDictType::Mdx
                }
            })
            .unwrap_or(MDictType::Mdx);
        parse_real_dictionary(name, path, data, dict_type)
    }

    fn parse_synthetic(name: String, path: Option<PathBuf>, data: &[u8]) -> Result<Self> {
        let mut cursor = 6;
        let count_bytes = data.get(cursor..cursor + 4).ok_or_else(|| {
            MdxError::InvalidFormat("synthetic dictionary missing entry count".into())
        })?;
        let count = u32::from_be_bytes(count_bytes.try_into().expect("length checked")) as usize;
        cursor += 4;
        let mut builder = DictionaryBuilder::new(name).path_opt(path);
        let mut record_data = Vec::new();
        for _ in 0..count {
            let key_len = *data.get(cursor).ok_or_else(|| {
                MdxError::InvalidFormat("synthetic dictionary missing key length".into())
            })? as usize;
            cursor += 1;
            let key = std::str::from_utf8(data.get(cursor..cursor + key_len).ok_or_else(|| {
                MdxError::InvalidFormat("synthetic dictionary truncated key".into())
            })?)
            .map_err(|err| {
                MdxError::InvalidFormat(format!("synthetic dictionary key is not UTF-8: {err}"))
            })?
            .to_owned();
            cursor += key_len;
            let value_len_bytes = data.get(cursor..cursor + 4).ok_or_else(|| {
                MdxError::InvalidFormat("synthetic dictionary missing value length".into())
            })?;
            let value_len =
                u32::from_be_bytes(value_len_bytes.try_into().expect("length checked")) as usize;
            cursor += 4;
            let value = data.get(cursor..cursor + value_len).ok_or_else(|| {
                MdxError::InvalidFormat("synthetic dictionary truncated value".into())
            })?;
            cursor += value_len;
            let start = record_data.len() as u64;
            record_data.extend_from_slice(value);
            let end = record_data.len() as u64;
            builder = builder.entry(MDictKeywordEntry::new(key, start, end)?);
        }
        Ok(builder.records(record_data).build())
    }

    pub fn build_index(&mut self) -> Result<()> {
        self.rebuild_lookup_maps();
        Ok(())
    }

    fn rebuild_lookup_maps(&mut self) {
        self.exact_lookup.clear();
        self.comparable_lookup.clear();
        for (idx, entry) in self.entries.iter().enumerate() {
            self.exact_lookup
                .entry(entry.lookup_key().to_string())
                .or_insert(idx);
            self.comparable_lookup
                .entry(normalize_comparable_key(entry.lookup_key()))
                .or_insert(idx);
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn title(&self) -> &str {
        &self.metadata.title
    }
    pub fn description(&self) -> &str {
        &self.metadata.description
    }
    pub fn generated_by_engine_version(&self) -> &str {
        &self.metadata.generated_by_engine_version
    }
    pub fn creation_date(&self) -> &str {
        &self.metadata.creation_date
    }
    pub fn version(&self) -> &str {
        &self.metadata.version
    }
    pub fn is_mdd(&self) -> bool {
        self.dict_type == MDictType::Mdd
    }
    pub fn is_record_encrypted(&self) -> bool {
        self.metadata.encrypted == 1
    }
    pub fn is_utf16(&self) -> bool {
        self.metadata.encoding.eq_ignore_ascii_case("UTF-16")
            || self.metadata.encoding.eq_ignore_ascii_case("UTF-16LE")
    }
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    pub fn dictionary_info(&self) -> DictionaryInfo {
        DictionaryInfo {
            name: self.name.clone(),
            title: self.title().to_string(),
            entry_count: self.entries.len(),
            path: self.path.clone(),
            dict_type: self.dict_type,
        }
    }

    pub fn lookup(&self, word: &str) -> Result<Vec<u8>> {
        self.lookup_with_redirects(word, 0, &mut HashSet::new())
    }

    fn lookup_with_redirects(
        &self,
        word: &str,
        depth: u8,
        seen: &mut HashSet<String>,
    ) -> Result<Vec<u8>> {
        let word = word.trim();
        if word.is_empty() || depth > 8 {
            return Err(MdxError::IndexMiss {
                dictionary: self.name.clone(),
                keyword: word.to_string(),
            });
        }
        let entry = self
            .find_exact_entry(word)
            .or_else(|| self.find_comparable_entry(word))
            .ok_or_else(|| MdxError::IndexMiss {
                dictionary: self.name.clone(),
                keyword: word.to_string(),
            })?;
        let content = self.resolve(entry)?;
        let Some(target) = parse_link_target(&content) else {
            return Ok(content);
        };
        let normalized = target.to_lowercase();
        if !seen.insert(normalized) {
            return Ok(content);
        }
        self.lookup_with_redirects(&target, depth + 1, seen)
    }

    pub fn find_exact_entry(&self, word: &str) -> Option<&MDictKeywordEntry> {
        self.exact_lookup
            .get(word)
            .and_then(|idx| self.entries.get(*idx))
    }

    pub fn find_comparable_entry(&self, word: &str) -> Option<&MDictKeywordEntry> {
        self.comparable_lookup
            .get(&normalize_comparable_key(word))
            .and_then(|idx| self.entries.get(*idx))
    }

    pub fn resolve(&self, entry: &MDictKeywordEntry) -> Result<Vec<u8>> {
        let start = usize::try_from(entry.record_start_offset).map_err(|_| {
            MdxError::InvalidFormat("record start offset does not fit usize".into())
        })?;
        let end = if entry.record_end_offset == 0 {
            self.records.len()
        } else {
            usize::try_from(entry.record_end_offset).map_err(|_| {
                MdxError::InvalidFormat("record end offset does not fit usize".into())
            })?
        };
        let slice = self.records.get(start..end).ok_or_else(|| {
            MdxError::InvalidFormat(format!(
                "record range {start}..{end} exceeds record data length {}",
                self.records.len()
            ))
        })?;
        if self.raw_record_bytes && self.dict_type != MDictType::Mdd && self.is_utf16() {
            return Ok(decode_utf16le(slice)?.into_bytes());
        }
        Ok(slice.to_vec())
    }

    pub fn keyword_entry_to_index(&self, entry: &MDictKeywordEntry) -> Result<MDictKeywordIndex> {
        let block = self
            .range_tree
            .as_ref()
            .and_then(|tree| tree.query(entry.record_start_offset))
            .cloned()
            .unwrap_or(RecordBlockInfo {
                compressed_size: self.records.len() as u64,
                decompressed_size: self.records.len() as u64,
                compressed_accumulator_offset: 0,
                decompressed_accumulator_offset: 0,
            });
        Ok(MDictKeywordIndex {
            keyword_entry: entry.clone(),
            record_block: MDictKeywordIndexRecordBlock {
                data_start_offset: block.compressed_accumulator_offset,
                compressed_size: block.compressed_size,
                decompressed_size: block.decompressed_size,
                keyword_part_start_offset: entry
                    .record_start_offset
                    .saturating_sub(block.decompressed_accumulator_offset),
                keyword_part_data_end_offset: if entry.record_end_offset == 0 {
                    block.decompressed_size
                } else {
                    entry
                        .record_end_offset
                        .saturating_sub(block.decompressed_accumulator_offset)
                },
            },
        })
    }

    pub fn get_keyword_entries(&self) -> &[MDictKeywordEntry] {
        &self.entries
    }
    pub fn export_index(&self) -> Vec<MDictKeywordEntry> {
        self.entries.clone()
    }
    pub fn export_entries(&self) -> Vec<MDictKeywordEntry> {
        self.entries
            .iter()
            .filter(|entry| !entry.is_resource)
            .cloned()
            .collect()
    }
    pub fn export_resources(&self) -> Vec<MDictKeywordEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.is_resource)
            .cloned()
            .collect()
    }
    pub fn set_asset_resolver(&mut self, resolver: AssetResolver) {
        self.asset_resolver = Some(Arc::new(resolver));
    }
    pub fn asset_resolver(&self) -> Option<Arc<AssetResolver>> {
        self.asset_resolver.clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextEncoding {
    Utf8,
    Utf16,
}

#[derive(Debug)]
struct RealParseContext {
    dict_type: MDictType,
    metadata: Metadata,
    version: f32,
    number_width: usize,
    encoding: TextEncoding,
    cursor: usize,
}

#[derive(Debug, Clone)]
struct KeyBlockInfoItem {
    compressed_size: u64,
    decompressed_size: u64,
}

fn parse_real_dictionary(
    name: String,
    path: Option<PathBuf>,
    data: &[u8],
    dict_type: MDictType,
) -> Result<Dictionary> {
    let (mut ctx, mut cursor) = parse_real_context(data, dict_type)?;
    ctx.cursor = cursor;
    let (
        key_block_count,
        entry_count,
        key_info_decompressed_size,
        key_info_compressed_size,
        key_data_size,
    ) = parse_key_block_meta(data, &mut ctx)?;
    cursor = ctx.cursor;
    let key_info_raw = checked_slice(data, cursor, key_info_compressed_size as usize)?;
    cursor = cursor
        .checked_add(key_info_compressed_size as usize)
        .ok_or_else(|| MdxError::InvalidFormat("key info cursor overflow".into()))?;
    let key_info = decode_zlib_wrapped_block(
        key_info_raw,
        key_info_decompressed_size as usize,
        "key block info",
    )?;
    let key_blocks = parse_key_block_info(&key_info, &ctx, key_block_count)?;
    let key_data_raw = checked_slice(data, cursor, key_data_size as usize)?;
    cursor = cursor
        .checked_add(key_data_size as usize)
        .ok_or_else(|| MdxError::InvalidFormat("key data cursor overflow".into()))?;
    let mut entries = parse_key_entries(key_data_raw, &key_blocks, &ctx)?;
    if entries.len() != entry_count as usize {
        return Err(MdxError::InvalidFormat(format!(
            "decoded {} key entries but metadata declares {entry_count}",
            entries.len()
        )));
    }

    let record_block_count = read_num(data, cursor, ctx.number_width)?;
    cursor += ctx.number_width;
    let record_entry_count = read_num(data, cursor, ctx.number_width)?;
    cursor += ctx.number_width;
    if record_entry_count != entry_count {
        return Err(MdxError::InvalidFormat(format!(
            "record entry count {record_entry_count} does not match key entry count {entry_count}"
        )));
    }
    let record_info_size = read_num(data, cursor, ctx.number_width)?;
    cursor += ctx.number_width;
    let record_data_size = read_num(data, cursor, ctx.number_width)?;
    cursor += ctx.number_width;

    let record_infos = parse_record_block_info(
        checked_slice(data, cursor, record_info_size as usize)?,
        &ctx,
        record_block_count,
    )?;
    cursor += record_info_size as usize;
    let record_data = checked_slice(data, cursor, record_data_size as usize)?;
    let records = decode_record_blocks(record_data, &record_infos, &ctx)?;
    let range_tree = RecordRangeTree::new(record_infos.clone())?;

    for entry in &entries {
        let end = if entry.record_end_offset == 0 {
            records.len() as u64
        } else {
            entry.record_end_offset
        };
        if end < entry.record_start_offset || end as usize > records.len() {
            return Err(MdxError::InvalidFormat(format!(
                "entry '{}' has invalid record range {}..{} for {} bytes",
                entry.keyword,
                entry.record_start_offset,
                end,
                records.len()
            )));
        }
    }

    let builder = DictionaryBuilder::new(name)
        .path_opt(path)
        .dict_type(ctx.dict_type)
        .metadata(ctx.metadata)
        .entries(std::mem::take(&mut entries))
        .records(records)
        .range_tree(range_tree)
        .raw_record_bytes(true);
    Ok(builder.build())
}

fn parse_real_context(data: &[u8], dict_type: MDictType) -> Result<(RealParseContext, usize)> {
    let metadata = metadata_from_mdict_header_bytes(data)?;
    let header_len =
        u32::from_be_bytes(checked_slice(data, 0, 4)?.try_into().expect("checked")) as usize;
    let cursor = 4usize
        .checked_add(header_len)
        .and_then(|v| v.checked_add(4))
        .ok_or_else(|| MdxError::InvalidFormat("header cursor overflow".into()))?;
    if cursor > data.len() {
        return Err(MdxError::InvalidFormat("header exceeds file length".into()));
    }
    let version: f32 = metadata
        .generated_by_engine_version
        .parse()
        .map_err(|err| {
            MdxError::InvalidFormat(format!(
                "invalid GeneratedByEngineVersion '{}': {err}",
                metadata.generated_by_engine_version
            ))
        })?;
    let number_width = if version >= 2.0 { 8 } else { 4 };
    let encoding = if metadata.encoding.eq_ignore_ascii_case("UTF-16")
        || metadata.encoding.eq_ignore_ascii_case("UTF-16LE")
    {
        TextEncoding::Utf16
    } else {
        TextEncoding::Utf8
    };
    if metadata.encrypted != 0 {
        return Err(MdxError::Unsupported(
            "encrypted MDict files are not supported by this Rust parser yet".into(),
        ));
    }
    Ok((
        RealParseContext {
            dict_type,
            metadata,
            version,
            number_width,
            encoding,
            cursor,
        },
        cursor,
    ))
}

fn parse_key_block_meta(
    data: &[u8],
    ctx: &mut RealParseContext,
) -> Result<(u64, u64, u64, u64, u64)> {
    let meta_len = if ctx.version >= 2.0 { 40 } else { 16 };
    let raw = checked_slice(data, ctx.cursor, meta_len)?;
    let key_block_count = read_meta_uint(raw, 0, ctx.number_width)?;
    let entry_count = read_meta_uint(raw, ctx.number_width, ctx.number_width)?;
    let (key_info_decompressed_size, size_offset) = if ctx.version >= 2.0 {
        (
            read_meta_uint(raw, ctx.number_width * 2, ctx.number_width)?,
            ctx.number_width * 3,
        )
    } else {
        (0, ctx.number_width * 2)
    };
    let key_info_compressed_size = read_meta_uint(raw, size_offset, ctx.number_width)?;
    let key_data_size = read_meta_uint(raw, size_offset + ctx.number_width, ctx.number_width)?;
    ctx.cursor += meta_len;
    if ctx.version >= 2.0 {
        ctx.cursor += 4;
    }
    Ok((
        key_block_count,
        entry_count,
        key_info_decompressed_size,
        key_info_compressed_size,
        key_data_size,
    ))
}

fn parse_key_block_info(
    buf: &[u8],
    ctx: &RealParseContext,
    count: u64,
) -> Result<Vec<KeyBlockInfoItem>> {
    let mut offset = 0usize;
    let mut items = Vec::new();
    for _ in 0..count {
        let _entries = read_num(buf, offset, ctx.number_width)?;
        offset += ctx.number_width;
        let first_len = read_key_len(buf, &mut offset, ctx)?;
        skip_key_text(buf, &mut offset, first_len, ctx)?;
        let last_len = read_key_len(buf, &mut offset, ctx)?;
        skip_key_text(buf, &mut offset, last_len, ctx)?;
        let compressed_size = read_num(buf, offset, ctx.number_width)?;
        offset += ctx.number_width;
        let decompressed_size = read_num(buf, offset, ctx.number_width)?;
        offset += ctx.number_width;
        items.push(KeyBlockInfoItem {
            compressed_size,
            decompressed_size,
        });
    }
    Ok(items)
}

fn read_key_len(buf: &[u8], offset: &mut usize, ctx: &RealParseContext) -> Result<usize> {
    let len = if ctx.version >= 2.0 {
        u16::from_be_bytes(checked_slice(buf, *offset, 2)?.try_into().expect("checked")) as usize
    } else {
        *checked_slice(buf, *offset, 1)?.first().expect("checked") as usize
    };
    *offset += if ctx.version >= 2.0 { 2 } else { 1 };
    Ok(len)
}

fn skip_key_text(
    buf: &[u8],
    offset: &mut usize,
    char_len: usize,
    ctx: &RealParseContext,
) -> Result<()> {
    let term = if ctx.version >= 2.0 { 1 } else { 0 };
    let width = if ctx.encoding == TextEncoding::Utf16 || ctx.dict_type == MDictType::Mdd {
        2
    } else {
        1
    };
    let bytes = (char_len + term)
        .checked_mul(width)
        .ok_or_else(|| MdxError::InvalidFormat("key text length overflow".into()))?;
    checked_slice(buf, *offset, bytes)?;
    *offset += bytes;
    Ok(())
}

fn parse_key_entries(
    raw: &[u8],
    blocks: &[KeyBlockInfoItem],
    ctx: &RealParseContext,
) -> Result<Vec<MDictKeywordEntry>> {
    let mut cursor = 0usize;
    let mut entries = Vec::new();
    for block in blocks {
        let block_raw = checked_slice(raw, cursor, block.compressed_size as usize)?;
        cursor += block.compressed_size as usize;
        let key_block =
            decode_compressed_block(block_raw, block.decompressed_size as usize, "key block")?;
        split_key_block(&key_block, ctx, &mut entries)?;
    }
    Ok(entries)
}

fn split_key_block(
    key_block: &[u8],
    ctx: &RealParseContext,
    entries: &mut Vec<MDictKeywordEntry>,
) -> Result<()> {
    let width = if ctx.encoding == TextEncoding::Utf16 || ctx.dict_type == MDictType::Mdd {
        2
    } else {
        1
    };
    let mut pos = 0usize;
    while pos < key_block.len() {
        let start_offset = read_num(key_block, pos, ctx.number_width)?;
        pos += ctx.number_width;
        let key_start = pos;
        while pos < key_block.len() {
            if width == 1 && key_block[pos] == 0 {
                break;
            }
            if width == 2
                && pos + 1 < key_block.len()
                && key_block[pos] == 0
                && key_block[pos + 1] == 0
            {
                break;
            }
            pos += width;
        }
        let key_bytes = checked_slice(key_block, key_start, pos - key_start)?;
        let key = if width == 2 {
            decode_utf16le(key_bytes)?
        } else {
            crate::binary::decode_utf8(key_bytes)?
        };
        pos = pos
            .checked_add(width)
            .ok_or_else(|| MdxError::InvalidFormat("key terminator overflow".into()))?;
        let mut entry = MDictKeywordEntry::new(key, start_offset, start_offset)?;
        entry.record_end_offset = 0;
        entry.is_resource = ctx.dict_type == MDictType::Mdd;
        if entry.is_resource {
            entry.normalized_keyword = Some(crate::asset::normalize_mdd_key(&entry.keyword));
        }
        if let Some(prev) = entries.last_mut() {
            prev.record_end_offset = entry.record_start_offset;
        }
        entries.push(entry);
    }
    Ok(())
}

fn parse_record_block_info(
    buf: &[u8],
    ctx: &RealParseContext,
    count: u64,
) -> Result<Vec<RecordBlockInfo>> {
    let mut offset = 0usize;
    let mut comp_acc = 0u64;
    let mut decomp_acc = 0u64;
    let mut infos = Vec::new();
    for _ in 0..count {
        let compressed_size = read_num(buf, offset, ctx.number_width)?;
        offset += ctx.number_width;
        let decompressed_size = read_num(buf, offset, ctx.number_width)?;
        offset += ctx.number_width;
        infos.push(RecordBlockInfo {
            compressed_size,
            decompressed_size,
            compressed_accumulator_offset: comp_acc,
            decompressed_accumulator_offset: decomp_acc,
        });
        comp_acc += compressed_size;
        decomp_acc += decompressed_size;
    }
    if offset != buf.len() {
        return Err(MdxError::InvalidFormat(format!(
            "record block info consumed {offset} bytes but buffer has {}",
            buf.len()
        )));
    }
    Ok(infos)
}

fn decode_record_blocks(
    raw: &[u8],
    infos: &[RecordBlockInfo],
    _ctx: &RealParseContext,
) -> Result<Vec<u8>> {
    let mut cursor = 0usize;
    let mut records = Vec::new();
    for info in infos {
        let block_raw = checked_slice(raw, cursor, info.compressed_size as usize)?;
        cursor += info.compressed_size as usize;
        let block =
            decode_compressed_block(block_raw, info.decompressed_size as usize, "record block")?;
        records.extend_from_slice(&block);
    }
    Ok(records)
}

fn decode_zlib_wrapped_block(raw: &[u8], expected_len: usize, label: &str) -> Result<Vec<u8>> {
    decode_compressed_block(raw, expected_len, label)
}

fn decode_compressed_block(raw: &[u8], expected_len: usize, label: &str) -> Result<Vec<u8>> {
    if raw.len() < 8 {
        return Err(MdxError::InvalidFormat(format!(
            "{label} is shorter than 8-byte compression header"
        )));
    }
    let comp_type = raw[0];
    let expected_checksum = u32::from_be_bytes(
        raw[4..8]
            .try_into()
            .expect("compression header checksum length checked"),
    );
    let payload = &raw[8..];
    let out = match comp_type {
        0 => payload.to_vec(),
        2 => {
            let mut decoder = flate2::read::ZlibDecoder::new(payload);
            let mut out = Vec::new();
            std::io::Read::read_to_end(&mut decoder, &mut out).map_err(|err| {
                MdxError::InvalidFormat(format!("zlib decode failed for {label}: {err}"))
            })?;
            out
        }
        1 => {
            let mut out = vec![0u8; expected_len];
            let len = lzokay::decompress::decompress(payload, &mut out).map_err(|err| {
                MdxError::InvalidFormat(format!("LZO decode failed for {label}: {err:?}"))
            })?;
            out.truncate(len);
            out
        }
        other => {
            return Err(MdxError::InvalidFormat(format!(
                "unknown {label} compression type {other}"
            )))
        }
    };
    if expected_len != 0 && out.len() != expected_len {
        return Err(MdxError::InvalidFormat(format!(
            "{label} decompressed to {} bytes, expected {expected_len}",
            out.len()
        )));
    }
    if expected_checksum != 0 {
        let actual = adler32(&out);
        if actual != expected_checksum {
            return Err(MdxError::InvalidFormat(format!(
                "{label} checksum mismatch: expected {expected_checksum}, got {actual}"
            )));
        }
    }
    Ok(out)
}

fn adler32(data: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65_521;
    let mut a = 1u32;
    let mut b = 0u32;
    for byte in data {
        a = (a + u32::from(*byte)) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }
    (b << 16) | a
}

fn read_num(buf: &[u8], offset: usize, width: usize) -> Result<u64> {
    read_meta_uint(buf, offset, width)
}

fn checked_slice(buf: &[u8], start: usize, len: usize) -> Result<&[u8]> {
    let end = start
        .checked_add(len)
        .ok_or_else(|| MdxError::InvalidFormat("slice offset overflow".into()))?;
    buf.get(start..end).ok_or_else(|| {
        MdxError::InvalidFormat(format!(
            "slice {start}..{end} exceeds buffer length {}",
            buf.len()
        ))
    })
}

pub fn parse_link_target(content: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(content).ok()?.trim();
    let target = text.strip_prefix("@@LINK=")?.trim();
    if target.is_empty() {
        None
    } else {
        Some(target.to_string())
    }
}

pub struct DictionaryBuilder {
    name: String,
    path: Option<PathBuf>,
    dict_type: MDictType,
    metadata: Metadata,
    entries: Vec<MDictKeywordEntry>,
    records: Vec<u8>,
    range_tree: Option<RecordRangeTree>,
    raw_record_bytes: bool,
}

impl DictionaryBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            path: None,
            dict_type: MDictType::Mdx,
            metadata: Metadata::default(),
            entries: Vec::new(),
            records: Vec::new(),
            range_tree: None,
            raw_record_bytes: false,
        }
    }
    pub fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }
    pub fn path_opt(mut self, path: Option<PathBuf>) -> Self {
        self.path = path;
        self
    }
    pub fn dict_type(mut self, dict_type: MDictType) -> Self {
        self.dict_type = dict_type;
        self
    }
    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }
    pub fn entry(mut self, entry: MDictKeywordEntry) -> Self {
        self.entries.push(entry);
        self
    }
    pub fn entries(mut self, entries: Vec<MDictKeywordEntry>) -> Self {
        self.entries = entries;
        self
    }
    pub fn records(mut self, records: Vec<u8>) -> Self {
        self.records = records;
        self
    }
    pub fn range_tree(mut self, range_tree: RecordRangeTree) -> Self {
        self.range_tree = Some(range_tree);
        self
    }
    pub fn raw_record_bytes(mut self, raw_record_bytes: bool) -> Self {
        self.raw_record_bytes = raw_record_bytes;
        self
    }
    pub fn build(self) -> Dictionary {
        let mut dict = Dictionary {
            path: self.path,
            name: self.name,
            dict_type: self.dict_type,
            metadata: self.metadata,
            entries: self.entries,
            records: self.records,
            exact_lookup: HashMap::new(),
            comparable_lookup: HashMap::new(),
            range_tree: self.range_tree,
            raw_record_bytes: self.raw_record_bytes,
            asset_resolver: None,
        };
        dict.rebuild_lookup_maps();
        dict
    }
}
