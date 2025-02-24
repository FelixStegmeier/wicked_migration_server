mod db_util;
mod files;
mod migration;
mod routes;
use axum::{
    routing::{get, post},
    Router,
};
use clap::Parser;
use db_util::{create_db, rm_file_after_expiration};
use migration::pull_latest_migration_image;
use routes::{redirect, redirect_post_multipart_form, return_config_file, return_config_json};
use rusqlite::Connection;
use std::{cmp::max, fs::create_dir_all, num::NonZeroUsize, sync::Arc, thread};
use tokio::{runtime::Builder, sync::Mutex};
use tower_http::services::ServeDir;

const DEFAULT_DB_PATH: &str = "/var/lib/wicked_migration_server/db.db3";
const DEFAULT_STATIC_FILE_PATH: &str = "./static";
const DEFAULT_IP_ADDRESS: &str = "::";
const DEFAULT_PORT: &str = "8080";

#[derive(Parser)]
#[command(about = "Server to host Wicked config migration", long_about = None)]
struct Args {
    /// Path where the database is located, or is created if it doesn't exist.
    #[arg(long, short, default_value_t = DEFAULT_DB_PATH.to_string())]
    db_path: String,
    /// Path where the static files are located.
    #[arg(long, short, default_value_t = DEFAULT_STATIC_FILE_PATH.to_string())]
    static_path: String,
    /// IP address the server will bind to
    #[arg(long, short, default_value_t = DEFAULT_IP_ADDRESS.to_string())]
    ip_address: String,
    /// Port the server will listen on.
    #[arg(long, short, default_value_t = DEFAULT_PORT.to_string())]
    port: String,
}

#[derive(Clone)]
struct AppState {
    database: Arc<Mutex<Connection>>,
}

async fn async_jobs(db_clone: Arc<Mutex<Connection>>) -> ! {
    let mut loop_counter: u32 = 60;
    loop {
        match rm_file_after_expiration(&db_clone).await {
            Ok(ok) => ok,
            Err(e) => eprintln!("Error when running file cleanup: {}", e),
        };

        loop_counter += 1;
        if loop_counter >= 60 {
            loop_counter = 0;
            if let Err(e) = pull_latest_migration_image() {
                eprintln!("Failed to pull newest migration image: {e}");
            }
        }

        std::thread::sleep(std::time::Duration::from_secs(15));
    }
}

async fn async_main() {
    let args = Args::parse();

    let db_path = args.db_path;
    let static_path = args.static_path;
    let bind_addr = format!("{}:{}", args.ip_address, &args.port);

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

    tokio::spawn(async_jobs(db_data.clone()));

    let app_state = AppState { database: db_data };

    let app = Router::new()
        .route("/tar/:uuid", get(return_config_file))
        .route("/json/:uuid", get(return_config_json))
        .route("/multipart", post(redirect_post_multipart_form))
        .route("/json", post(redirect_post_multipart_form))
        .route("/", post(redirect))
        .route("/", axum::routing::get_service(ServeDir::new(&static_path)))
        .fallback_service(ServeDir::new(static_path))
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind(bind_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

fn main() {
    let num_cores = thread::available_parallelism().map_or(1, NonZeroUsize::get);

    let worker_threads = max(2, num_cores);

    let runtime = Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .enable_all()
        .build()
        .expect("Failed to build Tokio runtime");

    runtime.block_on(async_main());
}
