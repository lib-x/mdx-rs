# mdx-rs

Rust port of the core parser and lookup path from the Go `mdx` library.

## Implemented

- Real `.mdx` / `.mdd` header parsing and metadata extraction.
- Key block metadata, key block info, key entries, record block metadata, record block info, and record data parsing.
- UTF-8 and UTF-16 dictionary text handling.
- MDD resource key parsing and raw resource data resolution.
- Uncompressed, zlib-compressed, and LZO-compressed key/record blocks.
- Exact/comparable lookup, `@@LINK=` entry redirects, asset lookup candidates, resource schemes, and UTF-16 `@@@LINK=` asset redirects.
- In-memory index/fuzzy stores, range mapping, and dictionary-library registration with sidecar/MDD asset resolver wiring.

## Quick start

```rust
use mdx_rs::Dictionary;

fn main() -> mdx_rs::Result<()> {
    let mut dict = Dictionary::open("/path/to/dictionary.mdx")?;
    dict.build_index()?;

    let definition = dict.lookup("ability")?;
    println!("{} bytes", definition.len());
    Ok(())
}
```

## Companion MDD assets

```rust
use mdx_rs::{DictionaryLibrary, DictionarySpec};

fn main() -> mdx_rs::Result<()> {
    let mut library = DictionaryLibrary::new();
    library.register(
        DictionarySpec::new("oale9", "/path/to/dictionary.mdx")
            .with_mdd("/path/to/dictionary.mdd"),
    )?;

    let dict = library.open("oale9")?;
    let resolver = dict.asset_resolver().expect("library installs an asset resolver");
    let bytes = resolver.read("sound://audio/uk/example.spx")?;
    println!("{} bytes", bytes.len());
    Ok(())
}
```

## Explicit current limits

- Encrypted dictionaries return `MdxError::Unsupported`.
- The parser eagerly reads record blocks into memory; UTF-16 MDX entries are decoded after entry offset slicing, but it is not yet the Go implementation's lazy/on-demand record fetch path.
- HTTP serving, full filesystem adapter parity, Redis lifecycle management, and all Go auxiliary APIs are not yet ported.

## Verification

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

An ignored local-fixture regression test is available when you provide a compatible OALE9 `.mdx` fixture via an environment variable:

```bash
MDX_RS_OALE9_FIXTURE=/path/to/oale9.mdx cargo test external_oale9_fixture_opens_and_looks_up_words -- --ignored
```
