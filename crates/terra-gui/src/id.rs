//! Stable widget identifiers for hot/active tracking.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Id(pub u64);

impl Id {
    pub fn new(label: &str) -> Self {
        Self(hash_str(label))
    }

    pub fn with(self, index: u64) -> Self {
        Self(self.0 ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }

    pub fn child(self, label: &str) -> Self {
        Self(self.0 ^ hash_str(label).wrapping_mul(0xC2B2_AE3D_27D4_EB4F))
    }
}

fn hash_str(s: &str) -> u64 {
    // FNV-1a 64-bit
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_differ_by_label() {
        assert_ne!(Id::new("a"), Id::new("b"));
        assert_eq!(Id::new("slider"), Id::new("slider"));
        assert_ne!(Id::new("item").with(0), Id::new("item").with(1));
    }
}
