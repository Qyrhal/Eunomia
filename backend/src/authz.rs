//! Authorization in one place. Two independent questions, both answered here:
//!
//! 1. What may this *vault role* do? ([`permits`], an exhaustive matrix over
//!    owner and member, applied by [`authorize`].)
//! 2. What may this *credential* do? A personal access token carries scopes and
//!    an optional vault restriction. The gate (`gate.rs`) puts the request's
//!    [`Caller`] in a task-local, and [`authorize`] and the vault lookups read
//!    it, so a restricted token cannot reach another vault by any path that goes
//!    through here.
//!
//! [`VaultScope`] can only be built by [`authorize`]: holding one is the proof
//! that the check ran.

use surrealdb::types::SurrealValue;
use std::future::Future;

use serde::Deserialize;
use surrealdb::types::RecordId;
use crate::rid::RecordIdExt;

use crate::pool::OrgDb;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::scopes;
use crate::store;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Owner,
    Member,
}

/// Everything a caller can ask to do to a vault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ReadMemories,
    WriteMemories,
    ListMembers,
    ManageMembers,
    Invite,
    Rename,
    Delete,
    Clone,
    Merge,
    Leave,
}

impl Action {
    pub const ALL: [Action; 10] = [
        Action::ReadMemories,
        Action::WriteMemories,
        Action::ListMembers,
        Action::ManageMembers,
        Action::Invite,
        Action::Rename,
        Action::Delete,
        Action::Clone,
        Action::Merge,
        Action::Leave,
    ];

    /// The token scope this action needs.
    pub fn scope(self) -> &'static str {
        match self {
            Action::ReadMemories | Action::ListMembers => scopes::MEMORY_READ,
            Action::WriteMemories => scopes::MEMORY_WRITE,
            Action::ManageMembers
            | Action::Invite
            | Action::Rename
            | Action::Delete
            | Action::Clone
            | Action::Merge
            | Action::Leave => scopes::VAULTS_ADMIN,
        }
    }
}

/// The role matrix. Owners administer; members read and write content and may
/// clone, merge from, list and leave a vault they belong to.
pub fn permits(role: Role, action: Action) -> bool {
    match action {
        Action::ReadMemories
        | Action::WriteMemories
        | Action::ListMembers
        | Action::Clone
        | Action::Merge
        | Action::Leave => true,
        Action::ManageMembers | Action::Invite | Action::Rename | Action::Delete => role == Role::Owner,
    }
}

/// Proof that `authorize` allowed an action on a vault. No public constructor.
#[derive(Debug, Clone)]
pub struct VaultScope {
    vault: RecordId,
    role: Role,
}

impl VaultScope {
    pub fn vault(&self) -> &RecordId {
        &self.vault
    }

    pub fn role(&self) -> Role {
        self.role
    }
}

/// Who is acting, for the audit ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    /// `user` (browser session), `token` (personal access token), `oauth` (client) or `anonymous`.
    pub kind: &'static str,
    pub id: String,
}

/// The credential behind the current request.
#[derive(Debug, Clone)]
pub struct Caller {
    pub actor: Actor,
    pub scopes: Vec<String>,
    /// A vault-restricted token may only touch this vault.
    pub vault: Option<RecordId>,
}

impl Caller {
    /// A browser session: every scope, every vault the user belongs to.
    pub fn session(user_id: &RecordId) -> Self {
        Caller { actor: Actor { kind: "user", id: user_id.to_string() }, scopes: scopes::ALL.iter().map(|s| s.to_string()).collect(), vault: None }
    }

    /// `memory:write` implies `memory:read` (see `scopes::allows`).
    pub fn allows(&self, scope: &str) -> bool {
        scopes::allows(&self.scopes, scope)
    }
}

tokio::task_local! {
    static CALLER: Caller;
}

/// Runs `fut` with `caller` as the current credential (the gate does this per request).
pub async fn with_caller<F: Future>(caller: Caller, fut: F) -> F::Output {
    CALLER.scope(caller, fut).await
}

/// The current request's credential. `None` outside a request (background
/// jobs, tests of services), which means unrestricted.
pub fn current() -> Option<Caller> {
    CALLER.try_with(Caller::clone).ok()
}

/// 403 `auth.scope` unless the current credential carries `scope`.
pub fn require_scope(scope: &str) -> AppResult<()> {
    match current() {
        Some(c) if !c.allows(scope) => {
            Err(AppError::coded(ErrorCode::AuthScope, format!("this token does not have the {scope} scope")))
        }
        _ => Ok(()),
    }
}

/// The vault the current token is restricted to, if any.
pub fn restricted_vault() -> Option<RecordId> {
    current().and_then(|c| c.vault)
}

