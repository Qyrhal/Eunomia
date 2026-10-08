//! Vaults: a knowledge scope (person/organisation/location/... + memory rows)
//! that more than one user can belong to. Access is membership, not a single
//! `owner` field -- a personal vault, a team's org vault, and "share my
//! stuff with Bob" are all the same `vault` + `vault_member` primitive (see
//! `db.rs`'s schema statements). Two roles only: "owner" (can invite/remove
//! members, rename, delete) and "member" (read/write content). The vault's
//! creator is its first owner; anyone who invites must already be an owner,
//! so admin rights only ever come from creating a vault or being promoted by
//! an existing owner -- no separate "who can invite" check needed.
//!
//! Ported from `vaults/service.py`. Callers resolve a path/user-supplied
//! vault id into a `RecordId` before calling in (see `routers/vaults.rs`),
//! unlike the Python version's `_as_rid` which accepted either -- everything
//! here already deals in `RecordId`.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use surrealdb::{Datetime, RecordId};

use crate::authz::{self, Action};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::store;
use crate::tx::with_retry;

/// Entity tables a vault's data can live in -- mirrors `entities/service.py`'s
/// `KINDS` tuple and `db.rs`'s `memory.subject` record union.
const ENTITY_KINDS: [&str; 6] = ["person", "organisation", "location", "repository", "file", "symbol"];

#[derive(Debug, Deserialize)]
struct VaultRow {
    id: RecordId,
}

#[derive(Debug, Deserialize)]
struct VaultFullRow {
    id: RecordId,
    #[serde(default)]
    name: String,
    #[serde(default = "default_kind")]
    kind: String,
    created_at: Option<Datetime>,
}

fn default_kind() -> String {
    "personal".to_string()
}

