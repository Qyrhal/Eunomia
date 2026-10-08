//! SurrealDB connection. The schema lives in `migrations/` (see `migrate.rs`).

use surrealdb::engine::any::Any;
use surrealdb::opt::auth::Root;
use surrealdb::Surreal;

use crate::config::Settings;

/// `Any` so the same code runs over `ws://` in production and `mem://` in tests.
pub type Db = Surreal<Any>;

pub async fn connect(settings: &Settings) -> surrealdb::Result<Db> {
    let db: Db = surrealdb::engine::any::connect(settings.surreal_url.as_str()).await?;
    // The embedded in-memory engine (tests) has no auth to sign in to.
    if !settings.surreal_url.starts_with("mem://") {
        db.signin(Root { username: settings.surreal_user.clone(), password: settings.surreal_pass.clone() })
            .await?;
    }
    // 3.x no longer creates the namespace and database on first use.
    db.query(format!(
        "DEFINE NAMESPACE IF NOT EXISTS `{ns}`; USE NS `{ns}`; DEFINE DATABASE IF NOT EXISTS `{db}`;",
        ns = settings.surreal_ns,
        db = settings.surreal_db
    ))
    .await?
    .check()?;
    db.use_ns(&settings.surreal_ns).use_db(&settings.surreal_db).await?;
    Ok(db)
}
