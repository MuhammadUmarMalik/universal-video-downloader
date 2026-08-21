use super::error::PersistenceError;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use std::time::Duration;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

const DATABASE_FILENAME: &str = "umd.sqlite3";
const MAX_CONNECTIONS: u32 = 4;
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
#[allow(dead_code)] // Repository and application services consume these fields in the next Phase 2 subtasks.
pub struct Database {
    pool: SqlitePool,
    path: PathBuf,
}

impl Database {
    pub async fn from_app_data_dir(
        app_data_dir: impl AsRef<Path>,
    ) -> Result<Self, PersistenceError> {
        let directory = app_data_dir.as_ref();
        tokio::fs::create_dir_all(directory)
            .await
            .map_err(PersistenceError::PrepareDirectory)?;
        harden_directory_permissions(directory).map_err(PersistenceError::PrepareDirectory)?;

        Self::connect_at_path(directory.join(DATABASE_FILENAME)).await
    }

    pub async fn connect_at_path(path: PathBuf) -> Result<Self, PersistenceError> {
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .busy_timeout(BUSY_TIMEOUT);

        let pool = SqlitePoolOptions::new()
            .max_connections(MAX_CONNECTIONS)
            .min_connections(1)
            .acquire_timeout(BUSY_TIMEOUT)
            .connect_with(options)
            .await
            .map_err(PersistenceError::Connection)?;

        MIGRATOR
            .run(&pool)
            .await
            .map_err(PersistenceError::Migration)?;
        if let Some(parent) = path.parent() {
            harden_directory_permissions(parent).map_err(PersistenceError::PrepareDirectory)?;
        }
        harden_database_permissions(&path).map_err(PersistenceError::PrepareDirectory)?;

        let database = Self { pool, path };
        database.health_check().await?;
        Ok(database)
    }

    #[allow(dead_code)] // Repository implementations will borrow the shared pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    #[allow(dead_code)] // Diagnostics and backup workflows will expose the resolved path later.
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn health_check(&self) -> Result<(), PersistenceError> {
        let _: i64 = sqlx::query_scalar("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(PersistenceError::HealthCheck)?;

        let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&self.pool)
            .await
            .map_err(PersistenceError::HealthCheck)?;
        if foreign_keys != 1 {
            return Err(PersistenceError::ForeignKeysDisabled);
        }

        Ok(())
    }

    #[cfg(test)]
    async fn in_memory() -> Result<Self, PersistenceError> {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .in_memory(true)
            .foreign_keys(true)
            .busy_timeout(BUSY_TIMEOUT);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .min_connections(1)
            .connect_with(options)
            .await
            .map_err(PersistenceError::Connection)?;
        MIGRATOR
            .run(&pool)
            .await
            .map_err(PersistenceError::Migration)?;
        let database = Self {
            pool,
            path: PathBuf::from(":memory:"),
        };
        database.health_check().await?;
        Ok(database)
    }
}

