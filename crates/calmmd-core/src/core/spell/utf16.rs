//! Byte offsets from zspell → UTF-16 code-unit offsets for CodeMirror.

/// Converts a UTF-8 byte offset in `text` to a UTF-16 code-unit offset.
/// Out-of-range offsets clamp to the end of the text so a bad dictionary index can never panic.
pub fn byte_to_utf16(text: &str, byte_index: usize) -> u32 {
    let prefix = text.get(..byte_index).unwrap_or(text);
    prefix.encode_utf16().count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accounts_for_multibyte_characters() {
        // "Café" has a 2-byte é (one UTF-16 unit); 😀 is 4 bytes but two UTF-16 units.
        let text = "Café 😀 recieve";
        let bytes = text.find("recieve").unwrap();
        let units: u32 = text[..bytes].encode_utf16().count() as u32;
        assert_eq!(byte_to_utf16(text, bytes), units);
        assert_ne!(bytes as u32, units);
    }

    #[test]
    fn clamps_out_of_range_offsets() {
        let text = "abc";
        assert_eq!(byte_to_utf16(text, 4), 3);
    }
}
