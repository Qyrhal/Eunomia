//! Statements for the app area. See the module docs in store/mod.rs.

use super::Stmt;

pub const ALL: &[&Stmt] = &[
    &AUTH_SESSION_CREATE,
    &AUTH_SESSION_FIND,
    &AUTH_SESSION_TOUCH,
    &AUTH_SESSION_REVOKE_BY_SID,
    &AUTH_SESSION_REVOKE,
    &AUTH_SESSION_LIST,
    &AUTH_USER_ID_BY_EMAIL,
    &AUTH_USER_CREATE,
    &AUTH_USER_BY_EMAIL,
    &AUTH_USER_COUNT,
    &AUTH_USER_ONBOARDED,
    &AUTH_TOKEN_CREATE,
    &AUTH_TOKEN_LIST,
    &AUTH_TOKEN_BY_HASH,
    &AUTH_TOKEN_TOUCH,
    &CHAT_SETTINGS_UPSERT,
    &CHAT_THREAD_CREATE,
    &CHAT_THREAD_LIST,
    &CHAT_THREAD_MESSAGES_DELETE,
    &CHAT_THREAD_DELETE,
    &CHAT_THREAD_RETITLE,
    &CHAT_THREAD_TOUCH,
    &CHAT_MESSAGES_FOR_THREAD,
    &CHAT_MESSAGES_FOR_OWNER,
    &CHAT_MESSAGE_CREATE,
    &CHAT_MESSAGES_CLEAR,
    &CONNECTOR_BY_KIND,
    &CONNECTOR_CREATE,
    &CONNECTOR_ENABLED,
    &AUDIT_LIST,
    &AUDIT_COUNT,
    &EXPORT_PERSONAL_VAULT,
    &EXPORT_USER_EMAILS,
    &EXPORT_MEMORIES,
    &EXPORT_RELATIONS_OUT,
    &EXPORT_RELATIONS_IN,
    &SETTINGS_UPSERT_DEFAULTS,
    &SOURCES_SYNC_STATUS_LIST,
    &SOURCES_RECORD_COUNTS,
    &SOURCES_HEYPOCKET_RECENT,
    &SOURCES_HEYPOCKET_SEARCH,
    &SOURCES_RECORD_TOUCH,
    &SOURCES_RECORD_UPSERT,
    &SOURCES_USER_IDS,
    &SOURCES_SYNC_STATUS_UPSERT,
    &SOURCES_SYNC_OK,
    &SOURCES_SYNC_FAILED,
    &SOURCES_SYNC_INTERVALS,
    &SOURCES_UP_BY_TYPE,
];

// auth sessions and users

pub const AUTH_SESSION_CREATE: Stmt =
    Stmt::new("app.auth_session_create", "CREATE session SET owner = $owner, sid = $sid, user_agent = $user_agent");

pub const AUTH_SESSION_FIND: Stmt = Stmt::new(
    "app.auth_session_find",
    "SELECT * FROM session WHERE sid = $sid AND owner = $owner AND revoked = false LIMIT 1",
);

pub const AUTH_SESSION_TOUCH: Stmt = Stmt::new("app.auth_session_touch", "UPDATE $id SET last_seen_at = time::now()");

pub const AUTH_SESSION_REVOKE_BY_SID: Stmt =
    Stmt::new("app.auth_session_revoke_by_sid", "UPDATE session SET revoked = true WHERE sid = $sid");

pub const AUTH_SESSION_REVOKE: Stmt = Stmt::new("app.auth_session_revoke", "UPDATE $id SET revoked = true");

pub const AUTH_SESSION_LIST: Stmt = Stmt::new(
    "app.auth_session_list",
    "SELECT id, user_agent, created_at, last_seen_at FROM session \
     WHERE owner = $owner AND revoked = false ORDER BY last_seen_at DESC",
);

pub const AUTH_USER_ID_BY_EMAIL: Stmt =
    Stmt::new("app.auth_user_id_by_email", "SELECT id FROM user WHERE string::lowercase(email) = $email LIMIT 1");

pub const AUTH_USER_CREATE: Stmt =
    Stmt::new("app.auth_user_create", "CREATE user SET email = $email, password_hash = $password_hash RETURN AFTER");

pub const AUTH_USER_BY_EMAIL: Stmt =
    Stmt::new("app.auth_user_by_email", "SELECT * FROM user WHERE string::lowercase(email) = $email LIMIT 1");

