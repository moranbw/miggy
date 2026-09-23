use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "miggy", version, about = "Bare-bones, opinionated wrapper around the sqlx Migrator")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Drop views, run pending migrations, recreate views.
    Migrate {
        /// Project name: used as the `./{project}-migrate` directory prefix and as the
        /// Postgres schema migrations/views are scoped to.
        #[arg(long)]
        project: String,

        /// Postgres connection URL. Falls back to the DATABASE_URL env var.
        #[arg(long, env = "DATABASE_URL")]
        database_url: String,

        /// Additional schema(s) to add to the search_path, ahead of `public`. Repeatable.
        #[arg(long = "extra-schema")]
        extra_schemas: Vec<String>,
    },
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Migrate {
            project,
            database_url,
            extra_schemas,
        } => {
            let extra_schemas: Vec<&str> = extra_schemas.iter().map(String::as_str).collect();
            miggy::migrate(&database_url, &project, &extra_schemas).await
        }
    };

    if let Err(e) = result {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}
