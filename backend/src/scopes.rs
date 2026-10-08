//! Scope names shared by OAuth access tokens and personal access tokens.
//! One place so the two credential types can never drift apart.

pub const MEMORY_READ: &str = "memory:read";
pub const MEMORY_WRITE: &str = "memory:write";
pub const VAULTS_ADMIN: &str = "vaults:admin";
pub const CONNECTORS: &str = "connectors";

pub const ALL: &[&str] = &[MEMORY_READ, MEMORY_WRITE, VAULTS_ADMIN, CONNECTORS];

/// Granted when a client asks for no scope: day-to-day memory use, nothing administrative.
pub const DEFAULT: &[&str] = &[MEMORY_READ, MEMORY_WRITE];

pub fn is_known(scope: &str) -> bool {
    ALL.contains(&scope)
}

/// Plain-words description for the consent screen.
pub fn describe(scope: &str) -> &'static str {
    match scope {
        MEMORY_READ => "Read your memories, entities and synced records",
        MEMORY_WRITE => "Add, edit and delete memories and entities",
        VAULTS_ADMIN => "Create, share, merge and delete vaults",
        CONNECTORS => "Manage connected sources",
        _ => "",
    }
}

/// Whether a granted scope list covers `required`. `memory:write` implies `memory:read`.
pub fn allows(granted: &[String], required: &str) -> bool {
    granted.iter().any(|g| g == required) || (required == MEMORY_READ && granted.iter().any(|g| g == MEMORY_WRITE))
}

/// The scope an MCP tool call needs: vault tools that change membership or
/// existence need `vaults:admin`, other mutating tools `memory:write`, reads `memory:read`.
pub fn for_tool(name: &str, read_only: bool) -> &'static str {
    if read_only {
        MEMORY_READ
    } else if name.starts_with("vault_") {
        VAULTS_ADMIN
    } else {
        MEMORY_WRITE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_implies_read_but_not_admin() {
        let g = vec![MEMORY_WRITE.to_string()];
        assert!(allows(&g, MEMORY_READ) && allows(&g, MEMORY_WRITE) && !allows(&g, VAULTS_ADMIN));
        assert!(!allows(&[MEMORY_READ.to_string()], MEMORY_WRITE));
    }

    #[test]
    fn tools_map_to_scopes() {
        assert_eq!(for_tool("recall", true), MEMORY_READ);
        assert_eq!(for_tool("memory_write", false), MEMORY_WRITE);
        assert_eq!(for_tool("vault_delete", false), VAULTS_ADMIN);
    }
}
