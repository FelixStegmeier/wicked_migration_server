mod db_util;
mod files;
mod migration;
mod routes;
use axum::{
    routing::{get, post},
    Router,
};
use clap::Parser;
use db_util::{async_db_cleanup, create_db};
use routes::{redirect, redirect_post_multipart_form, return_config_file, return_config_json};
use rusqlite::Connection;
use std::{fs::create_dir_all, sync::Arc};
use tokio::sync::Mutex;
use tower_http::services::ServeDir;

const DEFAULT_DB_PATH: &str = "/var/lib/wicked_migration_server/db.db3";

#[derive(Parser)]
#[command(about = "Server to host Wicked config migration", long_about = None)]
struct Args {
    #[arg(default_value_t = DEFAULT_DB_PATH.to_string())]
    db_path: String,
}
#[derive(Clone)]
struct AppState {
    database: Arc<Mutex<Connection>>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let db_path = args.db_path;

    if db_path == DEFAULT_DB_PATH {
        if let Some(path) = std::path::Path::new(&db_path).parent() {
            if !path.exists() {
                create_dir_all(path)
                    .unwrap_or_else(|err| panic!("Couldn't create db directory: {err}"));
            }
        }
    };

    let database: Connection = create_db(&db_path);

    let db_data: Arc<Mutex<Connection>> = Arc::new(Mutex::new(database));

    tokio::spawn(async_db_cleanup(db_data.clone()));

    let app_state = AppState { database: db_data };

    let app = Router::new()
        .route("/tar/:uuid", get(return_config_file))
        .route("/json/:uuid", get(return_config_json))
        .route("/multipart", post(redirect_post_multipart_form))
        .route("/json", post(redirect_post_multipart_form))
        .route("/", post(redirect))
        .route("/", axum::routing::get_service(ServeDir::new("static/")))
        .fallback_service(ServeDir::new("static/"))
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
