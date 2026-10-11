use crate::bytes::{u16_at, u32_at};

/// Pulls "Canon EOS 1500D" out of the PTP DeviceInfo dataset, since the USB name is just "Canon Digital Camera".
pub fn model_name(info: &[u8]) -> Option<String> {
    let mut reader = DatasetReader { bytes: info, offset: 0 };
    // Fixed header: standard version, vendor extension id and version, extension description, functional mode
    reader.skip(2 + 4 + 2)?;
    reader.string()?;
    reader.skip(2)?;
    // Operations, events, properties, capture formats, image formats
    for _ in 0..5 {
        reader.skip_u16_array()?;
    }
    let manufacturer = reader.string()?;
    let model = reader.string()?;
    // "Canon Inc." plus "Canon EOS Rebel T7" would say Canon twice
    let brand = manufacturer.split(' ').next().unwrap_or("").to_lowercase();
    if brand.is_empty() || model.to_lowercase().starts_with(&brand) {
        Some(model)
    } else {
        Some(format!("{manufacturer} {model}"))
    }
}

/// Walks PTP datasets, where strings are a length byte followed by UTF-16 characters.
struct DatasetReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl DatasetReader<'_> {
    fn skip(&mut self, count: usize) -> Option<()> {
        (self.offset + count <= self.bytes.len()).then(|| self.offset += count)
    }

    fn skip_u16_array(&mut self) -> Option<()> {
        let count = u32_at(self.bytes, self.offset)? as usize;
        self.skip(4 + count * 2)
    }

    fn string(&mut self) -> Option<String> {
        let count = *self.bytes.get(self.offset)? as usize;
        self.offset += 1;
        let mut units = Vec::with_capacity(count);
        for _ in 0..count {
            units.push(u16_at(self.bytes, self.offset)?);
            self.offset += 2;
        }
        // The count includes the trailing null
        let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
        Some(String::from_utf16_lossy(&units[..end]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ptp_string(text: &str) -> Vec<u8> {
        let mut units: Vec<u16> = text.encode_utf16().collect();
        units.push(0);
        let mut bytes = vec![units.len() as u8];
        bytes.extend(units.iter().flat_map(|u| u.to_le_bytes()));
        bytes
    }

    fn device_info(manufacturer: &str, model: &str) -> Vec<u8> {
        let mut info = vec![0u8; 8];
        info.extend(ptp_string("canon.com: 1.0"));
        info.extend([0, 0]);
        info.extend([1, 0, 0, 0, 0x01, 0x10]); // one operation
        for _ in 0..4 {
            info.extend([0, 0, 0, 0]);
        }
        info.extend(ptp_string(manufacturer));
        info.extend(ptp_string(model));
        info
    }

    #[test]
    fn the_brand_is_not_repeated() {
        assert_eq!(model_name(&device_info("Canon Inc.", "Canon EOS 1500D")).as_deref(), Some("Canon EOS 1500D"));
    }

    #[test]
    fn a_bare_model_gets_the_manufacturer() {
        assert_eq!(model_name(&device_info("Canon Inc.", "EOS R")).as_deref(), Some("Canon Inc. EOS R"));
    }

    #[test]
    fn a_short_dataset_gives_nothing() {
        assert_eq!(model_name(&[0; 5]), None);
    }
}
