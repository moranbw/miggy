<p align="center">
  <img src="assets/logo.png" alt="miggy" width="280">
</p>

# miggy

Bare-bones, opinionated wrapper around the [sqlx](https://github.com/launchbadge/sqlx)
Migrator. Points it at a per-project Postgres schema, and rebuilds that schema's SQL
views around every migration run so they never end up stale or pointing at dropped
columns.

Ships as both a library crate (`miggy::migrate`) and a standalone `miggy` binary, so you
can call it from Rust code or from a `justfile`/shell script in any repo.

## What it does

Running a migration for project `foo` does the following, in order:

1. `CREATE SCHEMA IF NOT EXISTS foo` — the schema is created on demand, you don't need
   to provision it yourself.
2. Sets the connection's `search_path` to `foo`, any `extra_schemas` you pass, then
   `public`.
3. Drops every view under `./foo-migrate/views/` (`DROP VIEW IF EXISTS foo.<name>`).
4. Runs all pending migrations from `./foo-migrate/migrations/` via `sqlx::migrate::Migrator`.
5. Recreates every view that was dropped in step 3, from the same SQL read off disk.

If step 4 fails (or the migrations directory itself is missing/invalid), miggy still
recreates the views from the copies it already read into memory before returning the
error — so a broken migration doesn't leave your schema without its views.

This is deliberately opinionated: every project gets its own schema, no exceptions. If
you want unscoped/`public`-schema migrations, use [`sqlx-cli`](https://github.com/launchbadge/sqlx/tree/main/sqlx-cli)
directly instead.

## Directory layout

miggy expects a `{project}-migrate/` directory relative to wherever it's run from,
containing a `migrations/` folder and a `views/` folder:

```text
./foo-migrate/
  migrations/
    001_initial.sql
    002_add_table.sql
  views/
    widget_names.sql
    another_view.sql
```

- **`migrations/`** — plain sqlx migration files, run in order and tracked in that
  project's `_sqlx_migrations` table. See the
  [sqlx migrate docs](https://docs.rs/sqlx/latest/sqlx/migrate/index.html) for naming
  rules.
- **`views/`** — one `CREATE VIEW ...` statement per file. The file name (minus `.sql`)
  is treated as the view name for the drop step, so `widget_names.sql` must define a
  view named `widget_names`.

Put this directory in your project's repo root (or wherever your `just`/build tooling
runs commands from) — miggy doesn't care where it lives as long as the relative path
resolves.

## Installing / running

For now, build it from source (a `cargo-binstall`-installable release is planned once
this repo is pushed to GitHub with release CI set up):

```sh
cargo install --path . --locked
```

or just run it in place with `cargo run --bin miggy -- ...` from this repo.

## CLI usage

```sh
miggy migrate --project foo --database-url postgres://user:pass@localhost/mydb
```

- `--project <name>` (required) — directory prefix (`./{project}-migrate`) and the
  Postgres schema to scope everything to.
- `--database-url <url>` — falls back to the `DATABASE_URL` env var if omitted.
- `--extra-schema <schema>` (repeatable) — additional schema(s) to add to the
  `search_path`, ahead of `public`. Omit unless you actually need cross-schema access
  (e.g. views that reference tables in another schema).

Typical `justfile` recipe:

```just
migrate:
    miggy migrate --project foo --database-url {{env_var('DATABASE_URL')}}
```

### Logging

miggy is silent by default. Set `RUST_LOG` to see progress (schema/view/migration
steps) via [`env_logger`](https://docs.rs/env_logger):

```sh
RUST_LOG=info miggy migrate --project foo
```

Fatal errors are always printed to stderr regardless of `RUST_LOG`.

## Library usage

```rust
miggy::migrate(
    "postgres://user:pass@localhost/mydb",
    "foo",
    &[], // extra_schemas
).await?;
```

## Requirements

- PostgreSQL only — no other backends are supported. The schema-scoping and view
  drop/recreate flow don't map cleanly onto engines without Postgres-style schemas.
