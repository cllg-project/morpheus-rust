use bitflags::bitflags;

bitflags! {
    /// Dialect bitmask (mirrors C `Dialect` short from dialect.h).
    /// `Dialect::empty()` = ALL_DIAL (no restriction) — same semantics as C `ALL_DIAL = 0`.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
    pub struct Dialect: u16 {
        const ATTIC           = 0o0002;
        const IONIC           = 0o0010;
        const AEOLIC          = 0o0020;
        const LESBIAN         = 0o0040;
        const HOMERIC         = 0o0100;
        const DORIC           = 0o0200;
        const PARADIGM        = 0o0400;
        const NON_HOMERIC_EPIC = 0o2000;
        const PROSE           = 0o4000;
        const EPIC            = Self::NON_HOMERIC_EPIC.bits() | Self::HOMERIC.bits();
        const RHO_ETA_DIAL    = Self::EPIC.bits() | Self::IONIC.bits();
        const RHO_ALPHA_DIAL  = Self::ATTIC.bits() | Self::DORIC.bits() | Self::AEOLIC.bits();
    }
}

impl Dialect {
    /// Returns true if there is no dialect restriction (ALL_DIAL = 0).
    #[inline]
    pub fn is_any(&self) -> bool {
        self.is_empty()
    }

    /// Two dialect sets are compatible if either is unrestricted or they share a dialect.
    #[inline]
    pub fn compatible_with(&self, other: Dialect) -> bool {
        self.is_empty() || other.is_empty() || self.intersects(other)
    }

    /// Intersect dialects, keeping unrestricted (empty) semantics:
    /// if one side is unrestricted, adopt the other side.
    pub fn and_dialect(&mut self, other: Dialect) {
        if self.is_empty() {
            *self = other;
        } else if !other.is_empty() {
            *self &= other;
        }
    }

    /// Parse one dialect name (as accepted by the CLI `-d` flag and the
    /// Python `dialects=` kwarg). `epic` covers both Homeric and later epic.
    pub fn parse_name(name: &str) -> Option<Dialect> {
        Some(match name.trim().to_lowercase().as_str() {
            "attic"   => Dialect::ATTIC,
            "ionic"   => Dialect::IONIC,
            "aeolic"  => Dialect::AEOLIC,
            "lesbian" => Dialect::LESBIAN,
            "doric"   => Dialect::DORIC,
            "homeric" => Dialect::HOMERIC,
            "epic"    => Dialect::EPIC,
            "prose"   => Dialect::PROSE,
            _ => return None,
        })
    }

    /// Parse a comma/space-separated list of dialect names into one mask.
    pub fn from_names(names: &str) -> Result<Dialect, String> {
        let mut mask = Dialect::empty();
        for name in names.split([',', ' ']).filter(|s| !s.is_empty()) {
            mask |= Self::parse_name(name)
                .ok_or_else(|| format!("unknown dialect '{name}'"))?;
        }
        Ok(mask)
    }

    /// Human-readable dialect names (same spellings as the Python bindings).
    pub fn names(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.contains(Dialect::ATTIC)            { out.push("attic"); }
        if self.contains(Dialect::IONIC)            { out.push("ionic"); }
        if self.contains(Dialect::AEOLIC)           { out.push("aeolic"); }
        if self.contains(Dialect::LESBIAN)          { out.push("lesbian"); }
        if self.contains(Dialect::DORIC)            { out.push("doric"); }
        if self.contains(Dialect::HOMERIC)          { out.push("homeric"); }
        if self.contains(Dialect::NON_HOMERIC_EPIC) { out.push("epic"); }
        out
    }
}
