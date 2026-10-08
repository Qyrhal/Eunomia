//! Provisioning: the only code that signs in as the SurrealDB root user.
//!
//! Root is needed for exactly three things: creating the `control` database and its user at boot,
//! creating an org's database and database user (`DEFINE DATABASE org_<uuid> STRICT`, migrations,
//! `DEFINE USER app ... ROLES EDITOR`), and moving a pre-tenancy install's data out of the old
//! single database. Everything a request does afterwards runs as a database-level user via
//! `pool.for_org(..)`, which cannot even `USE` another org's database.
//!
//! Compiled behind the `provisioning` cargo feature (on by default). Without it nothing here can
//! sign in as root: [`Provisioner::connect`] returns `None`, boot skips setup, and signup answers
//! `tenant.provisioning_disabled`; such a build serves an install that a default build provisioned.
//!
//! Org databases are `STRICT`: a query naming a table the migrations did not define fails instead of
//! silently reading nothing. The legacy single database stays non-strict (it predates this).

use std::sync::Arc;

use surrealdb::types::SurrealValue;

use crate::config::Settings;
use crate::connectors::crypto;
use crate::db::Db;
use crate::error::{AppError, AppResult, ErrorCode};
use crate::pool::{self, ControlDb, OrgId, DB_USER};
use crate::store::{self, root};

pub mod legacy;

/// A root session. Held by [`Provisioner`] and nothing else.
#[derive(Clone)]
pub struct Provisioner {
    root: Arc<Db>,
    settings: Settings,
}

#[derive(Debug, serde::Deserialize, SurrealValue)]
struct TenantState {
    db: String,
    db_pass_enc: String,
}

impl Provisioner {
    /// Sign in as root on a clone of `template`. `None` when the `provisioning` feature is off.
    #[cfg(feature = "provisioning")]
    pub(crate) async fn connect(template: &Db, settings: &Settings) -> surrealdb::Result<Option<Provisioner>> {
        use surrealdb::opt::auth::Root;
        let session = template.clone();
        session.signin(Root { username: settings.surreal_user.clone(), password: settings.surreal_pass.clone() }).await?;
        Ok(Some(Provisioner { root: Arc::new(session), settings: settings.clone() }))
    }

    #[cfg(not(feature = "provisioning"))]
    pub(crate) async fn connect(_template: &Db, _settings: &Settings) -> surrealdb::Result<Option<Provisioner>> {
        Ok(None)
    }

    /// A root session on `db` in the install's namespace.
    async fn session(&self, db: &str) -> surrealdb::Result<Db> {
        let s = (*self.root).clone();
        s.use_ns(&self.settings.surreal_ns).use_db(db).await?;
        Ok(s)
    }

    /// `DEFINE DATABASE` runs with a namespace selected and no database.
    async fn define_database(&self, db: &str, strict: bool) -> surrealdb::Result<()> {
        let s = (*self.root).clone();
        s.use_ns(&self.settings.surreal_ns).await?;
        let strict = if strict { " STRICT" } else { "" };
        root(&s, "provisioning.database", format!("DEFINE DATABASE IF NOT EXISTS `{db}`{strict}")).await?.check()?;
        Ok(())
    }

    /// Create the namespace, the `control` database and its user, and bring control to the latest schema.
    pub async fn ensure_control(&self) -> surrealdb::Result<()> {
        let ns = &self.settings.surreal_ns;
        root(&self.root, "provisioning.namespace", format!("DEFINE NAMESPACE IF NOT EXISTS `{ns}`")).await?.check()?;
        self.define_database(pool::CONTROL_DB, false).await?;
        let s = self.session(pool::CONTROL_DB).await?;
        // OVERWRITE keeps the password in step with ENCRYPTION_KEY, the only thing it derives from.
        root(
            &s,
            "provisioning.control_user",
            format!("DEFINE USER OVERWRITE {DB_USER} ON DATABASE PASSWORD '{}' ROLES EDITOR", pool::control_password(&self.settings)),
        )
        .await?
        .check()?;
        crate::migrate::migrate_control(&s).await
    }

