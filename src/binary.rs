use crate::{MdxError, Result};

pub fn be_u8(bytes: &[u8]) -> Result<u8> {
    bytes
        .first()
        .copied()
        .ok_or_else(|| MdxError::InvalidFormat("expected 1 byte".into()))
}

pub fn be_u16(bytes: &[u8]) -> Result<u16> {
    let arr: [u8; 2] = bytes
        .get(..2)
        .ok_or_else(|| MdxError::InvalidFormat("expected 2 bytes".into()))?
        .try_into()
        .expect("slice length checked");
    Ok(u16::from_be_bytes(arr))
}

pub fn be_u32(bytes: &[u8]) -> Result<u32> {
    let arr: [u8; 4] = bytes
        .get(..4)
        .ok_or_else(|| MdxError::InvalidFormat("expected 4 bytes".into()))?
        .try_into()
        .expect("slice length checked");
    Ok(u32::from_be_bytes(arr))
}

pub fn be_u64(bytes: &[u8]) -> Result<u64> {
    let arr: [u8; 8] = bytes
        .get(..8)
        .ok_or_else(|| MdxError::InvalidFormat("expected 8 bytes".into()))?
        .try_into()
        .expect("slice length checked");
    Ok(u64::from_be_bytes(arr))
}

pub fn read_meta_uint(buf: &[u8], offset: usize, number_width: usize) -> Result<u64> {
    let end = offset
        .checked_add(number_width)
        .ok_or_else(|| MdxError::InvalidFormat("metadata integer offset overflow".into()))?;
    let window = buf.get(offset..end).ok_or_else(|| MdxError::InvalidFormat(format!("metadata integer at offset {offset} with width {number_width} exceeds buffer length {}", buf.len())))?;
    match number_width {
        4 => be_u32(window).map(u64::from),
        8 => be_u64(window),
        _ => Err(MdxError::InvalidFormat(format!(
            "unsupported metadata integer width {number_width}"
        ))),
    }
}

pub fn decode_utf16le(raw: &[u8]) -> Result<String> {
    if !raw.len().is_multiple_of(2) {
        return Err(MdxError::InvalidFormat(
            "UTF-16LE data must contain an even number of bytes".into(),
        ));
    }
    let units: Vec<u16> = raw
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&units)
        .map_err(|err| MdxError::InvalidFormat(format!("invalid UTF-16LE: {err}")))
}

pub fn decode_utf8(raw: &[u8]) -> Result<String> {
    std::str::from_utf8(raw)
        .map(str::to_owned)
        .map_err(|err| MdxError::InvalidFormat(format!("invalid UTF-8: {err}")))
}

pub fn read_slice(buf: &[u8], start: usize, len: usize) -> Result<&[u8]> {
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

pub fn parse_header_attributes(header: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let mut rest = header;
    while let Some(eq) = rest.find('=') {
        let before = &rest[..eq];
        let key_start = before
            .rfind(|c: char| c.is_whitespace() || c == '<')
            .map_or(0, |idx| idx + 1);
        let key = before[key_start..].trim();
        let after = rest[eq + 1..].trim_start();
        let mut chars = after.chars();
        let Some(quote @ ('"' | '\'')) = chars.next() else {
            break;
        };
        let value_start = quote.len_utf8();
        let Some(value_end) = after[value_start..].find(quote) else {
            break;
        };
        if !key.is_empty() {
            attrs.push((
                key.to_string(),
                after[value_start..value_start + value_end].to_string(),
            ));
        }
        rest = &after[value_start + value_end + quote.len_utf8()..];
    }
    attrs
}

pub fn header_attr(header: &str, name: &str) -> Option<String> {
    parse_header_attributes(header)
        .into_iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value)
}