impl From<VaultFullRow> for VaultOut {
    fn from(r: VaultFullRow) -> Self {
        VaultOut { id: r.id.to_string(), name: r.name, kind: r.kind, created_at: r.created_at }
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct VaultOut {
    pub id: String,
    pub name: String,
    pub kind: String,
    #[schema(value_type = Option<String>)]
    pub created_at: Option<Datetime>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct VaultWithRole {
    pub id: String,
    pub name: String,
    pub kind: String,
    #[schema(value_type = Option<String>)]
    pub created_at: Option<Datetime>,
    pub role: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct InviteOut {
    pub vault_id: String,
    pub user_email: String,
    pub role: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MemberOut {
    pub email: String,
    pub role: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct InvitationOut {
    pub vault_id: String,
    pub vault_name: String,
    pub vault_kind: String,
    pub role: String,
    #[schema(value_type = Option<String>)]
    pub created_at: Option<Datetime>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CloneOut {
    pub id: String,
    pub name: String,
    pub kind: String,
    #[schema(value_type = Option<String>)]
    pub created_at: Option<Datetime>,
    pub entities_copied: usize,
}

#[derive(Debug, Deserialize)]
struct MembershipRow {
    id: RecordId,
    #[serde(default)]
    role: String,
    #[serde(default)]
    status: String,
}

#[derive(Debug, Deserialize)]
struct EmailLookupRow {
    id: RecordId,
}

#[derive(Debug, Deserialize)]
struct CountRow {
    count: i64,
}

/// Called once, at registration. Every user has exactly one `kind="personal"`
/// vault -- it's never shared by anyone else joining it as an owner-less
/// member, only ever by the user inviting others into it.
pub async fn create_personal_vault(db: &Db, user_id: &RecordId) -> AppResult<RecordId> {
    // Deterministic id: a second personal vault for the same user collides on
    // the key, so "one personal vault per user" holds even under a race.
    let vault_id = RecordId::from_table_key("vault", crate::tx::stable_key('p', &user_id.to_string()));
    with_retry(|| async {
        store::vaults::CREATE_PERSONAL.on(db)
        .bind(("vault", vault_id.clone()))
        .bind(("user", user_id.clone()))
        .await?
        .check()
    })
    .await?;
    Ok(vault_id)
}

/// Create a new vault (org, or an extra personal-style one -- a user can have
/// several, per the product ask); creator becomes its owner.
pub async fn create_vault(db: &Db, user_id: &RecordId, name: &str, kind: &str) -> AppResult<VaultOut> {
    authz::require_unrestricted()?;
    let vault = with_retry(|| async {
        let mut res = store::vaults::CREATE_VAULT
            .on(db)
            .bind(("name", name.to_string()))
            .bind(("kind", kind.to_string()))
            .bind(("user", user_id.clone()))
            .await?
            .check()?;
        let rows: Vec<VaultFullRow> = res.take(0)?;
        Ok(rows.into_iter().next())
    })
    .await?
    .ok_or_else(|| AppError::internal("vault insert returned no row"))?;
    Ok(vault.into())
}

/// Active membership only -- a pending invite isn't membership yet.
async fn membership(db: &Db, vault_id: &RecordId, user_id: &RecordId) -> AppResult<Option<MembershipRow>> {
    let mut res = store::vaults::MEMBERSHIP_ACTIVE.on(db)
        .bind(("vault", vault_id.clone()))
        .bind(("user", user_id.clone()))
        .await?;
    let rows: Vec<MembershipRow> = res.take(0)?;
    Ok(rows.into_iter().next())
}

/// Any `vault_member` row regardless of status -- used only where a pending
/// invite also needs to count (duplicate-invite checks, accept/decline).
async fn membership_any_status(db: &Db, vault_id: &RecordId, user_id: &RecordId) -> AppResult<Option<MembershipRow>> {
    let mut res = store::vaults::MEMBERSHIP_ANY.on(db)
        .bind(("vault", vault_id.clone()))
        .bind(("user", user_id.clone()))
        .await?;
    let rows: Vec<MembershipRow> = res.take(0)?;
    Ok(rows.into_iter().next())
}

/// Every vault `user_id` belongs to (any role) -- the read/write scope passed
/// to entity/recall lookups.
pub async fn accessible_vault_ids(db: &Db, user_id: &RecordId) -> AppResult<Vec<RecordId>> {
    #[derive(Deserialize)]
    struct Row {
        vault: RecordId,
    }
    let mut res = store::vaults::ACCESSIBLE_IDS.on(db)
        .bind(("user", user_id.clone()))
        .await?;
    let rows: Vec<Row> = res.take(0)?;
    let only = authz::restricted_vault();
    Ok(rows.into_iter().map(|r| r.vault).filter(|v| only.as_ref().is_none_or(|o| o == v)).collect())
}

/// `user_id`'s personal vault -- the implicit scope for any tool call that
/// doesn't pass `vault_id`, so existing single-user callers need no changes.
pub async fn default_vault_id(db: &Db, user_id: &RecordId) -> AppResult<RecordId> {
    // a vault-restricted token's default scope is its vault, not the personal one
    if let Some(only) = authz::restricted_vault() {
        authz::ensure_member(db, user_id, &only).await?;
        return Ok(only);
    }
    personal_vault_id(db, user_id).await
}

/// The user's own personal vault, whatever the credential is restricted to. The
/// only vault that also draws on the user's synced source records.
pub async fn personal_vault_id(db: &Db, user_id: &RecordId) -> AppResult<RecordId> {
    #[derive(Deserialize)]
    struct Row {
        vault: RecordId,
    }
    let mut res = store::vaults::DEFAULT_ID.on(db)
        .bind(("user", user_id.clone()))
        .await?;
    let rows: Vec<Row> = res.take(0)?;
    rows.into_iter().next().map(|r| r.vault).ok_or_else(|| {
        AppError::internal(format!("user {user_id} has no personal vault -- registration should have created one"))
    })
}

pub async fn list_my_vaults(db: &Db, user_id: &RecordId) -> AppResult<Vec<VaultWithRole>> {
    #[derive(Deserialize)]
    struct Row {
        vault: VaultFullRow,
        role: String,
    }
    let mut res = store::vaults::LIST_MINE.on(db)
        .bind(("user", user_id.clone()))
        .await?;
    let rows: Vec<Row> = res.take(0)?;
    let only = authz::restricted_vault();
    Ok(rows
        .into_iter()
        .filter(|r| only.as_ref().is_none_or(|o| o == &r.vault.id))
        .map(|r| {
            let v: VaultOut = r.vault.into();
            VaultWithRole { id: v.id, name: v.name, kind: v.kind, created_at: v.created_at, role: r.role }
        })
        .collect())
}

pub async fn rename_vault(db: &Db, user_id: &RecordId, vault_id: &RecordId, name: &str) -> AppResult<VaultOut> {
    authz::authorize(db, user_id, Action::Rename, vault_id).await?;
    let mut res = store::vaults::RENAME.on(db)
        .bind(("id", vault_id.clone()))
        .bind(("name", name.to_string()))
        .await?;
    let rows: Vec<VaultFullRow> = res.take(0)?;
    let vault = rows.into_iter().next().ok_or_else(|| AppError::internal("vault update returned no row"))?;
    Ok(vault.into())
}

/// Owner-only. Deletes memberships; leaves any person/organisation/.../memory
/// rows already in the vault in place (orphaned-but-inaccessible, same
/// accepted tradeoff as the rest of this codebase's delete paths -- add a
/// cascade if dangling vault data ever becomes a real problem).
pub async fn delete_vault(db: &Db, user_id: &RecordId, vault_id: &RecordId) -> AppResult<()> {
    authz::authorize(db, user_id, Action::Delete, vault_id).await?;
    with_retry(|| async {
        store::vaults::DELETE_VAULT.on(db)
            .bind(("vault", vault_id.clone()))
            .await?
            .check()
    })
    .await?;
    Ok(())
}

/// Add `email` to `vault_id`. Caller must already be an owner (so admin
/// rights only ever trace back to "created it" or "an existing owner
/// promoted me" -- never self-granted).
pub async fn invite_member(
    db: &Db,
    user_id: &RecordId,
    vault_id: &RecordId,
    email: &str,
    role: &str,
) -> AppResult<InviteOut> {
    authz::authorize(db, user_id, Action::Invite, vault_id).await?;

    let mut res = store::vaults::USER_ID_BY_EMAIL.on(db)
        .bind(("email", crate::models_user::normalize_email(email)))
        .await?;
    let rows: Vec<EmailLookupRow> = res.take(0)?;
    let invitee = rows.into_iter().next().ok_or_else(|| AppError::bad_request(format!("no user with email '{email}'")))?.id;

    if membership_any_status(db, vault_id, &invitee).await?.is_some() {
        return Err(AppError::bad_request(format!("{email} is already a member or has a pending invite")));
    }

    let mut res = store::vaults::INVITE.on(db)
        .bind(("vault", vault_id.clone()))
        .bind(("user", invitee))
        .bind(("role", role.to_string()))
        .await?
        .check()
        .map_err(|e| {
            // lost the race against a concurrent invite of the same user
            if e.to_string().contains("already contains") {
                AppError::bad_request(format!("{email} is already a member or has a pending invite"))
            } else {
                e.into()
            }
        })?;
    let rows: Vec<MembershipRow> = res.take(0)?;
    let member = rows.into_iter().next().ok_or_else(|| AppError::internal("vault_member insert returned no row"))?;

    Ok(InviteOut { vault_id: vault_id.to_string(), user_email: email.to_string(), role: member.role })
}

pub async fn list_members(db: &Db, user_id: &RecordId, vault_id: &RecordId) -> AppResult<Vec<MemberOut>> {
    authz::authorize(db, user_id, Action::ListMembers, vault_id).await?;
    #[derive(Deserialize)]
    struct Row {
        email: String,
        role: String,
    }
    let mut res = store::vaults::LIST_MEMBERS.on(db)
        .bind(("vault", vault_id.clone()))
        .await?;
    let rows: Vec<Row> = res.take(0)?;
    Ok(rows.into_iter().map(|r| MemberOut { email: r.email, role: r.role }).collect())
}

/// Pending invitations for `user_id` across every vault -- what the vaults
/// page's invitations panel renders to accept/decline.
pub async fn list_my_invitations(db: &Db, user_id: &RecordId) -> AppResult<Vec<InvitationOut>> {
    #[derive(Deserialize)]
    struct Row {
        vault: VaultFullRow,
        role: String,
        created_at: Option<Datetime>,
    }
    let mut res = store::vaults::LIST_INVITATIONS.on(db)
        .bind(("user", user_id.clone()))
        .await?;
    let rows: Vec<Row> = res.take(0)?;
    let only = authz::restricted_vault();
    Ok(rows
        .into_iter()
        .filter(|r| only.as_ref().is_none_or(|o| o == &r.vault.id))
        .map(|r| InvitationOut {
            vault_id: r.vault.id.to_string(),
            vault_name: r.vault.name,
            vault_kind: r.vault.kind,
            role: r.role,
            created_at: r.created_at,
        })
        .collect())
}

/// Accept a pending invitation into `vault_id`. Only the invitee can accept
/// their own invite.
pub async fn accept_invitation(db: &Db, user_id: &RecordId, vault_id: &RecordId) -> AppResult<VaultWithRole> {
    authz::check_vault(vault_id)?;
    let m = membership_any_status(db, vault_id, user_id)
        .await?
        .filter(|m| m.status == "pending")
        .ok_or_else(|| AppError::bad_request("no pending invitation for this vault"))?;

    // Guarded flip plus the vault read in one transaction: a double accept, or
    // an accept racing a decline/withdraw, activates at most once.
    let mut res = with_retry(|| async {
        store::vaults::ACCEPT_INVITATION.on(db)
        .bind(("id", m.id.clone()))
        .bind(("vault", vault_id.clone()))
        .await?
        .check()
    })
    .await?;
    let rows: Vec<VaultFullRow> = res.take(0)?;
    let vault = rows.into_iter().next().ok_or_else(|| AppError::bad_request("no pending invitation for this vault"))?;
    let v: VaultOut = vault.into();
    Ok(VaultWithRole { id: v.id, name: v.name, kind: v.kind, created_at: v.created_at, role: m.role })
}

/// Decline (delete) a pending invitation into `vault_id`.
pub async fn decline_invitation(db: &Db, user_id: &RecordId, vault_id: &RecordId) -> AppResult<()> {
    authz::check_vault(vault_id)?;
    let m = membership_any_status(db, vault_id, user_id)
        .await?
        .filter(|m| m.status == "pending")
        .ok_or_else(|| AppError::bad_request("no pending invitation for this vault"))?;

    store::vaults::DELETE_RECORD.on(db).bind(("id", m.id)).await?;
    Ok(())
}

/// Owner-only. Refuses to remove the last owner, so a vault can't be left
/// admin-less.
pub async fn remove_member(db: &Db, user_id: &RecordId, vault_id: &RecordId, email: &str) -> AppResult<()> {
    authz::authorize(db, user_id, Action::ManageMembers, vault_id).await?;

    let mut res = store::vaults::USER_ID_BY_EMAIL.on(db)
        .bind(("email", crate::models_user::normalize_email(email)))
        .await?;
    let rows: Vec<EmailLookupRow> = res.take(0)?;
    let target_id = rows.into_iter().next().ok_or_else(|| AppError::bad_request(format!("no user with email '{email}'")))?.id;

    // any status: this also withdraws a pending invitation
    let target_membership = membership_any_status(db, vault_id, &target_id)
        .await?
        .ok_or_else(|| AppError::bad_request(format!("{email} is not a member or invitee")))?;

    // Owner count check and delete in one transaction, so two concurrent
    // removals cannot each see a second owner and strand the vault.
    with_retry(|| async {
        store::vaults::REMOVE_MEMBER.on(db)
        .bind(("vault", vault_id.clone()))
        .bind(("is_owner", target_membership.role == "owner"))
        .bind(("id", target_membership.id.clone()))
        .await?
        .check()
    })
    .await
    .map_err(|e| {
        if e.to_string().contains("last_owner") { AppError::bad_request("cannot remove the last owner") } else { e.into() }
    })?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct EntityRow {
    id: RecordId,
    #[serde(default)]
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    summary: String,
}

#[derive(Debug, Deserialize)]
struct MemoryRow {
    #[serde(default)]
    text: String,
    #[serde(rename = "type", default = "default_memory_type")]
    mem_type: String,
    #[serde(default)]
    source: Option<RecordId>,
}

fn default_memory_type() -> String {
    "world".to_string()
}

#[derive(Debug, Deserialize)]
struct RelationRow {
    #[serde(rename = "out")]
    other: RecordId,
    #[serde(default)]
    label: String,
}

/// Looks an entity up in `vault` by case-insensitive name (merge matching).
async fn find_by_name(db: &Db, kind: &str, vault: &RecordId, name: &str) -> AppResult<Option<EntityRow>> {
    // dynamic: table name varies over the six entity kinds
    let mut res = store::dynamic(db, "vaults.find_by_name", format!("SELECT * FROM {kind} WHERE vault = $vault AND string::lowercase(name) = $name LIMIT 1"))
        .bind(("vault", vault.clone()))
        .bind(("name", name.to_lowercase()))
        .await?;
    let rows: Vec<EntityRow> = res.take(0)?;
    Ok(rows.into_iter().next())
}

/// Merge rule for one incoming memory against what `subject` already has:
/// an identical fact is skipped, and a second observation is appended to the
/// existing one (one observation per subject). Returns true if handled.
async fn fold_into_existing_memory(db: &Db, subject: &RecordId, mem: &MemoryRow) -> AppResult<bool> {
    #[derive(Deserialize)]
    struct Existing {
        id: RecordId,
        #[serde(default)]
        text: String,
    }
    if mem.mem_type == "observation" {
        let mut res = store::vaults::OBSERVATION_OF.on(db)
            .bind(("s", subject.clone()))
            .await?;
        let existing: Vec<Existing> = res.take(0)?;
        let Some(existing) = existing.into_iter().next() else { return Ok(false) };
        if existing.text != mem.text {
            store::vaults::APPEND_OBSERVATION.on(db)
                .bind(("id", existing.id))
                .bind(("text", format!("{}\n\n{}", existing.text, mem.text)))
                .await?;
        }
        return Ok(true);
    }
    let mut res = store::vaults::SAME_MEMORY.on(db)
        .bind(("s", subject.clone()))
        .bind(("type", mem.mem_type.clone()))
        .bind(("text", mem.text.clone()))
        .await?;
    let same: Vec<VaultRow> = res.take(0)?;
    Ok(!same.is_empty())
}

/// Copies every entity, memory and relation of `src` into `dest` (both
/// already accessible to `user_id`); returns how many entities of `src` landed
/// in `dest`. With `merge_duplicates`, entities that share a kind and
/// (case-insensitive) name with one already in `dest` are folded into it
/// instead of duplicated.
async fn copy_into(db: &Db, user_id: &RecordId, src: &RecordId, dest: &RecordId, merge_duplicates: bool) -> AppResult<usize> {
    #[allow(clippy::mutable_key_type)] // RecordId hashes by value; the interior mutability is never touched
    let mut id_map: HashMap<RecordId, RecordId> = HashMap::new();
    for entity_kind in ENTITY_KINDS {
        // dynamic: table name varies over the six entity kinds
        let mut res = store::dynamic(db, "vaults.entities_in_vault", format!("SELECT * FROM {entity_kind} WHERE vault = $vault"))
            .bind(("vault", src.clone()))
            .await?;
        let rows: Vec<EntityRow> = res.take(0)?;
        for row in rows {
            if merge_duplicates
                && let Some(existing) = find_by_name(db, entity_kind, dest, &row.name).await? {
                    let mut aliases = existing.aliases.clone();
                    for alias in row.aliases.iter().cloned() {
                        if !aliases.iter().any(|a| a.eq_ignore_ascii_case(&alias)) {
                            aliases.push(alias);
                        }
                    }
                    let summary = if existing.summary.is_empty() { row.summary.clone() } else { existing.summary.clone() };
                    store::vaults::MERGE_ENTITY_INTO.on(db)
                        .bind(("id", existing.id.clone()))
                        .bind(("aliases", aliases))
                        .bind(("summary", summary))
                        .await?;
                    id_map.insert(row.id, existing.id);
                    continue;
                }
            // dynamic: table name varies over the six entity kinds
            let mut created = store::dynamic(
                db,
                "vaults.copy_entity",
                format!(
                    "CREATE {entity_kind} SET owner = $owner, vault = $vault, name = $name, \
                     aliases = $aliases, summary = $summary RETURN AFTER"
                ),
            )
                .bind(("owner", user_id.clone()))
                .bind(("vault", dest.clone()))
                .bind(("name", row.name))
                .bind(("aliases", row.aliases))
                .bind(("summary", row.summary))
                .await?;
            let created_rows: Vec<VaultRow> = created.take(0)?;
            let created_row =
                created_rows.into_iter().next().ok_or_else(|| AppError::internal("entity insert returned no row"))?;
            id_map.insert(row.id, created_row.id);
        }
    }

    for (old_id, new_id) in id_map.clone() {
        let mut res = store::vaults::MEMORIES_OF.on(db)
            .bind(("id", old_id))
            .await?;
        let memories: Vec<MemoryRow> = res.take(0)?;
        for mem in memories {
            if merge_duplicates && fold_into_existing_memory(db, &new_id, &mem).await? {
                continue;
            }
            store::vaults::COPY_MEMORY.on(db)
            .bind(("owner", user_id.clone()))
            .bind(("vault", dest.clone()))
            .bind(("subject", new_id.clone()))
            .bind(("text", mem.text))
            .bind(("type", mem.mem_type))
            .bind(("source", mem.source))
            .await?;
        }
    }

    #[allow(clippy::mutable_key_type)] // RecordId hashes by value; the interior mutability is never touched
    let mut seen_edges: HashSet<(RecordId, RecordId, String)> = HashSet::new();
    for (old_id, new_id) in id_map.clone() {
        let mut res = store::vaults::RELATIONS_FROM.on(db).bind(("id", old_id.clone())).await?;
        let edges: Vec<RelationRow> = res.take(0)?;
        for edge in edges {
            let Some(other_new) = id_map.get(&edge.other) else { continue };
            let key = (old_id.clone(), edge.other.clone(), edge.label.clone());
            if seen_edges.contains(&key) {
                continue;
            }
            seen_edges.insert(key);
            if merge_duplicates {
                let mut res = store::vaults::SAME_RELATION.on(db)
                    .bind(("in", new_id.clone()))
                    .bind(("out", other_new.clone()))
                    .bind(("label", edge.label.clone()))
                    .await?;
                let found: Vec<VaultRow> = res.take(0)?;
                if !found.is_empty() {
                    continue;
                }
            }
            store::vaults::COPY_RELATION.on(db)
                .bind(("in", new_id.clone()))
                .bind(("out", other_new.clone()))
                .bind(("label", edge.label))
                .bind(("owner", user_id.clone()))
                .await?;
        }
    }

    Ok(id_map.len())
}

/// Deep-copy `vault_id` (read access required, not necessarily owner) into a
/// brand-new vault the caller owns -- every entity, memory, and relation is
/// duplicated, not referenced, so editing the clone never touches the
/// source. The typical use: clone your personal vault into a fresh org vault
/// to start sharing a snapshot of it, without exposing the original.
///
/// `source_memories` (an observation's consolidation lineage, pointing at
/// other `memory` ids) is dropped on the copies rather than remapped -- those
/// ids belong to the source vault and would dangle; a cloned observation
/// reads fine, it just starts its lineage over. Likewise a relation whose
/// other endpoint isn't in the cloned vault (e.g. it points cross-vault at
/// something `user_id` can't read) is skipped rather than left dangling.
pub async fn clone_vault(
    db: &Db,
    user_id: &RecordId,
    vault_id: &RecordId,
    name: Option<&str>,
    kind: &str,
) -> AppResult<CloneOut> {
    authz::authorize(db, user_id, Action::Clone, vault_id).await?;
    authz::require_unrestricted()?;

    let source: Option<VaultFullRow> = db.select(vault_id.clone()).await?;
    let source = source.ok_or_else(|| AppError::coded(crate::error::ErrorCode::VaultNotFound, format!("vault not found: {vault_id}")))?;

    let clone_name = name.map(str::to_string).unwrap_or_else(|| format!("{} (copy)", source.name));
    let clone = create_vault(db, user_id, &clone_name, kind).await?;
    let clone_rid: RecordId = clone.id.parse().map_err(|_| AppError::internal("clone vault id did not round-trip"))?;

    let copied = copy_into(db, user_id, vault_id, &clone_rid, false).await?;

    Ok(CloneOut {
        id: clone.id,
        name: clone.name,
        kind: clone.kind,
        created_at: clone.created_at,
        entities_copied: copied,
    })
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct MergeOut {
    pub id: String,
    pub name: String,
    pub kind: String,
    #[schema(value_type = Option<String>)]
    pub created_at: Option<Datetime>,
    /// Entities in the merged vault (duplicates across the two sources count once).
    pub entities: usize,
    pub merged_from: [String; 2],
}

/// Merge two vaults into a NEW one: both are copied (never moved -- the
/// sources stay exactly as they were) into a fresh vault the caller owns, the
/// second folding into the first wherever an entity of the same kind and name
/// exists in both (aliases unioned, identical facts kept once, relations
/// deduplicated, a second observation appended to the first).
pub async fn merge_vaults(
    db: &Db,
    user_id: &RecordId,
    a: &RecordId,
    b: &RecordId,
    name: Option<&str>,
    kind: &str,
) -> AppResult<MergeOut> {
    if a == b {
        return Err(AppError::bad_request("pick two different vaults to merge"));
    }
    authz::authorize(db, user_id, Action::Merge, a).await?;
    authz::authorize(db, user_id, Action::Merge, b).await?;
    authz::require_unrestricted()?;
    let (va, vb): (Option<VaultFullRow>, Option<VaultFullRow>) = (db.select(a.clone()).await?, db.select(b.clone()).await?);
    let va = va.ok_or_else(|| AppError::coded(crate::error::ErrorCode::VaultNotFound, format!("vault not found: {a}")))?;
    let vb = vb.ok_or_else(|| AppError::coded(crate::error::ErrorCode::VaultNotFound, format!("vault not found: {b}")))?;

    let merged_name = name.map(str::to_string).unwrap_or_else(|| format!("{} + {}", va.name, vb.name));
    let merged = create_vault(db, user_id, &merged_name, kind).await?;
    let dest: RecordId = merged.id.parse().map_err(|_| AppError::internal("merged vault id did not round-trip"))?;

    copy_into(db, user_id, a, &dest, true).await?;
    copy_into(db, user_id, b, &dest, true).await?;

    let mut entities = 0;
    for entity_kind in ENTITY_KINDS {
        // dynamic: table name varies over the six entity kinds
        let mut res = store::dynamic(db, "vaults.count_entities", format!("SELECT count() FROM {entity_kind} WHERE vault = $vault GROUP ALL"))
            .bind(("vault", dest.clone()))
            .await?;
        let rows: Vec<CountRow> = res.take(0)?;
        entities += rows.first().map(|r| r.count).unwrap_or(0) as usize;
    }
    Ok(MergeOut {
        id: merged.id,
        name: merged.name,
        kind: merged.kind,
        created_at: merged.created_at,
        entities,
        merged_from: [a.to_string(), b.to_string()],
    })
}

/// Any member can leave their own membership, except the last owner of a
/// vault that still has other members (would strand them admin-less --
/// delete the vault instead if that's the intent).
pub async fn leave_vault(db: &Db, user_id: &RecordId, vault_id: &RecordId) -> AppResult<()> {
    match authz::authorize(db, user_id, Action::Leave, vault_id).await {
        Ok(_) => {}
        // not a member: nothing to leave
        Err(e) if e.code == crate::error::ErrorCode::VaultForbidden => return Ok(()),
        Err(e) => return Err(e),
    }
    let Some(m) = membership(db, vault_id, user_id).await? else { return Ok(()) };

    with_retry(|| async {
        store::vaults::LEAVE.on(db)
        .bind(("vault", vault_id.clone()))
        .bind(("user", user_id.clone()))
        .bind(("is_owner", m.role == "owner"))
        .bind(("id", m.id.clone()))
        .await?
        .check()
    })
    .await
    .map_err(|e| {
        if e.to_string().contains("last_owner") {
            AppError::bad_request("you are the last owner -- promote someone else first")
        } else {
            e.into()
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod vault_tests {
    use super::*;

    #[test]
    fn vault_out_from_full_row_carries_fields() {
        let row = VaultFullRow {
            id: "vault:abc".parse().unwrap(),
            name: "Personal".to_string(),
            kind: "personal".to_string(),
            created_at: None,
        };
        let out: VaultOut = row.into();
        assert_eq!(out.id, "vault:abc");
        assert_eq!(out.name, "Personal");
        assert_eq!(out.kind, "personal");
    }

    #[test]
    fn default_kind_is_personal() {
        assert_eq!(default_kind(), "personal");
    }

    #[test]
    fn default_memory_type_is_world() {
        assert_eq!(default_memory_type(), "world");
    }

    #[test]
    fn entity_kinds_match_memory_subject_union() {
        assert_eq!(ENTITY_KINDS, ["person", "organisation", "location", "repository", "file", "symbol"]);
    }
}
