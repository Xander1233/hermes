use std::hash::{Hash, Hasher};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NameError {
    #[error("empty label in domain name")]
    EmptyLabel,
    #[error("label exceeds 63 bytes: {0}")]
    LabelTooLong(usize),
    #[error("domain name exceeds 255 bytes: {0}")]
    NameTooLong(usize),
}

#[derive(Debug, Clone)]
pub struct Name {
    raw: Vec<u8>,
    offsets: Vec<u8>,
    is_wildcard: bool,
}

impl Name {
    pub fn new(raw: Vec<u8>) -> Self {
        if raw.is_empty() || *raw.last().unwrap() != 0 {
            panic!("Invalid DNS name: missing terminating zero");
        }

        if raw.len() > 255 {
            panic!("Invalid DNS name: exceeds maximum length of 255 bytes");
        }

        let mut offsets: Vec<u8> = Vec::new();
        let mut pos = 0;
        while pos < raw.len() {
            offsets.push(pos as u8);
            let len = raw[pos] as usize;

            if len == 0 {
                break;
            }

            pos += len + 1;
        }

        let is_wildcard = if !offsets.is_empty() {
            let first_label_offset = offsets[0] as usize;

            raw[first_label_offset] == 1 && raw[first_label_offset + 1] == b'*'
        } else {
            false
        };

        Name { raw, offsets, is_wildcard }
    }

    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    pub fn offsets(&self) -> &[u8] {
        &self.offsets
    }

    pub fn suffix_from(&self, i: usize) -> Name {
        if i >= self.offsets.len() {
            panic!("Index out of bounds for suffix_from: {}", i);
        }

        Name::new(self.raw[self.offsets[i] as usize..].to_vec())
    }

    pub fn suffix_from_reversed(&self, i: usize) -> Name {
        if i >= self.offsets.len() {
            panic!("Index out of bounds for suffix_from_reversed: {}", i);
        }

        let start_index = self.offsets.len() - 1 - i;
        Name::new(self.raw[self.offsets[start_index] as usize..].to_vec())
    }

    pub fn from_ascii(input: &str) -> Result<Name, NameError> {
        let trimmed = input.strip_suffix('.').unwrap_or(input);

        let mut raw = Vec::new();

        if !trimmed.is_empty() {
            for label in trimmed.split('.') {
                let len = label.len();

                if len == 0 {
                    return Err(NameError::EmptyLabel);
                }

                if len > 63 {
                    return Err(NameError::LabelTooLong(len));
                }

                raw.push(len as u8);
                raw.extend_from_slice(label.as_bytes());
            }
        }

        raw.push(0);

        if raw.len() > 255 {
            return Err(NameError::NameTooLong(raw.len()));
        }

        Ok(Name::new(raw))
    }

    pub fn to_display(&self) -> String {
        if self.is_root() {
            return ".".to_string();
        }

        let mut out = String::new();

        for &offset in &self.offsets {
            let offset = offset as usize;
            let len = self.raw[offset] as usize;

            if len == 0 {
                break;
            }

            for &byte in &self.raw[offset + 1..offset + 1 + len] {
                match byte {
                    b'.' | b'\\' => {
                        out.push('\\');
                        out.push(byte as char);
                    }
                    0x21..=0x7e => out.push(byte as char),
                    _ => out.push_str(&format!("\\{:03}", byte)),
                }
            }

            out.push('.');
        }
        out
    }

    pub fn is_wildcard(&self) -> bool {
        self.is_wildcard
    }

    pub fn label_count(&self) -> usize {
        self.offsets.len() - 1
    }

    pub fn is_root(&self) -> bool {
        self.label_count() == 0
    }

    pub fn parent(&self) -> Option<Name> {
        if self.is_root() {
            None
        } else {
            Some(self.suffix_from(1))
        }
    }

    pub fn is_in_zone(&self, apex: &Name) -> bool {
        if self.label_count() < apex.label_count() {
            return false;
        }

        self.suffix_from(self.label_count() - apex.label_count()).eq(apex)
    }
}

impl PartialEq for Name {
    fn eq(&self, other: &Self) -> bool {
        self.offsets.len() == other.offsets.len() && self.offsets.iter().zip(&other.offsets).all(|(&a, &b)| {
            let a = a as usize;
            let b = b as usize;
            let a_len = self.raw[a] as usize;
            let b_len = other.raw[b] as usize;

            a_len == b_len && self.raw[(a + 1)..(a + 1 + a_len)].eq_ignore_ascii_case(&other.raw[(b + 1)..(b + 1 + b_len)])
        })
    }
}

impl Eq for Name {}

impl Hash for Name {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for &offset in &self.offsets {
            let offset = offset as usize;
            let len = self.raw[offset] as usize;