/// 403 `auth.scope` if the current token is restricted to a vault other than `vault`.
pub fn check_vault(vault: &RecordId) -> AppResult<()> {
    match restricted_vault() {
        Some(only) if &only != vault => {
            Err(AppError::coded(ErrorCode::AuthScope, format!("this token is restricted to vault {}", only.to_string())))
        }
        _ => Ok(()),
    }
}

/// 403 `auth.scope` for a vault-restricted token. For operations that create
/// data outside the restricted vault (new, cloned or merged vaults) or are not
/// about a vault at all.
pub fn require_unrestricted() -> AppResult<()> {
    match restricted_vault() {
        Some(only) => Err(AppError::coded(ErrorCode::AuthScope, format!("this token is restricted to vault {}", only.to_string()))),
        None => Ok(()),
    }
}

#[derive(Deserialize, SurrealValue)]
struct RoleRow {
    #[serde(default)]
    #[surreal(default)]
    role: String,
}

async fn role_of(db: &OrgDb, user: &RecordId, vault: &RecordId) -> AppResult<Option<Role>> {
    let mut res = store::vaults::MEMBERSHIP_ACTIVE
        .on(db)
        .bind(("vault", vault.clone()))
        .bind(("user", user.clone()))
        .await?;
    let rows: Vec<RoleRow> = res.take(0)?;
    Ok(rows.into_iter().next().map(|r| if r.role == "owner" { Role::Owner } else { Role::Member }))
}

/// 403 unless `user` is an active member of `vault`. Not an authorization of
/// any action: only for resolving a restricted token's own vault.
pub async fn ensure_member(db: &OrgDb, user: &RecordId, vault: &RecordId) -> AppResult<()> {
    match role_of(db, user, vault).await? {
        Some(_) => Ok(()),
        None => Err(AppError::coded(ErrorCode::VaultForbidden, format!("not a member of vault {}", vault.to_string()))),
    }
}

/// The one gate for acting on a vault: the credential's scope and vault
/// restriction, then the user's active role against the matrix.
pub async fn authorize(db: &OrgDb, user: &RecordId, action: Action, vault: &RecordId) -> AppResult<VaultScope> {
    require_scope(action.scope())?;
    check_vault(vault)?;
    match role_of(db, user, vault).await? {
        Some(role) if permits(role, action) => Ok(VaultScope { vault: vault.clone(), role }),
        _ if permits(Role::Member, action) => {
            Err(AppError::coded(ErrorCode::VaultForbidden, format!("not a member of vault {}", vault.to_string())))
        }
        _ => Err(AppError::coded(ErrorCode::VaultForbidden, format!("must be an owner of vault {}", vault.to_string()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_matrix() {
        use Action::*;
        // (action, owner, member)
        let table = [
            (ReadMemories, true, true),
            (WriteMemories, true, true),
            (ListMembers, true, true),
            (ManageMembers, true, false),
            (Invite, true, false),
            (Rename, true, false),
            (Delete, true, false),
            (Clone, true, true),
            (Merge, true, true),
            (Leave, true, true),
        ];
        assert_eq!(table.len(), Action::ALL.len(), "every action is in the table");
        for (action, owner, member) in table {
            assert_eq!(permits(Role::Owner, action), owner, "owner {action:?}");
            assert_eq!(permits(Role::Member, action), member, "member {action:?}");
        }
    }

    #[test]
    fn action_scopes() {
        assert_eq!(Action::ReadMemories.scope(), scopes::MEMORY_READ);
        assert_eq!(Action::WriteMemories.scope(), scopes::MEMORY_WRITE);
        for a in [Action::Invite, Action::Rename, Action::Delete, Action::Clone, Action::Merge, Action::ManageMembers] {
            assert_eq!(a.scope(), scopes::VAULTS_ADMIN, "{a:?}");
        }
    }

    #[tokio::test]
    async fn restriction_and_scope_checks_follow_the_current_caller() {
        let v1: RecordId = crate::rid::parse("vault:one").unwrap();
        let v2: RecordId = crate::rid::parse("vault:two").unwrap();
        // no caller: unrestricted
        assert!(check_vault(&v1).is_ok() && require_scope(scopes::VAULTS_ADMIN).is_ok() && require_unrestricted().is_ok());

        let caller = Caller {
            actor: Actor { kind: "token", id: "api_token:x".into() },
            scopes: vec![scopes::MEMORY_READ.to_string()],
            vault: Some(v1.clone()),
        };
        with_caller(caller, async {
            assert!(check_vault(&v1).is_ok());
            assert_eq!(check_vault(&v2).unwrap_err().code, ErrorCode::AuthScope);
            assert!(require_scope(scopes::MEMORY_READ).is_ok());
            assert_eq!(require_scope(scopes::MEMORY_WRITE).unwrap_err().code, ErrorCode::AuthScope);
            assert_eq!(require_unrestricted().unwrap_err().code, ErrorCode::AuthScope);
        })
        .await;
    }
}
