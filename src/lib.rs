use std::fs;
use std::path::{Path, PathBuf};

use log::{error, info};
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, Executor};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Runs database migrations for a project.
///
/// This function:
/// 1. Drops existing views in the project schema
/// 2. Runs all pending migrations from `./{project}-migrate/migrations`
/// 3. Recreates views from `./{project}-migrate/views`
///
/// # Arguments
///
/// * `database_url` - PostgreSQL connection URL
/// * `project` - Project name; used both as the migration directory prefix and as the
///   Postgres schema migrations/views are scoped to
/// * `extra_schemas` - Additional schemas to add to the search_path, ahead of `public`
///
/// # Migration Directory Structure
///
/// ```text
/// ./{project}-migrate/
///   migrations/
///     001_initial.sql
///     002_add_table.sql
///   views/
///     my_view.sql
///     another_view.sql
/// ```
pub async fn migrate(database_url: &str, project: &str, extra_schemas: &[&str]) -> Result<(), Error> {
    info!("Connecting to database...");
    let mut schemas = vec![project];
    schemas.extend_from_slice(extra_schemas);
    schemas.push("public");
    let create_schema = format!("CREATE SCHEMA IF NOT EXISTS {};", project);
    let search_path = format!(
        "SET search_path = {};",
        schemas
            .iter()
            .map(|schema| format!("'{}'", schema))
            .collect::<Vec<_>>()
            .join(",")
    );
    let postgres_pool = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |conn, _meta| {
            let create_schema = create_schema.clone();
            let search_path = search_path.clone();
            Box::pin(async move {
                conn.execute(AssertSqlSafe(create_schema)).await?;
                conn.execute(AssertSqlSafe(search_path)).await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await?;
    let views = list_view_files(format!("./{}-migrate/views", &project).as_str())?;
    for view in &views[..] {
        sqlx::raw_sql(AssertSqlSafe(format!(
            "DROP VIEW IF EXISTS {}.{}",
            &project, &view.0
        )))
        .execute(&postgres_pool)
        .await?;
        info!("{} view dropped", &view.0);
    }
    info!("Creating Migrator...");
    let migrator = match Migrator::new(Path::new(
        format!("./{}-migrate/migrations", &project).as_str(),
    ))
    .await
    {
        Ok(migrator) => migrator,
        Err(e) => {
            error!("Error creating migrator: {}", e);
            recreate_views(&postgres_pool, &views).await?;
            return Err(Error::Migrate(e));
        }
    };
    info!("Running migrations...");
    match migrator.run(&postgres_pool).await {
        Ok(()) => info!("Migrations applied successfully."),
        Err(e) => {
            error!("Error applying migrations: {}", e);
            recreate_views(&postgres_pool, &views).await?;
            return Err(Error::Migrate(e));
        }
    };

    recreate_views(&postgres_pool, &views).await?;

    Ok(())
}

/// Scaffolds a new, empty migration file under `./{project}-migrate/migrations`,
/// prefixed with a `YYYYMMDDHHMMSS` timestamp so migrations sort chronologically and
/// concurrent branches don't collide on the same version number.
///
/// Returns the path of the created file.
pub fn add_migration(project: &str, name: &str) -> Result<PathBuf, Error> {
    let dir = format!("./{}-migrate/migrations", project);
    fs::create_dir_all(&dir)?;
    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let slug = name.trim().replace(' ', "_");
    let path = Path::new(&dir).join(format!("{timestamp}_{slug}.sql"));
    fs::write(&path, "-- Add migration script here\n")?;
    Ok(path)
}

async fn recreate_views(postgres_pool: &sqlx::PgPool, views: &[(String, String)]) -> Result<(), Error> {
    for view in &views[..] {
        sqlx::raw_sql(AssertSqlSafe(view.1.clone()))
            .execute(postgres_pool)
            .await?;
        info!("{} view created", &view.0);
    }

    Ok(())
}

fn list_view_files(dir: &str) -> Result<Vec<(String, String)>, Error> {
    let mut names = Vec::new();

    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(names),
        Err(e) => return Err(e.into()),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("sql") {
            if let (Some(file_name), Ok(contents)) = (
                path.file_stem().and_then(|s| s.to_str()),
                fs::read_to_string(&path),
            ) {
                names.push((file_name.to_string(), contents));
            }
        }
    }

    Ok(names)
}
