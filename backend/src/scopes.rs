//! Token scopes: what a personal access token (or, later, an OAuth grant) may do.
//! Which scope each route and tool needs is decided in `gate.rs` and `tools::registry`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    MemoryRead,
    MemoryWrite,
    VaultsAdmin,
    Connectors,
}

impl Scope {
    pub const ALL: [Scope; 4] = [Scope::MemoryRead, Scope::MemoryWrite, Scope::VaultsAdmin, Scope::Connectors];

    pub fn as_str(self) -> &'static str {
        match self {
            Scope::MemoryRead => "memory:read",
            Scope::MemoryWrite => "memory:write",
            Scope::VaultsAdmin => "vaults:admin",
            Scope::Connectors => "connectors",
        }
    }

    pub fn parse(s: &str) -> Option<Scope> {
        Scope::ALL.into_iter().find(|sc| sc.as_str() == s)
    }
}

/// Parses a stored list; unknown names are dropped (a token never gains power from a typo).
pub fn parse_all<S: AsRef<str>>(names: &[S]) -> Vec<Scope> {
    names.iter().filter_map(|n| Scope::parse(n.as_ref())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for s in Scope::ALL {
            assert_eq!(Scope::parse(s.as_str()), Some(s));
        }
        assert_eq!(Scope::parse("memory:admin"), None);
        assert_eq!(parse_all(&["memory:read", "nope"]), vec![Scope::MemoryRead]);
    }
}