fn harden_directory_permissions(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn harden_database_permissions(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        if path.exists() {
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Database;
    use sqlx::Row;
    use tempfile::tempdir;

    #[tokio::test]
    async fn fresh_database_applies_schema_and_seed() {
        let database = Database::in_memory()
            .await
            .expect("database should initialize");

        let table_count: i64 = sqlx::query("SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' AND name NOT LIKE '_sqlx_%'")
            .fetch_one(database.pool())
            .await
            .expect("schema query should succeed")
            .get("count");
        assert_eq!(table_count, 11);

        let license_state_count: i64 = sqlx::query("SELECT COUNT(*) AS count FROM license_state")
            .fetch_one(database.pool())
            .await
            .expect("license seed query should succeed")
            .get("count");
        assert_eq!(license_state_count, 1);
    }

    #[tokio::test]
    async fn foreign_keys_reject_invalid_relationships() {
        let database = Database::in_memory()
            .await
            .expect("database should initialize");
        let result = sqlx::query(
            "INSERT INTO media_sources (id, platform_id, source_url, normalized_url, source_type, discovered_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("source-1")
        .bind("missing-platform")
        .bind("https://example.test/media")
        .bind("https://example.test/media")
        .bind("generic")
        .bind("2026-01-01T00:00:00Z")
        .execute(database.pool())
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn migrations_are_idempotent() {
        let database = Database::in_memory()
            .await
            .expect("database should initialize");

        super::MIGRATOR
            .run(database.pool())
            .await
            .expect("running applied migrations again should succeed");
    }

    #[tokio::test]
    async fn schema_rejects_invalid_job_status() {
        let database = Database::in_memory()
            .await
            .expect("database should initialize");
        sqlx::query(
            "INSERT INTO platforms (id, slug, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("platform-1")
        .bind("generic")
        .bind("Generic")
        .bind("2026-01-01T00:00:00Z")
        .bind("2026-01-01T00:00:00Z")
        .execute(database.pool())
        .await
        .expect("platform insert should succeed");
        sqlx::query("INSERT INTO media_sources (id, platform_id, source_url, normalized_url, source_type, discovered_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind("source-1")
            .bind("platform-1")
            .bind("https://example.test/media")
            .bind("https://example.test/media")
            .bind("generic")
            .bind("2026-01-01T00:00:00Z")
            .execute(database.pool())
            .await
            .expect("source insert should succeed");
        sqlx::query("INSERT INTO media_items (id, source_id, canonical_url, title, first_seen_at, last_seen_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind("item-1")
            .bind("source-1")
            .bind("https://example.test/media")
            .bind("Example")
            .bind("2026-01-01T00:00:00Z")
            .bind("2026-01-01T00:00:00Z")
            .execute(database.pool())
            .await
            .expect("item insert should succeed");

        let result = sqlx::query("INSERT INTO download_jobs (id, media_item_id, status, destination_path, filename, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind("job-1")
            .bind("item-1")
            .bind("not-a-valid-status")
            .bind("/tmp/example")
            .bind("example.mp4")
            .bind("2026-01-01T00:00:00Z")
            .bind("2026-01-01T00:00:00Z")
            .execute(database.pool())
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn schema_rejects_cross_source_collection_membership() {
        let database = Database::in_memory()
            .await
            .expect("database should initialize");
        for (platform_id, slug) in [("platform-1", "generic-one"), ("platform-2", "generic-two")] {
            sqlx::query("INSERT INTO platforms (id, slug, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
                .bind(platform_id)
                .bind(slug)
                .bind(slug)
                .bind("2026-01-01T00:00:00Z")
                .bind("2026-01-01T00:00:00Z")
                .execute(database.pool())
                .await
                .expect("platform insert should succeed");
        }
        for (source_id, platform_id, url) in [
            ("source-1", "platform-1", "https://one.example.test"),
            ("source-2", "platform-2", "https://two.example.test"),
        ] {
            sqlx::query("INSERT INTO media_sources (id, platform_id, source_url, normalized_url, source_type, discovered_at) VALUES (?, ?, ?, ?, ?, ?)")
                .bind(source_id)
                .bind(platform_id)
                .bind(url)
                .bind(url)
                .bind("collection")
                .bind("2026-01-01T00:00:00Z")
                .execute(database.pool())
                .await
                .expect("source insert should succeed");
        }
        sqlx::query("INSERT INTO collections (id, source_id, title, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
            .bind("collection-1")
            .bind("source-1")
            .bind("Collection")
            .bind("2026-01-01T00:00:00Z")
            .bind("2026-01-01T00:00:00Z")
            .execute(database.pool())
            .await
            .expect("collection insert should succeed");

        let result = sqlx::query("INSERT INTO media_items (id, source_id, collection_id, canonical_url, title, first_seen_at, last_seen_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind("item-1")
            .bind("source-2")
            .bind("collection-1")
            .bind("https://two.example.test/item")
            .bind("Item")
            .bind("2026-01-01T00:00:00Z")
            .bind("2026-01-01T00:00:00Z")
            .execute(database.pool())
            .await;

        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn app_data_permissions_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempdir().expect("temporary directory should be created");
        let database = Database::from_app_data_dir(directory.path())
            .await
            .expect("file database should initialize");
        assert_eq!(
            std::fs::metadata(directory.path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(database.path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[tokio::test]
    async fn file_database_is_created_inside_requested_directory() {
        let directory = tempdir().expect("temporary directory should be created");
        let database = Database::from_app_data_dir(directory.path())
            .await
            .expect("file database should initialize");

        assert_eq!(database.path(), directory.path().join("umd.sqlite3"));
        assert!(database.path().is_file());
        let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(database.pool())
            .await
            .expect("journal mode query should succeed");
        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
    }
}
