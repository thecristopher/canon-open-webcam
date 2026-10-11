use crate::bytes::u32_at;

/// GetViewFinderData returns [length][type][bytes] blobs, and the JPEG is the one typed 1 or 11.
pub fn jpeg_from_viewfinder(blob: &[u8]) -> Option<&[u8]> {
    let mut offset = 0usize;
    while let (Some(length), Some(kind)) = (u32_at(blob, offset), u32_at(blob, offset + 4)) {
        let length = length as usize;
        if length < 8 || offset + length > blob.len() {
            return None;
        }
        if kind == 1 || kind == 11 {
            return Some(&blob[offset + 8..offset + length]);
        }
        offset += length;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::packed;

    fn record(kind: u32, body: &[u8]) -> Vec<u8> {
        let mut bytes = packed(&[(body.len() + 8) as u32, kind]);
        bytes.extend(body);
        bytes
    }

    #[test]
    fn the_jpeg_record_is_found_after_others() {
        let mut blob = record(8, &[0xAA; 4]);
        blob.extend(record(1, &[0xFF, 0xD8, 0xFF, 0xD9]));
        assert_eq!(jpeg_from_viewfinder(&blob), Some(&[0xFF, 0xD8, 0xFF, 0xD9][..]));
    }

    #[test]
    fn type_eleven_also_carries_a_jpeg() {
        assert_eq!(jpeg_from_viewfinder(&record(11, &[1, 2])), Some(&[1, 2][..]));
    }

    #[test]
    fn a_record_longer_than_the_blob_gives_nothing() {
        assert_eq!(jpeg_from_viewfinder(&packed(&[400, 1, 0])), None);
    }
}
