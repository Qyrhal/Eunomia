//! The raw SurrealDB client type and how to open one. The schema lives in `migrations/` (see
//! `migrate.rs`). Nothing outside `pool.rs` and `provisioning/` holds a `Db`: request code gets an
//! `OrgDb` or `ControlDb` (see `pool.rs`).

use surrealdb::engine::any::Any;
use surrealdb::opt::auth::Root;
use surrealdb::Surreal;

use crate::config::Settings;

/// `Any` so the same code runs over `ws://` in production and `mem://` in tests.
pub(crate) type Db = Surreal<Any>;

/// An unauthenticated connection. Every `clone()` of a 3.x `Surreal` is its own session (own
/// namespace, database and credentials), so the pool and the provisioner each take a clone and sign
/// in separately; this one is never signed in. The embedded in-memory engine (tests, the spike)
/// starts with authentication enforced, with the configured root user.
pub(crate) async fn connect_raw(settings: &Settings, config: surrealdb::opt::Config) -> surrealdb::Result<Db> {
    let config = if settings.surreal_url.starts_with("mem://") {
        config.user(Root { username: settings.surreal_user.clone(), password: settings.surreal_pass.clone() })
    } else {
        config
    };
    surrealdb::engine::any::connect((settings.surreal_url.as_str(), config)).await
}
