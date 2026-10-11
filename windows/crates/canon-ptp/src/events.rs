use crate::bytes::u32_at;
use crate::codes::{CanonEvent, CanonProperty};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CameraEvent {
    PropertyChanged { property: u32, value: u32 },
    TextPropertyChanged { property: u32, text: String },
    ChoicesChanged { property: u32, choices: Vec<u32> },
}

/// Canon packs every event into one GetEvent blob of [size][type][body] records.
pub fn parse_canon_events(blob: &[u8]) -> Vec<CameraEvent> {
    let mut events = Vec::new();
    let mut offset = 0usize;
    while let (Some(size), Some(kind)) = (u32_at(blob, offset), u32_at(blob, offset + 4)) {
        let size = size as usize;
        // A zero type or a bogus size marks the end of the list
        if kind == 0 || size < 8 || offset + size > blob.len() {
            break;
        }
        if let Some(event) = parse_record(kind, &blob[offset..offset + size]) {
            events.push(event);
        }
        offset += size;
    }
    events
}

fn parse_record(kind: u32, record: &[u8]) -> Option<CameraEvent> {
    let property = u32_at(record, 8)?;
    if kind == CanonEvent::PropertyChanged as u32 {
        if record.len() == 16 {
            return Some(CameraEvent::PropertyChanged { property, value: u32_at(record, 12)? });
        }
        if property == CanonProperty::LensName.code() {
            return Some(CameraEvent::TextPropertyChanged { property, text: c_string(&record[12..]) });
        }
        None
    } else if kind == CanonEvent::AvailableValuesChanged as u32 {
        let count = u32_at(record, 16)? as usize;
        let choices = (0..count).filter_map(|i| u32_at(record, 20 + 4 * i)).collect();
        Some(CameraEvent::ChoicesChanged { property, choices })
    } else {
        None
    }
}

fn c_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::packed;

    fn record(words: &[u32]) -> Vec<u8> {
        let mut bytes = packed(&[(words.len() * 4 + 4) as u32]);
        bytes.extend(packed(words));
        bytes
    }

    fn end_of_events() -> Vec<u8> {
        packed(&[8, 0])
    }

    #[test]
    fn a_property_change_carries_its_new_value() {
        let blob = [record(&[0xC189, 0xD103, 0x58]), end_of_events()].concat();
        assert_eq!(parse_canon_events(&blob), vec![CameraEvent::PropertyChanged { property: 0xD103, value: 0x58 }]);
    }

    #[test]
    fn an_available_list_carries_every_choice() {
        let blob = [record(&[0xC18A, 0xD103, 3, 3, 0x48, 0x50, 0x58]), end_of_events()].concat();
        assert_eq!(
            parse_canon_events(&blob),
            vec![CameraEvent::ChoicesChanged { property: 0xD103, choices: vec![0x48, 0x50, 0x58] }]
        );
    }

    #[test]
    fn the_lens_name_comes_through_as_text() {
        let mut blob = packed(&[0, 0xC189, 0xD1D8]);
        blob.extend(b"Ranni's Moonlight 50mm\0\0");
        let len = blob.len() as u32;
        blob[..4].copy_from_slice(&len.to_le_bytes());
        blob.extend(end_of_events());
        assert_eq!(
            parse_canon_events(&blob),
            vec![CameraEvent::TextPropertyChanged { property: 0xD1D8, text: "Ranni's Moonlight 50mm".into() }]
        );
    }

    #[test]
    fn parsing_stops_at_the_terminator() {
        let blob = [end_of_events(), record(&[0xC189, 0xD103, 0x58])].concat();
        assert!(parse_canon_events(&blob).is_empty());
    }

    #[test]
    fn unknown_event_types_are_skipped_not_fatal() {
        let blob = [record(&[0xC1A7, 1, 2]), record(&[0xC189, 0xD101, 0x30]), end_of_events()].concat();
        assert_eq!(parse_canon_events(&blob), vec![CameraEvent::PropertyChanged { property: 0xD101, value: 0x30 }]);
    }

    #[test]
    fn a_record_claiming_more_bytes_than_exist_is_dropped() {
        assert!(parse_canon_events(&packed(&[400, 0xC189, 0xD103, 0x58])).is_empty());
    }
}
