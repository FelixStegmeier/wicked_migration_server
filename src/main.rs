use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{routing::get, Router};
use clap::Parser;
use core::str;
use rusqlite::Connection;
use std::fs::{self, create_dir_all};
use std::process::Command;
use std::str::FromStr;
use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tempfile::{self, tempdir};
use tokio::sync::Mutex;
use tower_http::services::ServeFile;

const REGISTRY_URL:&str = "registry.opensuse.org/home/jcronenberg/migrate-wicked/containers/opensuse/migrate-wicked-git:latest";
const TABLE_NAME: &str = "entries";
const DEFAULT_DB_PATH: &str = "/var/lib/wicked_migration_server/db.db3";
const FILE_EXPIRATION_IN_SEC: u64 = 5 * 60;

#[derive(PartialEq)]
enum FileType {
    Xml,
    Ifcfg,
}

impl FromStr for FileType {
    type Err = anyhow::Error;
    fn from_str(file_type: &str) -> Result<Self, Self::Err> {
        match file_type {
            "text/xml" => Ok(FileType::Xml),
            "text/plain" => Ok(FileType::Ifcfg),
            "application/octet-stream" => Ok(FileType::Ifcfg),
            _ => Err(anyhow::anyhow!("Unsupported file type: {}", file_type)),
        }
    }
}

struct File {
    file_content: String,
    file_name: String,
    file_type: FileType,
}

fn get_file_path_from_db(uuid: &str, database: &Connection) -> anyhow::Result<String> {
    let mut select_stmt = database
        .prepare(format!("SELECT file_path FROM {} WHERE uuid = (?1)", TABLE_NAME).as_str())?;

    let file_path = select_stmt.query_row([&uuid], |row| Ok(row.get(0)))?;
    Ok(file_path?)
}

async fn return_config_file_get(
    Path(path): Path<String>,
    State(shared_state): State<AppState>,
) -> Response {
    let database = shared_state.database.lock().await;
    let file_path = match get_file_path_from_db(
        &std::path::PathBuf::from_str(&path)
            .unwrap()
            .display()
            .to_string(),
        &database,
    ) {
        Ok(file_path) => file_path,
        Err(_e) => {
            return StatusCode::BAD_REQUEST.into_response();
        }
    };

    drop(database);

    let file_contents = match get_file_contents(std::path::Path::new("/tmp/").join(file_path)) {
        Ok(file_contents) => file_contents,
        Err(_e) => {
            return StatusCode::BAD_REQUEST.into_response();
        }
    };
    file_contents.into_response()
}

fn get_file_contents(path: std::path::PathBuf) -> Result<String, anyhow::Error> {
    let contents = std::fs::read_to_string(path)?;
    Ok(contents.to_string())
}

fn create_and_add_row(path: String, database: &Connection) -> anyhow::Result<String> {
    let uuid = uuid::Uuid::new_v4().to_string();

    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs()
        .to_string();

    let mut add_stmt = database.prepare(
        format!(
            "INSERT INTO {} (uuid, file_path, creation_time) VALUES (?1, ?2, ?3)",
            TABLE_NAME
        )
        .as_str(),
    )?;
    add_stmt.execute([&uuid, &path, &time])?;
    Ok(uuid)
}