            state.write_usize(len);

            for &byte in &self.raw[(offset + 1)..(offset + 1 + len)] {
                state.write_u8(byte.to_ascii_lowercase());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{encode, name};
    use std::collections::HashMap;

    #[test]
    fn new_computes_label_offsets() {
        let n = name(&["www", "example", "com"]);
        // 3www(0) 7example(4) 3com(12) 0(16)
        assert_eq!(n.offsets(), &[0, 4, 12, 16]);
    }

    #[test]
    fn new_root_has_single_offset() {
        let n = name(&[]);
        assert_eq!(n.raw(), &[0]);
        assert_eq!(n.offsets(), &[0]);
    }

    #[test]
    fn raw_returns_original_bytes() {
        let raw = encode(&["a", "b"]);
        let n = Name::new(raw.clone());
        assert_eq!(n.raw(), raw.as_slice());
    }

    #[test]
    fn suffix_from_returns_parent_name() {
        let full = name(&["www", "example", "com"]);
        assert_eq!(full.suffix_from(1), name(&["example", "com"]));
        assert_eq!(full.suffix_from(2), name(&["com"]));
        assert_eq!(full.suffix_from(3), name(&[])); // root
        assert_eq!(full.suffix_from(0), full);
    }

    #[test]
    #[should_panic]
    fn suffix_from_out_of_bounds_panics() {
        let n = name(&["com"]);
        let _ = n.suffix_from(5);
    }

    #[test]
    fn eq_is_case_insensitive() {
        assert_eq!(name(&["Example", "COM"]), name(&["example", "com"]));
    }

    #[test]
    fn eq_distinguishes_different_names() {
        assert_ne!(name(&["example", "com"]), name(&["example", "org"]));
    }

    #[test]
    fn eq_distinguishes_different_label_counts() {
        assert_ne!(name(&["example", "com"]), name(&["com"]));
    }

    #[test]
    fn hash_matches_for_case_insensitive_equal_names() {
        // Equal names must hash equal so they collide as HashMap keys.
        let mut map: HashMap<Name, u32> = HashMap::new();
        map.insert(name(&["Example", "Com"]), 42);
        assert_eq!(map.get(&name(&["example", "com"])), Some(&42));
    }

    #[test]
    fn label_count_excludes_root() {
        assert_eq!(name(&["zone", "example", "com"]).label_count(), 3);
        assert_eq!(name(&["example", "com"]).label_count(), 2);
        assert_eq!(name(&["com"]).label_count(), 1);
        assert_eq!(name(&[]).label_count(), 0);
    }

    #[test]
    fn is_root_only_for_root() {
        assert!(name(&[]).is_root());
        assert!(!name(&["com"]).is_root());
    }

    #[test]
    fn parent_strips_leftmost_label() {
        assert_eq!(name(&["www", "example", "com"]).parent(), Some(name(&["example", "com"])));
        assert_eq!(name(&["com"]).parent(), Some(name(&[])));
        assert_eq!(name(&[]).parent(), None);
    }

    #[test]
    fn is_in_zone_supports_multi_label_apex() {
        let apex = name(&["zone", "example", "com"]);
        assert!(name(&["zone", "example", "com"]).is_in_zone(&apex)); // apex itself
        assert!(name(&["www", "zone", "example", "com"]).is_in_zone(&apex));
        assert!(name(&["a", "b", "zone", "example", "com"]).is_in_zone(&apex));
        // Outside the zone.
        assert!(!name(&["example", "com"]).is_in_zone(&apex));
        assert!(!name(&["zone", "example", "org"]).is_in_zone(&apex));
        assert!(!name(&["other", "com"]).is_in_zone(&apex));
    }

    #[test]
    fn from_ascii_parses_names() {
        assert_eq!(Name::from_ascii("www.example.com").unwrap(), name(&["www", "example", "com"]));
        // Trailing dot is accepted and equivalent.
        assert_eq!(Name::from_ascii("example.com.").unwrap(), name(&["example", "com"]));
        // Empty string and "." are the root.
        assert_eq!(Name::from_ascii("").unwrap(), name(&[]));
        assert_eq!(Name::from_ascii(".").unwrap(), name(&[]));
    }

    #[test]
    fn from_ascii_rejects_invalid_names() {
        assert_eq!(Name::from_ascii("www..com"), Err(NameError::EmptyLabel));
        assert!(matches!(Name::from_ascii(&"a".repeat(64)), Err(NameError::LabelTooLong(64))));
    }

    #[test]
    fn from_ascii_roundtrips_with_to_display() {
        let n = Name::from_ascii("www.example.com").unwrap();
        assert_eq!(n.to_display(), "www.example.com.");
        assert_eq!(name(&[]).to_display(), ".");
    }
}

















