//! Statements for the vaults area. See the module docs in store/mod.rs.

use super::Stmt;

pub const CREATE_PERSONAL: Stmt = Stmt::new(
    "vaults.create_personal",
    r#"BEGIN TRANSACTION;
            CREATE $vault SET name = "Personal", kind = "personal";
            CREATE vault_member SET vault = $vault, user = $user, role = "owner";
            COMMIT TRANSACTION;"#,
);

pub const CREATE_VAULT: Stmt = Stmt::at(
    "vaults.create_vault",
    r#"BEGIN TRANSACTION;
                LET $v = (CREATE vault SET name = $name, kind = $kind RETURN AFTER);
                CREATE vault_member SET vault = $v[0].id, user = $user, role = "owner";
                RETURN $v;
                COMMIT TRANSACTION;"#,
    3, // BEGIN 0, LET 1, CREATE 2, RETURN 3
);

pub const MEMBERSHIP_ACTIVE: Stmt = Stmt::new(
    "vaults.membership_active",
    r#"SELECT * FROM vault_member WHERE vault = $vault AND user = $user AND status = "active" LIMIT 1"#,
);

pub const MEMBERSHIP_ANY: Stmt =
    Stmt::new("vaults.membership_any", "SELECT * FROM vault_member WHERE vault = $vault AND user = $user LIMIT 1");

pub const ACCESSIBLE_IDS: Stmt =
    Stmt::new("vaults.accessible_ids", r#"SELECT vault FROM vault_member WHERE user = $user AND status = "active""#);

pub const DEFAULT_ID: Stmt = Stmt::new(
    "vaults.default_id",
    r#"SELECT vault FROM vault_member WHERE user = $user AND status = "active" AND vault.kind = "personal" LIMIT 1"#,
);

pub const LIST_MINE: Stmt = Stmt::new(
    "vaults.list_mine",
    r#"SELECT vault.* AS vault, role FROM vault_member WHERE user = $user AND status = "active""#,
);

pub const RENAME: Stmt = Stmt::new("vaults.rename", "UPDATE $id SET name = $name RETURN AFTER");

pub const DELETE_VAULT: Stmt = Stmt::new(
    "vaults.delete_vault",
    "BEGIN TRANSACTION; DELETE vault_member WHERE vault = $vault; DELETE $vault; COMMIT TRANSACTION;",
);

pub const INVITE: Stmt = Stmt::new(
    "vaults.invite",
    r#"CREATE vault_member SET vault = $vault, user = $user, role = $role, status = "pending" RETURN AFTER"#,
);

pub const LIST_MEMBERS: Stmt = Stmt::new(
    "vaults.list_members",
    r#"SELECT user, role FROM vault_member WHERE vault = $vault AND status = "active""#,
);

pub const LIST_INVITATIONS: Stmt = Stmt::new(
    "vaults.list_invitations",
    r#"SELECT vault.* AS vault, role, created_at FROM vault_member WHERE user = $user AND status = "pending""#,
);

pub const ACCEPT_INVITATION: Stmt = Stmt::at(
    "vaults.accept_invitation",
    r#"BEGIN TRANSACTION;
            LET $flipped = (UPDATE $id SET status = "active" WHERE status = "pending" RETURN AFTER);
            RETURN IF array::len($flipped) > 0 { (SELECT * FROM $vault) } ELSE { [] };
            COMMIT TRANSACTION;"#,
    2, // BEGIN 0, LET 1, RETURN 2
);

pub const DELETE_RECORD: Stmt = Stmt::new("vaults.delete_record", "DELETE $id");

pub const REMOVE_MEMBER: Stmt = Stmt::new(
    "vaults.remove_member",
    r#"BEGIN TRANSACTION;
            LET $owners = (SELECT VALUE id FROM vault_member WHERE vault = $vault AND role = "owner");
            IF $is_owner AND array::len($owners) <= 1 { THROW "last_owner" };
            DELETE $id;
            COMMIT TRANSACTION;"#,
);

pub const LEAVE: Stmt = Stmt::new(
    "vaults.leave",
    r#"BEGIN TRANSACTION;
            LET $owners = (SELECT VALUE id FROM vault_member WHERE vault = $vault AND role = "owner");
            LET $others = (SELECT VALUE id FROM vault_member WHERE vault = $vault AND user != $user);
            IF $is_owner AND array::len($owners) <= 1 AND array::len($others) > 0 { THROW "last_owner" };
            DELETE $id;
            COMMIT TRANSACTION;"#,
);

pub const OBSERVATION_OF: Stmt = Stmt::new(
    "vaults.observation_of",
    r#"SELECT id, text FROM memory WHERE subject = $s AND type = "observation" LIMIT 1"#,
);

pub const APPEND_OBSERVATION: Stmt = Stmt::new(
    "vaults.append_observation",
    r#"UPDATE $id SET text = $text, version = version + 1, status = "stale", updated_at = time::now()"#,
);

pub const SAME_MEMORY: Stmt = Stmt::new(
    "vaults.same_memory",
    "SELECT id FROM memory WHERE subject = $s AND type = $type AND text = $text LIMIT 1",
);

pub const MERGE_ENTITY_INTO: Stmt = Stmt::new(
    "vaults.merge_entity_into",
    "UPDATE $id SET aliases = $aliases, summary = $summary, updated_at = time::now()",
);

pub const MEMORIES_OF: Stmt = Stmt::new("vaults.memories_of", "SELECT * FROM memory WHERE subject = $id");

pub const COPY_MEMORY: Stmt = Stmt::new(
    "vaults.copy_memory",
    "CREATE memory SET owner = $owner, vault = $vault, subject = $subject, text = $text, \
                 type = $type, source = $source RETURN AFTER",
);

pub const RELATIONS_FROM: Stmt = Stmt::new("vaults.relations_from", "SELECT * FROM relates_to WHERE in = $id");

pub const SAME_RELATION: Stmt = Stmt::new(
    "vaults.same_relation",
    "SELECT id FROM relates_to WHERE in = $in AND out = $out AND label = $label LIMIT 1",
);

pub const COPY_RELATION: Stmt =
    Stmt::new("vaults.copy_relation", "RELATE $in->relates_to->$out SET label = $label, owner = $owner");

pub const ALL: &[&Stmt] = &[
    &CREATE_PERSONAL,
    &CREATE_VAULT,
    &MEMBERSHIP_ACTIVE,
    &MEMBERSHIP_ANY,
    &ACCESSIBLE_IDS,
    &DEFAULT_ID,
    &LIST_MINE,
    &RENAME,
    &DELETE_VAULT,
    &INVITE,
    &LIST_MEMBERS,
    &LIST_INVITATIONS,
    &ACCEPT_INVITATION,
    &DELETE_RECORD,
    &REMOVE_MEMBER,
    &LEAVE,
    &OBSERVATION_OF,
    &APPEND_OBSERVATION,
    &SAME_MEMORY,
    &MERGE_ENTITY_INTO,
    &MEMORIES_OF,
    &COPY_MEMORY,
    &RELATIONS_FROM,
    &SAME_RELATION,
    &COPY_RELATION,
];
