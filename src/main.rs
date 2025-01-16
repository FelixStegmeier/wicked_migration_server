use axum::extract::{Multipart, OriginalUri, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{routing::get, Router};
use clap::Parser;
use core::{panic, str};
use rusqlite::Connection;
use std::fs::{self, create_dir_all};
use std::process::Command;
use std::str::FromStr;
use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
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
            "application/xml" => Ok(FileType::Xml), //////////////////////////////////////////////////////////////////////////////////////////////
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

fn delete_file_from_db(path: &str, uuid: &str, database: &Connection) -> anyhow::Result<()> {
    let mut stmt: rusqlite::Statement<'_> =
        database.prepare(format!("DELETE FROM {} WHERE uuid = (?1)", TABLE_NAME).as_str())?;
    stmt.execute([uuid])?;
    std::fs::remove_dir_all(path)?;
    Ok(())
}

fn migrate_files_write_to_db_return_uuid(
    redirect_path: String,
    files: Vec<File>,
    database: &Connection,
) -> Response {
    let migration_result = match migrate(files) {
        Ok(migration_result) => migration_result,
        Err(_e) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let uuid = match add_migration_result_to_db(
        migration_result.0,
        match String::from_utf8(migration_result.1) {
            Ok(log) => log,
            Err(e) => {
                eprint!("Log was not utf8 encoded: {}", e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        },
        database,
    ) {
        Ok(uuid) => uuid,
        Err(_e) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    axum::response::Redirect::to(format!("{}/{}", redirect_path, uuid).as_str()).into_response()
}

fn read_from_db(uuid: String, database: &Connection) -> anyhow::Result<(String, String)> {
    let mut select_stmt = database.prepare(
        format!(
            "SELECT file_path, log from {} WHERE uuid = (?1)",
            TABLE_NAME
        )
        .as_str(),
    )?;

    let path_log = select_stmt.query_row([&uuid], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(path_log)
}

fn generate_json(log: &str, files: Vec<File>) -> String {
    let mut data = json::JsonValue::new_object();
    data["log"] = log.into();
    data["files"] = json::JsonValue::new_array();
    for file in files {
        let mut file_data = json::JsonValue::new_object();
        file_data["fileName"] = file.file_name.into();
        file_data["fileContent"] = file.file_content.into();
        data["files"].push(file_data).unwrap();
    }
    data.dump()
}

fn file_arr_from_path(dir_path: String) -> Result<Vec<File>, anyhow::Error> {
    let mut file_arr: Vec<File> = vec![];

    let dir = match fs::read_dir(dir_path.clone() + "/NM-migrated/system-connections") {
        Ok(dir) => dir,
        Err(_e) => fs::read_dir(dir_path + "/NM-migrated")?,
    };

    for dir_entry in dir {
        let path = dir_entry?.path();
        let file_type = match path.extension() {
            Some(file_type) => match file_type.to_str().unwrap() {
                "xml" => FileType::Xml,
                _ => FileType::Ifcfg,
            },
            None => {
                return Err(anyhow::anyhow!("File extension was not recognized"));
            }
        };
        let file_contents = std::fs::read(&path).unwrap();
        file_arr.push(File {
            file_content: String::from_utf8(file_contents).unwrap(),
            file_name: path.file_name().unwrap().to_str().unwrap().to_owned(),
            file_type,
        });
    }
    Ok(file_arr)
}

async fn return_config_json(uuid: OriginalUri, State(shared_state): State<AppState>) -> Response {
    let database = shared_state.database.lock().await;

    let uuid_stripped_of_prefix = match uuid.to_string().strip_prefix("/json/") {
        Some(uri) => uri.to_owned(),
        None => uuid.to_string(),
    };

    let path_log: (String, String) =
        read_from_db(uuid_stripped_of_prefix.clone(), &database).unwrap();

    let json_string = generate_json(
        &path_log.1,
        match file_arr_from_path(path_log.0.clone()) {
            Ok(file_arr) => file_arr,
            Err(e) => {
                eprint!("Could not retrieve files from path {}: {}", path_log.0, e);
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        },
    );

    match delete_file_from_db(&path_log.0, &uuid_stripped_of_prefix, &database) {
        Ok(()) => (),
        Err(e) => eprint!("Error when removing directory {}: {}", path_log.0, e),
    };

    drop(database);

    axum::response::Json(json_string).into_response()
}

fn return_as_tar(path: String) -> anyhow::Result<tempfile::NamedTempFile> {
    let output_tmpfile: tempfile::NamedTempFile = tempfile::Builder::new()
        .prefix("nm-migrated.")
        .suffix(".tar")
        .keep(true)
        .tempfile()?;

    let output_path_str: &str = match output_tmpfile.path().to_str() {
        Some(output_path_str) => output_path_str,
        None => return Err(anyhow::anyhow!("Failed to convert path to string")),
    };

    let command_output = Command::new("tar")
        .arg("cf")
        .arg(output_path_str)
        .arg("-C")
        .arg(path)
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
    Ok(output_tmpfile)
}

async fn return_config_file(uuid: OriginalUri, State(shared_state): State<AppState>) -> Response {
    let database = shared_state.database.lock().await;

    let uuid_stripped_of_prefix = match uuid.to_string().strip_prefix("/") {
        Some(uri) => uri.to_owned(),
        None => uuid.to_string(),
    };

    let path_log: (String, String) = match read_from_db(uuid_stripped_of_prefix.clone(), &database)
    {
        Ok(path_log) => path_log,
        Err(e) => {
            eprint!("Error when attempting to retrieve entry from DB: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let tar_tempfile = match return_as_tar(path_log.0.clone() + "/NM-migrated") {
        Ok(tar_tempfile) => tar_tempfile,
        Err(e) => {
            eprint!("{e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let file_contents = match get_file_contents(tar_tempfile.path().to_owned()) {
        Ok(file_contents) => file_contents,
        Err(e) => {
            eprintln!(
                "Error when attempting to retrieve tar from {}: {e}",
                tar_tempfile.path().to_string_lossy()
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    match tar_tempfile.close() {
        Ok(()) => (),
        Err(e) => eprint!("failed to delete tempfile: {e}"),
    };
    match delete_file_from_db(&path_log.0, &uuid_stripped_of_prefix, &database) {
        Ok(()) => (),
        Err(e) => eprint!("Error when removing directory {}: {}", path_log.0, e),
    };

    drop(database);

    file_contents.into_response()
}

fn get_file_contents(path: std::path::PathBuf) -> Result<String, anyhow::Error> {
    let contents = std::fs::read_to_string(path)?;
    Ok(contents.to_string())
}

fn add_migration_result_to_db(
    dir_path: String,
    log: String,
    database: &Connection,
) -> anyhow::Result<String> {
    let uuid = uuid::Uuid::new_v4().to_string();

    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs()
        .to_string();

    let mut add_stmt = database.prepare(
        format!(
            "INSERT INTO {} (uuid, file_path, log, creation_time) VALUES (?1, ?2, ?3, ?4)",
            TABLE_NAME
        )
        .as_str(),
    )?;

    add_stmt.execute([&uuid, &dir_path, &log, &time])?;
    Ok(uuid)
}

async fn read_multipart_post_data_to_file_arr(
    mut multipart: Multipart,
) -> Result<Vec<File>, anyhow::Error> {
    let mut data_array: Vec<File> = Vec::new();

    while let Some(field) = multipart.next_field().await.unwrap() {
        let file_type = match field.content_type() {
            Some(file_type) => file_type,
            None => return Err(anyhow::anyhow!("Type missing in multipart/form data")),
        };

        let file_type = FileType::from_str(file_type)?;

        let file_name = match field.file_name() {
            Some(file_name) => file_name.to_string(),
            None => {
                return Err(anyhow::anyhow!(
                    "file name field missing in multipart/form data"
                ))
            }
        };

        let data = field.bytes().await?;

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
    Ok(data_array)
}

async fn redirect_post_multipart_form(
    uri: OriginalUri,
    State(shared_state): State<AppState>,
    multipart: Multipart,
) -> Response {
    let database: tokio::sync::MutexGuard<'_, Connection> = shared_state.database.lock().await;

    let data_array = match read_multipart_post_data_to_file_arr(multipart).await {
        Ok(ok) => ok,
        Err(e) => {
            eprint!("An error occurred when trying to read incoming data: {e}");
            return Response::builder()
                .status(400)
                .header("Content-Type", "text/plain")
                .body(format!("An error occured: {}", e).into())
                .unwrap();
        }
    };

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

    if uri.to_string() == "/json" {
        migrate_files_write_to_db_return_uuid("/json".to_string(), data_array, &database)
    } else {
        migrate_files_write_to_db_return_uuid("".to_string(), data_array, &database)
    }
}

async fn redirect(State(shared_state): State<AppState>, data_string: String) -> Response {
    let database: tokio::sync::MutexGuard<'_, Connection> = shared_state.database.lock().await;
    let data_arr: Vec<File> = vec![File {
        file_content: data_string,
        file_name: "wicked.xml".to_string(),
        file_type: FileType::Xml,
    }];
    migrate_files_write_to_db_return_uuid("".to_string(), data_arr, &database)
}

fn create_and_write_to_file(
    files: &Vec<File>,
    migration_target_path: String,
) -> Result<(), anyhow::Error> {
    for file in files {
        let input_file_path = migration_target_path.clone() + "/" + &file.file_name;
        fs::File::create_new(&input_file_path)?;
        std::fs::write(&input_file_path, file.file_content.as_bytes())?;
    }
    Ok(())
}

//migrates the files and returns the output for logging in the result
fn migrate_files(
    files: &Vec<File>,
    migration_target_path: String,
) -> Result<std::process::Output, anyhow::Error> {
    create_and_write_to_file(files, migration_target_path.clone())?;

    let arguments_str = if files[0].file_type == FileType::Ifcfg {
        format!(
            "run -e \"MIGRATE_WICKED_CONTINUE_MIGRATION=true\" --rm -v {}:/etc/sysconfig/network:z {}",
            migration_target_path.clone(),
                REGISTRY_URL
        )
    } else {
        format!("run --rm -v {}:/migration-tmpdir:z {} bash -c
            \"migrate-wicked migrate -c /migration-tmpdir/ && cp -r /etc/NetworkManager/system-connections /migration-tmpdir/NM-migrated\"",
            migration_target_path,
                REGISTRY_URL,
        )
    };

    let output: std::process::Output = Command::new("podman")
        .args(shlex::split(&arguments_str).unwrap())
        .output()?;

    if cfg!(debug_assertions) {
        eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(output)
}

fn migrate(files: Vec<File>) -> Result<(String, Vec<u8>), anyhow::Error> {
    let migration_target_path = "/tmp/".to_owned() + &uuid::Uuid::new_v4().to_string();
    fs::DirBuilder::new().create(&migration_target_path)?;

    let output: std::process::Output = migrate_files(&files, migration_target_path.clone())?;

    Ok((migration_target_path, output.stderr))
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
        if let Err(e) = std::fs::remove_dir_all(path.clone()) {
            eprintln!("Error when removing directory from {path}: {e}");
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

async fn browser_html() -> Response {
    axum::response::Html(fs::read_to_string("static/main.html").unwrap()).into_response()
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
                log TEXT,
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
        .route("/:uuid", get(return_config_file))
        .route("/json/:uuid", get(return_config_json))
        .route("/", get(browser_html))
        .route_service("/style.css", ServeFile::new("static/style.css"))
        .route_service("/script.js", ServeFile::new("static/script.js"))
        .route_service("/tar_writer.js", ServeFile::new("static/tar_writer.js"))
        .route("/multipart", post(redirect_post_multipart_form))
        .route("/json", post(redirect_post_multipart_form))
        .route("/", post(redirect))
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();
}
