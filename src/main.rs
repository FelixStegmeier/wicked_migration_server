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

const REGISTRY_URL:&str = "registry.opensuse.org/home/jcronenberg/migrate-wicked/containers/opensuse/migrate-wicked-git:latest";
const TABLE_NAME: &str = "entries";
const DEFAULT_DB_PATH: &str = "/var/lib/wicked_migration_server/db.db3";
const FILE_EXPIRATION_IN_SEC: u64 = 5; //* 60;

struct File {
    file_content: String,
    file_name: String,
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

// async fn redirect_post(State(shared_state): State<AppState>, data_string: String) -> Response {
//     let database: tokio::sync::MutexGuard<'_, Connection> = shared_state.database.lock().await;
//     let mut data_arr: Vec<String> = Vec::new();
//     data_arr.push(data_string);
//     let path = match migrate(data_arr) {
//         Ok(path) => path,
//         Err(_e) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
//     };

//     let uuid = match create_and_add_row(path, &database) {
//         Ok(uuid) => uuid,
//         Err(_e) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
//     };
//     println!("{}", uuid);
//     axum::response::Redirect::to(format!("/{}", uuid).as_str()).into_response()
// }

async fn redirect_post_mulipart_form(
    State(shared_state): State<AppState>,
    mut multipart: Multipart,
) -> Response {
    let database: tokio::sync::MutexGuard<'_, Connection> = shared_state.database.lock().await;
    let mut data_array: Vec<File> = Vec::new();

    while let Some(field) = multipart.next_field().await.unwrap() {
        let file_name_ = field.file_name().unwrap().to_string(); //.split(".").collect::<Vec<_>>()[1].to_string(); //kann index out of bounds wenn kein . drinnen ist
        let data = field.bytes().await.unwrap();

        let content_string = match str::from_utf8(&data) {
            Ok(v) => v.to_string(),
            Err(e) => panic!("Invalid UTF-8 sequence: {}", e),
        };

        data_array.push(File {
            file_content: content_string,
            file_name: file_name_,
        });
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

fn migrate(data_arr: Vec<File>) -> Result<String, anyhow::Error> {
    let output_tmpfile: tempfile::NamedTempFile = tempfile::Builder::new()
        .prefix("nm-migrated.")
        .suffix(".tar")
        .keep(true)
        .tempfile()?;

    let output_path_str: &str = output_tmpfile.path().to_str().unwrap();

    let migration_target_tmpdir: tempfile::TempDir = tempdir()?;

    for file in &data_arr {
        let input_file_path = std::path::Path::new("..")
            .join(migration_target_tmpdir.path())
            .join(&file.file_name);
        println!("input_file_path: {}", input_file_path.to_string_lossy());
        fs::File::create_new(&input_file_path).unwrap();
        std::fs::write(&input_file_path, file.file_content.as_bytes())?;
    }

    let arguments_str = if data_arr[0].file_name.contains("ifcfg") {
        format!(
            //"run --rm -v /home/fstegmeier/tmp/migration_test/asdf:/migration-tmpdir:z {}",
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

    let migrated_file_location =
        format!("{}/NM-migrated", migration_target_tmpdir.path().display());

    let mut command = Command::new("tar");
    command
        .arg("cf")
        .arg(output_path_str)
        .arg("-C")
        .arg(&migrated_file_location)
        .arg(".")
        .output()?;

    Ok(output_path_str.to_string())
}

async fn rm_file_after_expiration(database: &Arc<Mutex<Connection>>) -> Result<(), anyhow::Error> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
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
        .route("/", post(redirect_post_mulipart_form))
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
