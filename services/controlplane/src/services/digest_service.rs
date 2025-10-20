use proto::api::controlplane::{
    GetDigestByNameResponse, SetDigestToNameResponse,
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use std::path::Path;
use tonic::{Response, Status};

pub struct DigestService {
    pool: SqlitePool,
}

impl DigestService {
    pub async fn new(db_path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let parent = db_path.parent().ok_or("Database path has no parent directory")?;

        if !parent.exists() {
            return Err(format!(
                "Parent directory does not exist: {}",
                parent.display()
            ).into());
        }

        let database_url = format!("sqlite://{}?mode=rwc", db_path.display());

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS digests (
                name TEXT PRIMARY KEY,
                digest TEXT NOT NULL,
                created_at INTEGER DEFAULT (strftime('%s', 'now')),
                updated_at INTEGER DEFAULT (strftime('%s', 'now'))
            )
            "#,
        )
        .execute(&pool)
        .await?;
        
        Ok(Self { pool })
    }
    
    pub async fn get_digest_by_name(
        &self,
        key: &str,
    ) -> Result<Response<GetDigestByNameResponse>, Status> {
        let result = sqlx::query_as::<_, (String,)>(
            "SELECT digest FROM digests WHERE name = ?"
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        match result {
            Some((digest,)) => Ok(Response::new(GetDigestByNameResponse { digest })),
            None => Err(Status::not_found(format!("Digest not found for name: {}", key))),
        }
    }

    pub async fn set_digest_by_name(
        &self,
        key: &str,
        digest: &str,
    ) -> Result<Response<SetDigestToNameResponse>, Status> {
        sqlx::query(
            r#"
            INSERT INTO digests (name, digest, updated_at)
            VALUES (?, ?, strftime('%s', 'now'))
            ON CONFLICT(name) DO UPDATE SET
                digest = excluded.digest,
                updated_at = strftime('%s', 'now')
            "#,
        )
        .bind(key)
        .bind(digest)
        .execute(&self.pool)
        .await
        .map_err(|e| Status::internal(format!("Database error: {}", e)))?;
        
        Ok(Response::new(SetDigestToNameResponse { success: true }))
    }
}


