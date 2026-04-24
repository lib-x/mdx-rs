//! Rust port of the core `mdx` Go library APIs.
//!
//! The crate provides typed metadata, index/store abstractions, lookup helpers,
//! asset resolution, and low-level MDict parsing utilities. The public surface is
//! intentionally fallible: malformed files, missing entries, and unsafe asset
//! paths are reported as [`MdxError`] instead of being hidden by defaults.

pub mod asset;
pub mod binary;
pub mod dictionary;
pub mod error;
pub mod index;
pub mod library;
pub mod range_tree;
pub mod store;

pub use asset::{
    extract_resource_refs, AssetResolver, AssetSource, DictionaryAssetSource, FileAssetSource,
    MemoryAssetSource,
};
pub use dictionary::{
    Dictionary, DictionaryBuilder, DictionaryInfo, MDictKeywordEntry, MDictKeywordIndex, MDictType,
    Metadata,
};
pub use error::{MdxError, Result};
pub use index::{normalize_comparable_key, SearchHit};
pub use library::{DictionaryLibrary, DictionarySpec};
pub use range_tree::{RecordBlockInfo, RecordRangeTree};
pub use store::{FuzzyIndexStore, IndexStore, MemoryFuzzyIndexStore, MemoryIndexStore};
