use mdx_rs::asset::normalize_asset_ref;
use mdx_rs::*;

fn sample_dict() -> Dictionary {
    let records = b"hello definitionworld definition".to_vec();
    Dictionary::builder("sample")
        .metadata(Metadata {
            title: "Sample".into(),
            description: "Demo".into(),
            creation_date: "2026-04-24".into(),
            generated_by_engine_version: "2.0".into(),
            version: "2.0".into(),
            encoding: "UTF-8".into(),
            encrypted: 0,
            register_by: String::new(),
        })
        .entry(MDictKeywordEntry::new("hello", 0, 16).unwrap())
        .entry(MDictKeywordEntry::new("world", 16, 32).unwrap())
        .records(records)
        .build()
}

fn test_adler32(data: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65_521;
    let mut a = 1u32;
    let mut b = 0u32;
    for byte in data {
        a = (a + u32::from(*byte)) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }
    (b << 16) | a
}

fn zlib_wrapped(payload: &[u8]) -> Vec<u8> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(payload).unwrap();
    let compressed = encoder.finish().unwrap();
    let mut wrapped = vec![2, 0, 0, 0];
    wrapped.extend_from_slice(&test_adler32(payload).to_be_bytes());
    wrapped.extend_from_slice(&compressed);
    wrapped
}

fn uncompressed_wrapped(payload: &[u8]) -> Vec<u8> {
    let mut wrapped = vec![0, 0, 0, 0];
    wrapped.extend_from_slice(&test_adler32(payload).to_be_bytes());
    wrapped.extend_from_slice(payload);
    wrapped
}

fn lzo_wrapped(payload: &[u8]) -> Vec<u8> {
    let compressed = lzokay::compress::compress(payload).unwrap();
    let mut wrapped = vec![1, 0, 0, 0];
    wrapped.extend_from_slice(&test_adler32(payload).to_be_bytes());
    wrapped.extend_from_slice(&compressed);
    wrapped
}