pub const AUTH_USER_COUNT: Stmt = Stmt::new("app.auth_user_count", "SELECT count() FROM user GROUP ALL");

pub const AUTH_USER_ONBOARDED: Stmt = Stmt::new("app.auth_user_onboarded", "UPDATE $id SET onboarded_at = time::now()");

pub const AUTH_TOKEN_CREATE: Stmt = Stmt::new(
    "app.auth_token_create",
    "CREATE api_token SET owner = $owner, name = $name, token_hash = $hash RETURN AFTER",
);

pub const AUTH_TOKEN_LIST: Stmt = Stmt::new(
    "app.auth_token_list",
    "SELECT id, name, created_at, last_used_at FROM api_token \
     WHERE owner = $owner ORDER BY created_at DESC",
);

pub const AUTH_TOKEN_BY_HASH: Stmt =
    Stmt::new("app.auth_token_by_hash", "SELECT * FROM api_token WHERE token_hash = $hash LIMIT 1");

pub const AUTH_TOKEN_TOUCH: Stmt = Stmt::new("app.auth_token_touch", "UPDATE $id SET last_used_at = time::now()");

// chat

pub const CHAT_SETTINGS_UPSERT: Stmt =
    Stmt::new("app.chat_settings_upsert", "UPSERT $id SET owner = $owner RETURN AFTER");

pub const CHAT_THREAD_CREATE: Stmt =
    Stmt::new("app.chat_thread_create", "CREATE chat_thread SET owner = $owner, title = $title RETURN AFTER");

pub const CHAT_THREAD_LIST: Stmt =
    Stmt::new("app.chat_thread_list", "SELECT * FROM chat_thread WHERE owner = $owner ORDER BY updated_at DESC");

pub const CHAT_THREAD_MESSAGES_DELETE: Stmt =
    Stmt::new("app.chat_thread_messages_delete", "DELETE chat_message WHERE thread_id = $tid");

pub const CHAT_THREAD_DELETE: Stmt = Stmt::new("app.chat_thread_delete", "DELETE $id");

pub const CHAT_THREAD_RETITLE: Stmt =
    Stmt::new("app.chat_thread_retitle", "UPDATE $id SET title = $title, updated_at = time::now()");

pub const CHAT_THREAD_TOUCH: Stmt = Stmt::new("app.chat_thread_touch", "UPDATE $id SET updated_at = time::now()");

pub const CHAT_MESSAGES_FOR_THREAD: Stmt = Stmt::new(
    "app.chat_messages_for_thread",
    "SELECT * FROM chat_message WHERE owner = $owner AND thread_id = $thread_id ORDER BY created_at",
);

/// Also used by the data export.
pub const CHAT_MESSAGES_FOR_OWNER: Stmt =
    Stmt::new("app.chat_messages_for_owner", "SELECT * FROM chat_message WHERE owner = $owner ORDER BY created_at");

pub const CHAT_MESSAGE_CREATE: Stmt = Stmt::new(
    "app.chat_message_create",
    "CREATE chat_message SET owner = $owner, thread_id = $thread_id, role = $role, content = $content, \
     tool_calls = $tool_calls, tool_call_id = $tool_call_id RETURN AFTER",
);

pub const CHAT_MESSAGES_CLEAR: Stmt =
    Stmt::new("app.chat_messages_clear", "DELETE chat_message WHERE owner = $owner AND thread_id = $thread_id");

// connectors

pub const CONNECTOR_BY_KIND: Stmt =
    Stmt::new("app.connector_by_kind", "SELECT * FROM connector WHERE owner = $owner AND kind = $kind LIMIT 1");

pub const CONNECTOR_CREATE: Stmt =
    Stmt::new("app.connector_create", "CREATE connector SET owner = $owner, kind = $kind RETURN AFTER");

pub const CONNECTOR_ENABLED: Stmt =
    Stmt::new("app.connector_enabled", "SELECT kind, config FROM connector WHERE owner = $owner AND enabled = true");

// audit

pub const AUDIT_LIST: Stmt = Stmt::new(
    "app.audit_list",
    "SELECT * FROM audit_log WHERE owner = $owner ORDER BY created_at DESC LIMIT $limit START $offset",
);

pub const AUDIT_COUNT: Stmt =
    Stmt::new("app.audit_count", "SELECT count() FROM audit_log WHERE owner = $owner GROUP ALL");

// export

pub const EXPORT_PERSONAL_VAULT: Stmt = Stmt::new(
    "app.export_personal_vault",
    "SELECT vault FROM vault_member WHERE user = $user AND vault.kind = \"personal\" LIMIT 1",
);