    /// Create an org: its record, its database (migrated), its database user, and the routing row.
    /// Safe to re-run after a crash: the generated password is stored before anything else, and every
    /// step is idempotent, so a half-provisioned org resumes where it stopped.
    pub async fn provision_org(&self, control: &ControlDb, org: OrgId, name: &str) -> AppResult<()> {
        store::tenant::ORG_CREATE.on(control).bind(("id", org.record())).bind(("name", name.to_string())).await?.check()?;
        let existing = self.tenant_state(control, &org).await?;
        let (db, pass) = match existing {
            Some(t) => (t.db, crypto::decrypt(&self.settings.encryption_key, &t.db_pass_enc)?),
            None => {
                let (db, pass) = (org.db_name(), pool::generate_db_password());
                self.write_tenant(control, &org, &db, &pass, 0, "provisioning").await?;
                (db, pass)
            }
        };
        self.build_database(&db, &pass).await?;
        self.write_tenant(control, &org, &db, &pass, crate::migrate::LATEST_TENANT, "ready").await?;
        Ok(())
    }

    /// Point `org` at an existing database (the isolation suite's shared-database mode).
    #[cfg(feature = "test-support")]
    pub async fn adopt_org(&self, control: &ControlDb, org: OrgId, name: &str, db: &str, pass: &str) -> AppResult<()> {
        store::tenant::ORG_CREATE.on(control).bind(("id", org.record())).bind(("name", name.to_string())).await?.check()?;
        self.write_tenant(control, &org, db, pass, crate::migrate::LATEST_TENANT, "ready").await
    }

    /// `DEFINE DATABASE .. STRICT`, tenant migrations, and the org's database user.
    async fn build_database(&self, db: &str, pass: &str) -> AppResult<()> {
        let ns = &self.settings.surreal_ns;
        self.define_database(db, true).await?;
        let s = self.session(db).await?;
        crate::migrate::migrate(&s, &self.settings).await?;
        root(&s, "provisioning.db_user", format!("DEFINE USER OVERWRITE {DB_USER} ON DATABASE PASSWORD '{pass}' ROLES EDITOR")).await?.check()?;
        tracing::info!(db, ns, "org database ready");
        Ok(())
    }

    /// Apply pending tenant migrations to one org's database (the `migrate_tenant` job).
    pub async fn migrate_org(&self, control: &ControlDb, org: &OrgId) -> AppResult<()> {
        let Some(t) = self.tenant_state(control, org).await? else {
            return Err(AppError::coded(ErrorCode::TenantNotFound, "No data store exists for this organisation."));
        };
        let s = self.session(&t.db).await?;
        crate::migrate::migrate(&s, &self.settings).await?;
        store::tenant::SET_STATE
            .on(control)
            .bind(("id", org.tenant_record()))
            .bind(("status", "ready"))
            .bind(("version", crate::migrate::LATEST_TENANT as i64))
            .await?
            .check()?;
        Ok(())
    }

    async fn tenant_state(&self, control: &ControlDb, org: &OrgId) -> AppResult<Option<TenantState>> {
        let mut res = store::tenant::BY_ORG.on(control).bind(("id", org.tenant_record())).await?;
        Ok(res.take::<Vec<TenantState>>(0)?.into_iter().next())
    }

    async fn write_tenant(&self, control: &ControlDb, org: &OrgId, db: &str, pass: &str, version: u32, status: &str) -> AppResult<()> {
        store::tenant::UPSERT
            .on(control)
            .bind(("id", org.tenant_record()))
            .bind(("org", org.record()))
            .bind(("db", db.to_string()))
            .bind(("db_user", DB_USER))
            .bind(("db_pass_enc", crypto::encrypt(&self.settings.encryption_key, pass)))
            .bind(("version", version as i64))
            .bind(("status", status.to_string()))
            .await?
            .check()?;
        Ok(())
    }

    /// A root session on a fresh, empty database (migration tests, the spike). Not strict.
    #[cfg(feature = "test-support")]
    pub async fn scratch(&self, db: &str) -> surrealdb::Result<Db> {
        self.define_database(db, false).await?;
        self.session(db).await
    }

    /// A root session on `db` (tests that check what is in an org database).
    #[cfg(feature = "test-support")]
    pub async fn session_on(&self, db: &str) -> surrealdb::Result<Db> {
        self.session(db).await
    }

    /// Spike S1 and the isolation suite: create the database user and database for an org with a
    /// known password (no routing row).
    #[cfg(feature = "test-support")]
    pub async fn build_database_for_tests(&self, db: &str, pass: &str) -> AppResult<()> {
        self.build_database(db, pass).await
    }
}