fn utf16le_bytes(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn minimal_real_mdict_bytes() -> Vec<u8> {
    let header = r#"<Dictionary Title="Real" Description="Header" Encoding="UTF-8" Encrypted="No" RegisterBy="Email" GeneratedByEngineVersion="2.0"/>"#;
    let mut raw_header = Vec::new();
    for unit in header.encode_utf16() {
        raw_header.extend_from_slice(&unit.to_le_bytes());
    }

    let mut key_block = Vec::new();
    key_block.extend_from_slice(&0u64.to_be_bytes());
    key_block.extend_from_slice(b"alpha\0");
    key_block.extend_from_slice(&3u64.to_be_bytes());
    key_block.extend_from_slice(b"beta\0");
    let key_data = uncompressed_wrapped(&key_block);

    let mut key_info_plain = Vec::new();
    key_info_plain.extend_from_slice(&2u64.to_be_bytes());
    key_info_plain.extend_from_slice(&5u16.to_be_bytes());
    key_info_plain.extend_from_slice(b"alpha\0");
    key_info_plain.extend_from_slice(&4u16.to_be_bytes());
    key_info_plain.extend_from_slice(b"beta\0");
    key_info_plain.extend_from_slice(&(key_data.len() as u64).to_be_bytes());
    key_info_plain.extend_from_slice(&(key_block.len() as u64).to_be_bytes());
    let key_info = zlib_wrapped(&key_info_plain);

    let record_payload = b"onetwo";
    let record_data = uncompressed_wrapped(record_payload);
    let mut record_info = Vec::new();
    record_info.extend_from_slice(&(record_data.len() as u64).to_be_bytes());
    record_info.extend_from_slice(&(record_payload.len() as u64).to_be_bytes());

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(raw_header.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&raw_header);
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&1u64.to_be_bytes());
    bytes.extend_from_slice(&2u64.to_be_bytes());
    bytes.extend_from_slice(&(key_info_plain.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&(key_info.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&(key_data.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes.extend_from_slice(&key_info);
    bytes.extend_from_slice(&key_data);
    bytes.extend_from_slice(&1u64.to_be_bytes());
    bytes.extend_from_slice(&2u64.to_be_bytes());
    bytes.extend_from_slice(&(record_info.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&(record_data.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&record_info);
    bytes.extend_from_slice(&record_data);
    bytes
}

fn minimal_real_mdict_bytes_with_lzo() -> Vec<u8> {
    let header = r#"<Dictionary Title="Lzo" Description="Header" Encoding="UTF-8" Encrypted="No" GeneratedByEngineVersion="2.0"/>"#;
    let mut raw_header = Vec::new();
    for unit in header.encode_utf16() {
        raw_header.extend_from_slice(&unit.to_le_bytes());
    }
    let mut key_block = Vec::new();
    key_block.extend_from_slice(&0u64.to_be_bytes());
    key_block.extend_from_slice(b"alpha\0");
    key_block.extend_from_slice(&3u64.to_be_bytes());
    key_block.extend_from_slice(b"beta\0");
    let key_data = lzo_wrapped(&key_block);
    let mut key_info_plain = Vec::new();
    key_info_plain.extend_from_slice(&2u64.to_be_bytes());
    key_info_plain.extend_from_slice(&5u16.to_be_bytes());
    key_info_plain.extend_from_slice(b"alpha\0");
    key_info_plain.extend_from_slice(&4u16.to_be_bytes());
    key_info_plain.extend_from_slice(b"beta\0");
    key_info_plain.extend_from_slice(&(key_data.len() as u64).to_be_bytes());
    key_info_plain.extend_from_slice(&(key_block.len() as u64).to_be_bytes());
    let key_info = zlib_wrapped(&key_info_plain);
    let record_payload = b"onetwo";
    let record_data = lzo_wrapped(record_payload);
    let mut record_info = Vec::new();
    record_info.extend_from_slice(&(record_data.len() as u64).to_be_bytes());
    record_info.extend_from_slice(&(record_payload.len() as u64).to_be_bytes());
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(raw_header.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&raw_header);
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&1u64.to_be_bytes());
    bytes.extend_from_slice(&2u64.to_be_bytes());
    bytes.extend_from_slice(&(key_info_plain.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&(key_info.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&(key_data.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes.extend_from_slice(&key_info);
    bytes.extend_from_slice(&key_data);
    bytes.extend_from_slice(&1u64.to_be_bytes());
    bytes.extend_from_slice(&2u64.to_be_bytes());
    bytes.extend_from_slice(&(record_info.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&(record_data.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&record_info);
    bytes.extend_from_slice(&record_data);
    bytes
}

fn minimal_real_utf16_mdict_bytes() -> Vec<u8> {
    let header = r#"<Dictionary Title="Utf16" Description="Header" Encoding="UTF-16" Encrypted="No" GeneratedByEngineVersion="2.0"/>"#;
    let mut raw_header = Vec::new();
    for unit in header.encode_utf16() {
        raw_header.extend_from_slice(&unit.to_le_bytes());
    }
    let alpha = utf16le_bytes("alpha");
    let beta = utf16le_bytes("beta");
    let mut key_block = Vec::new();
    key_block.extend_from_slice(&0u64.to_be_bytes());
    key_block.extend_from_slice(&alpha);
    key_block.extend_from_slice(&[0, 0]);
    key_block.extend_from_slice(&6u64.to_be_bytes());
    key_block.extend_from_slice(&beta);
    key_block.extend_from_slice(&[0, 0]);
    let key_data = uncompressed_wrapped(&key_block);

    let mut key_info_plain = Vec::new();
    key_info_plain.extend_from_slice(&2u64.to_be_bytes());
    key_info_plain.extend_from_slice(&5u16.to_be_bytes());
    key_info_plain.extend_from_slice(&alpha);
    key_info_plain.extend_from_slice(&[0, 0]);
    key_info_plain.extend_from_slice(&4u16.to_be_bytes());
    key_info_plain.extend_from_slice(&beta);
    key_info_plain.extend_from_slice(&[0, 0]);
    key_info_plain.extend_from_slice(&(key_data.len() as u64).to_be_bytes());
    key_info_plain.extend_from_slice(&(key_block.len() as u64).to_be_bytes());
    let key_info = zlib_wrapped(&key_info_plain);

    let mut record_payload = utf16le_bytes("one");
    record_payload.extend_from_slice(&utf16le_bytes("two"));
    let record_data = uncompressed_wrapped(&record_payload);
    let mut record_info = Vec::new();
    record_info.extend_from_slice(&(record_data.len() as u64).to_be_bytes());
    record_info.extend_from_slice(&(record_payload.len() as u64).to_be_bytes());
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(raw_header.len() as u32).to_be_bytes());
    bytes.extend_from_slice(&raw_header);
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&1u64.to_be_bytes());
    bytes.extend_from_slice(&2u64.to_be_bytes());
    bytes.extend_from_slice(&(key_info_plain.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&(key_info.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&(key_data.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes.extend_from_slice(&key_info);
    bytes.extend_from_slice(&key_data);
    bytes.extend_from_slice(&1u64.to_be_bytes());
    bytes.extend_from_slice(&2u64.to_be_bytes());
    bytes.extend_from_slice(&(record_info.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&(record_data.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&record_info);
    bytes.extend_from_slice(&record_data);
    bytes
}

#[test]
fn big_endian_readers_reject_truncated_inputs() {
    assert_eq!(binary::be_u16(&[0x12, 0x34]).unwrap(), 0x1234);
    assert_eq!(binary::be_u32(&[0, 0, 1, 0]).unwrap(), 256);
    assert_eq!(binary::be_u64(&[0, 0, 0, 0, 0, 0, 1, 0]).unwrap(), 256);
    assert!(binary::be_u32(&[1, 2, 3]).is_err());
    assert!(binary::read_meta_uint(&[0; 8], 0, 2).is_err());
}

#[test]
fn utf16_and_header_metadata_are_parsed() {
    assert_eq!(binary::decode_utf16le(&[b'H', 0, b'i', 0]).unwrap(), "Hi");
    assert!(binary::decode_utf16le(&[0]).is_err());
    let header = r#"<Dictionary Title="Oxford" Description="Desc" Encoding="UTF-16" Encrypted="Yes" RegisterBy="Email" GeneratedByEngineVersion="2.0"/>"#;
    let meta = Metadata::from_header(header);
    assert_eq!(meta.title, "Oxford");
    assert_eq!(meta.description, "Desc");
    assert_eq!(meta.encoding, "UTF-16");
    assert_eq!(meta.encrypted, 1);
    assert_eq!(meta.register_by, "Email");
}

#[test]
fn dictionary_lookup_exact_and_comparable_keys() {
    let dict = sample_dict();
    assert_eq!(dict.lookup("hello").unwrap(), b"hello definition");
    assert_eq!(
        dict.find_comparable_entry("he-llo").unwrap().keyword,
        "hello"
    );
    let err = dict.lookup("missing").unwrap_err().to_string();
    assert!(err.contains("index miss"), "unexpected error: {err}");
}

#[test]
fn range_tree_maps_offsets_and_rejects_overlaps() {
    let tree = RecordRangeTree::new(vec![
        RecordBlockInfo {
            compressed_size: 5,
            decompressed_size: 10,
            compressed_accumulator_offset: 0,
            decompressed_accumulator_offset: 0,
        },
        RecordBlockInfo {
            compressed_size: 6,
            decompressed_size: 20,
            compressed_accumulator_offset: 5,
            decompressed_accumulator_offset: 10,
        },
    ])
    .unwrap();
    assert_eq!(tree.query(0).unwrap().compressed_size, 5);
    assert_eq!(tree.query(29).unwrap().compressed_size, 6);
    assert!(tree.query(30).is_none());
    let overlap = RecordRangeTree::new(vec![
        RecordBlockInfo {
            compressed_size: 1,
            decompressed_size: 10,
            compressed_accumulator_offset: 0,
            decompressed_accumulator_offset: 0,
        },
        RecordBlockInfo {
            compressed_size: 1,
            decompressed_size: 10,
            compressed_accumulator_offset: 1,
            decompressed_accumulator_offset: 9,
        },
    ]);
    assert!(overlap.is_err());
}

#[test]
fn memory_store_supports_exact_prefix_and_fuzzy_search() {
    let dict = sample_dict();
    let info = dict.dictionary_info();
    let entries = dict.export_index();

    let mut store = MemoryIndexStore::new();
    store.put(info.clone(), entries.clone()).unwrap();
    assert_eq!(
        store
            .get_exact("sample", "hello")
            .unwrap()
            .record_end_offset,
        16
    );
    assert_eq!(
        store.prefix_search("sample", "wo", 10).unwrap()[0].keyword,
        "world"
    );
    assert!(store.get_exact("sample", "missing").is_err());

    let mut fuzzy = MemoryFuzzyIndexStore::new();
    fuzzy.put(info, entries).unwrap();
    let hits = fuzzy.search("sample", "wrl", 1).unwrap();
    assert_eq!(hits[0].entry.keyword, "world");
    assert_eq!(hits.len(), 1);
}

#[test]
fn asset_resolver_normalizes_refs_and_rejects_traversal() {
    assert_eq!(
        normalize_asset_ref("/images/icon.png").unwrap(),
        r"images\icon.png"
    );
    assert!(normalize_asset_ref("../secret.txt").is_err());
    let source = MemoryAssetSource::new().insert(r"images\icon.png", b"png".to_vec());
    let resolver = AssetResolver::new().with_source(source);
    assert_eq!(resolver.read("images/icon.png").unwrap(), b"png");
    assert!(resolver.read("missing.png").is_err());
}

#[test]
fn extract_resource_refs_ignores_external_and_data_urls() {
    let refs = extract_resource_refs(br##"<a href="entry://ability">entry</a><a href="#frag">frag</a><link href="style.css"><img src='images/a.png'><img src="https://example.com/x.png"><img src="data:image/png;base64,abc"> sound://audio/uk/a.spx"##);
    assert_eq!(
        refs,
        vec![
            "images/a.png".to_string(),
            "style.css".to_string(),
            "sound://audio/uk/a.spx".to_string(),
        ]
    );
}

#[test]
fn synthetic_dictionary_bytes_round_trip() {
    let mut bytes = b"MDXRS\0".to_vec();
    bytes.extend_from_slice(&2u32.to_be_bytes());
    bytes.push(5);
    bytes.extend_from_slice(b"alpha");
    bytes.extend_from_slice(&3u32.to_be_bytes());
    bytes.extend_from_slice(b"one");
    bytes.push(4);
    bytes.extend_from_slice(b"beta");
    bytes.extend_from_slice(&3u32.to_be_bytes());
    bytes.extend_from_slice(b"two");
    let dict = Dictionary::from_bytes("fixture", None, &bytes).unwrap();
    assert_eq!(dict.lookup("alpha").unwrap(), b"one");
    assert_eq!(dict.lookup("beta").unwrap(), b"two");
}

#[test]
fn lookup_follows_entry_redirects_and_preserves_first_duplicate() {
    let records = b"@@LINK=betafirstsecondtarget".to_vec();
    let dict = Dictionary::builder("redirects")
        .entry(MDictKeywordEntry::new("alpha", 0, 11).unwrap())
        .entry(MDictKeywordEntry::new("dup", 11, 16).unwrap())
        .entry(MDictKeywordEntry::new("dup", 16, 22).unwrap())
        .entry(MDictKeywordEntry::new("beta", 22, 28).unwrap())
        .records(records)
        .build();
    assert_eq!(dict.lookup("alpha").unwrap(), b"target");
    assert_eq!(dict.lookup("dup").unwrap(), b"first");

    let mut store = MemoryIndexStore::new();
    store
        .put(dict.dictionary_info(), dict.export_index())
        .unwrap();
    assert_eq!(
        store
            .get_exact("redirects", "dup")
            .unwrap()
            .record_end_offset,
        16
    );
}

#[test]
fn resolver_supports_resource_schemes_candidates_and_utf16_redirects() {
    let redirect: Vec<u8> = "@@@LINK=snd://audio/real.spx"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let source = MemoryAssetSource::new()
        .insert(r"audio\redirect.spx", redirect)
        .insert(r"audio\real.spx", b"sound".to_vec());
    let resolver = AssetResolver::new().with_source(source);
    assert_eq!(
        resolver.read("sound://audio/redirect.spx").unwrap(),
        b"sound"
    );
}

#[test]
fn library_registers_and_opens_in_memory_dictionaries() {
    let mut library = DictionaryLibrary::new();
    library
        .register_dictionary("sample", sample_dict())
        .unwrap();
    assert_eq!(
        library.open("sample").unwrap().lookup("world").unwrap(),
        b"world definition"
    );
    assert!(library.open("missing").is_err());
}

#[test]
fn real_mdict_bytes_parse_metadata_index_and_records() {
    let bytes = minimal_real_mdict_bytes();
    let meta = mdx_rs::dictionary::metadata_from_mdict_header_bytes(&bytes).unwrap();
    assert_eq!(meta.title, "Real");
    assert_eq!(meta.description, "Header");
    assert_eq!(meta.register_by, "Email");
    let dict = Dictionary::from_bytes("real", None, &bytes).unwrap();
    assert_eq!(dict.title(), "Real");
    assert_eq!(dict.lookup("alpha").unwrap(), b"one");
    assert_eq!(dict.lookup("beta").unwrap(), b"two");

    let err = match Dictionary::from_bytes("bad", None, b"not-mdict") {
        Ok(_) => panic!("invalid MDict bytes must not parse successfully"),
        Err(err) => err.to_string(),
    };
    assert!(
        err.contains("truncated") || err.contains("exceeds") || err.contains("UTF-16"),
        "unexpected error: {err}"
    );
}

#[test]
fn real_utf16_mdict_slices_before_decoding_records() {
    let dict = Dictionary::from_bytes("utf16", None, &minimal_real_utf16_mdict_bytes()).unwrap();
    assert_eq!(dict.lookup("alpha").unwrap(), b"one");
    assert_eq!(dict.lookup("beta").unwrap(), b"two");
}

#[test]
fn real_lzo_blocks_are_decoded_for_keys_and_records() {
    let dict = Dictionary::from_bytes("lzo", None, &minimal_real_mdict_bytes_with_lzo()).unwrap();
    assert_eq!(dict.lookup("alpha").unwrap(), b"one");
    assert_eq!(dict.lookup("beta").unwrap(), b"two");
}

#[test]
fn library_opens_disk_backed_synthetic_dictionary() {
    let dir = std::env::temp_dir().join(format!("mdx-rs-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fixture.mdx");
    std::fs::write(&path, minimal_real_mdict_bytes()).unwrap();
    let mut library = DictionaryLibrary::new();
    library
        .register(DictionarySpec::new("fixture", &path))
        .unwrap();
    assert_eq!(
        library.open("fixture").unwrap().lookup("alpha").unwrap(),
        b"one"
    );
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(dir).unwrap();
}

#[test]
#[ignore = "requires MDX_RS_OALE9_FIXTURE to point to a local OALE9 .mdx fixture"]
fn external_oale9_fixture_opens_and_looks_up_words() {
    let Ok(path) = std::env::var("MDX_RS_OALE9_FIXTURE") else {
        eprintln!("skipping external fixture test; set MDX_RS_OALE9_FIXTURE to run it");
        return;
    };
    let dict = Dictionary::open(path).unwrap();
    assert!(dict.get_keyword_entries().len() > 200_000);
    assert!(dict.lookup("ability").unwrap().len() > 1_000);
    assert!(dict.lookup("hello").unwrap().len() > 1_000);
}