async fn redirect_post_mulipart_form(
    State(shared_state): State<AppState>,
    mut multipart: Multipart,
) -> Response {
    let database: tokio::sync::MutexGuard<'_, Connection> = shared_state.database.lock().await;
    let mut data_array: Vec<File> = Vec::new();

    while let Some(field) = multipart.next_field().await.unwrap() {
        let file_type = match field.content_type() {
            Some(file_type) => file_type,
            None => {
                return Response::builder()
                    .status(400)
                    .header("Content-Type", "text/plain")
                    .body("Type missing in multipart/form data".into())
                    .unwrap()
            }
        };

        let file_type = match FileType::from_str(file_type) {
            Ok(file_type) => file_type,
            Err(e) => {
                return Response::builder()
                    .status(400)
                    .header("Content-Type", "text/plain")
                    .body(format!("Error when parsing file type: {}", e).into())
                    .unwrap()
            }
        };

        let file_name = match field.file_name() {
            Some(file_name) => file_name.to_string(),
            None => {
                return Response::builder()
                    .status(400)
                    .header("Content-Type", "text/plain")
                    .body("file name field missing in multipart/form data".into())
                    .unwrap()
            }
        };

        let data = match field.bytes().await {
            Ok(data) => data,
            Err(e) => {
                return Response::builder()
                    .status(500)
                    .header("Content-Type", "text/plain")
                    .body(format!("Server was unable to read file: {}", e).into())
                    .unwrap()
            }
        };

        let file_content = match str::from_utf8(&data) {
            Ok(v) => v.to_string(),
            Err(e) => panic!("Invalid UTF-8 sequence: {}", e),
        };

        data_array.push(File {
            file_content,
            file_name,
            file_type,
        });
    }

    if !data_array
        .windows(2)
        .all(|elements| elements[0].file_type == elements[1].file_type)
    {
        return Response::builder()
            .status(400)
            .header("Content-Type", "text/plain")
            .body("File types not uniform, please dont mix ifcfg and .xml files".into())
            .unwrap();
    }

    let path = match migrate(data_array) {
        Ok(path) => path,
        Err(_e) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let uuid = match create_and_add_row(path, &database) {
        Ok(uuid) => uuid,
        Err(_e) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    axum::response::Redirect::to(format!("/{}", uuid).as_str()).into_response()
}

async fn redirect(State(shared_state): State<AppState>, data_string: String) -> Response {
    let database: tokio::sync::MutexGuard<'_, Connection> = shared_state.database.lock().await;
    let data_arr: Vec<File> = vec![File {
        file_content: data_string,
        file_name: "wicked.xml".to_string(),
        file_type: FileType::Xml,
    }];
    let path = match migrate(data_arr) {
        Ok(path) => path,
        Err(_e) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let uuid = match create_and_add_row(path, &database) {
        Ok(uuid) => uuid,
        Err(_e) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    axum::response::Redirect::to(format!("/{}", uuid).as_str()).into_response()
}

fn migrate(data_arr: Vec<File>) -> Result<String, anyhow::Error> {
    let output_tmpfile: tempfile::NamedTempFile = tempfile::Builder::new()
        .prefix("nm-migrated.")
        .suffix(".tar")
        .keep(true)
        .tempfile()?;

    let output_path_str: &str = match output_tmpfile.path().to_str() {
        Some(output_path_str) => output_path_str,
        None => return Err(anyhow::anyhow!("Failed to convert path to string")),
    };

    let migration_target_tmpdir: tempfile::TempDir = tempdir()?;

    for file in &data_arr {
        let input_file_path = migration_target_tmpdir.path().join(&file.file_name);
        fs::File::create_new(&input_file_path)?;
        std::fs::write(&input_file_path, file.file_content.as_bytes())?;
    }

    let arguments_str = if data_arr[0].file_type == FileType::Ifcfg {
        format!(
            "run -e \"MIGRATE_WICKED_CONTINUE_MIGRATION=true\" --rm -v {}:/etc/sysconfig/network:z {}",
            migration_target_tmpdir.path().display(),
            REGISTRY_URL
        )
    } else {
        format!("run --rm -v {}:/migration-tmpdir:z {} bash -c 
            \"migrate-wicked migrate -c /migration-tmpdir/ && cp -r /etc/NetworkManager/system-connections /migration-tmpdir/NM-migrated\"", 
            migration_target_tmpdir.path().display(),
            REGISTRY_URL,
        )
    };

    let output = Command::new("podman")
        .args(shlex::split(&arguments_str).unwrap())
        .output()?;

    if cfg!(debug_assertions) {
        println!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    }
    let migrated_file_location =
        format!("{}/NM-migrated", migration_target_tmpdir.path().display());

    let command_output = Command::new("tar")
        .arg("cf")
        .arg(output_path_str)
        .arg("-C")
        .arg(&migrated_file_location)
        .arg(".")
        .output()?;

    if cfg!(debug_assertions) {
        println!(
            "stdout: {}",
            String::from_utf8_lossy(&command_output.stdout)
        );
        println!(
            "stderr: {}",
            String::from_utf8_lossy(&command_output.stderr)
        );
    }

    Ok(output_path_str.to_string())
}

async fn rm_file_after_expiration(database: &Arc<Mutex<Connection>>) -> Result<(), anyhow::Error> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let diff = now - FILE_EXPIRATION_IN_SEC;

    let db = database.lock().await;
    let mut stmt =
        db.prepare(format!("SELECT * FROM {} WHERE creation_time < (?1)", TABLE_NAME).as_str())?;
    let rows = stmt.query([diff])?;
    let rows = rows.mapped(|row| Ok((row.get(0), row.get(1))));

    for row in rows {
        let row = row?;
        let uuid: String = row.0?;
        let path: String = row.1?;
        let mut stmt: rusqlite::Statement<'_> =
            db.prepare(format!("DELETE FROM {} WHERE uuid = (?1)", TABLE_NAME).as_str())?;
        stmt.execute([uuid])?;
        if let Err(e) = std::fs::remove_file(path) {
            eprintln!("Error when removing file: {e}");
        }
    }
    Ok(())
}

async fn async_db_cleanup(db_clone: Arc<Mutex<Connection>>) -> ! {
    loop {
        match rm_file_after_expiration(&db_clone).await {
            Ok(ok) => ok,
            Err(e) => eprintln!("Error when running file cleanup: {}", e),
        };
        std::thread::sleep(std::time::Duration::from_secs(15));
    }
}

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

    let database: Connection =
        Connection::open(&db_path).unwrap_or_else(|err| panic!("Couldn't create database: {err}"));

    database
        .execute(
            format!(
                "CREATE TABLE IF NOT EXISTS {} (
                uuid TEXT PRIMARY KEY,
                file_path TEXT NOT NULL,
                creation_time INTEGER
                )",
                TABLE_NAME
            )
            .as_str(),
            (),
        )
        .unwrap();
    let db_data = Arc::new(Mutex::new(database));

    tokio::spawn(async_db_cleanup(db_data.clone()));

    let app_state = AppState { database: db_data };

    let app = Router::new()
        .route("/:uuid", get(return_config_file_get))
        .route("/", get(browser_html))
        .route_service("/style.css", ServeFile::new("static/style.css"))
        .route_service("/script.js", ServeFile::new("static/script.js"))
        .route("/multipart", post(redirect_post_mulipart_form))
        .route("/", post(redirect))
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}

async fn browser_html() -> Response {
    axum::response::Html(fs::read_to_string("static/main.html").unwrap()).into_response()
}