pub const EXPORT_USER_EMAILS: Stmt = Stmt::new("app.export_user_emails", "SELECT id, email FROM user WHERE id IN $ids");

pub const EXPORT_MEMORIES: Stmt =
    Stmt::new("app.export_memories", "SELECT * FROM memory WHERE subject = $id ORDER BY created_at DESC");

pub const EXPORT_RELATIONS_OUT: Stmt = Stmt::new("app.export_relations_out", "SELECT * FROM relates_to WHERE in = $id");

pub const EXPORT_RELATIONS_IN: Stmt = Stmt::new("app.export_relations_in", "SELECT * FROM relates_to WHERE out = $id");

// settings

pub const SETTINGS_UPSERT_DEFAULTS: Stmt = Stmt::new(
    "app.settings_upsert_defaults",
    "UPSERT $id SET owner = $owner, embedding_model = $embedding_model, \
     sync_intervals = $sync_intervals, theme = $theme, \
     observations_mission = $observations_mission, openai_base_url = $openai_base_url, \
     updated_at = time::now() RETURN AFTER",
);

// sources

pub const SOURCES_SYNC_STATUS_LIST: Stmt =
    Stmt::new("app.sources_sync_status_list", "SELECT * FROM sync_status WHERE owner = $owner");

pub const SOURCES_RECORD_COUNTS: Stmt = Stmt::new(
    "app.sources_record_counts",
    "SELECT source, count() AS count FROM cache_record WHERE owner = $owner AND deleted = false GROUP BY source",
);

pub const SOURCES_HEYPOCKET_RECENT: Stmt = Stmt::new(
    "app.sources_heypocket_recent",
    "SELECT title, occurred_at, url, payload FROM cache_record \
     WHERE owner = $owner AND type = 'heypocket.recording' AND deleted = false \
     AND occurred_at != NONE AND occurred_at >= $since ORDER BY occurred_at DESC LIMIT 2000",
);

pub const SOURCES_HEYPOCKET_SEARCH: Stmt = Stmt::new(
    "app.sources_heypocket_search",
    "SELECT title, occurred_at, url, payload FROM cache_record \
     WHERE owner = $owner AND type = 'heypocket.recording' AND deleted = false \
     AND (string::contains(string::lowercase(title), $q) OR string::contains(string::lowercase(body_text), $q)) \
     LIMIT 20",
);

pub const SOURCES_RECORD_TOUCH: Stmt = Stmt::new("app.sources_record_touch", "UPDATE $id SET ingested_at = $now");

pub const SOURCES_RECORD_UPSERT: Stmt = Stmt::new(
    "app.sources_record_upsert",
    "UPSERT $id SET owner = $owner, source = $source, type = $type, external_id = $external_id, \
     title = $title, body_text = $body_text, occurred_at = $occurred_at, url = $url, \
     payload = $payload, content_hash = $content_hash, ingested_at = $now, \
     updated_at = $now, deleted = $deleted",
);

pub const SOURCES_USER_IDS: Stmt = Stmt::new("app.sources_user_ids", "SELECT id FROM user");

pub const SOURCES_SYNC_STATUS_UPSERT: Stmt = Stmt::new(
    "app.sources_sync_status_upsert",
    "UPSERT $id SET owner = $owner, cursor = '', consecutive_failures = 0, last_error = '', \
     last_report = {} RETURN AFTER",
);

pub const SOURCES_SYNC_OK: Stmt = Stmt::new(
    "app.sources_sync_ok",
    "UPDATE $id SET last_run = $now, cursor = $cursor, last_ok = $now, \
     last_error = '', consecutive_failures = 0, last_report = $report",
);

pub const SOURCES_SYNC_FAILED: Stmt = Stmt::new(
    "app.sources_sync_failed",
    "UPDATE $id SET last_run = $now, consecutive_failures = $failures, last_error = $error",
);

pub const SOURCES_SYNC_INTERVALS: Stmt = Stmt::new(
    "app.sources_sync_intervals",
    "SELECT sync_intervals FROM app_settings WHERE owner = $owner LIMIT 1",
);

pub const SOURCES_UP_BY_TYPE: Stmt = Stmt::new(
    "app.sources_up_by_type",
    "SELECT title, external_id, occurred_at, payload FROM cache_record WHERE owner = $owner AND type = $type ORDER BY occurred_at DESC LIMIT $limit",
);
