#[macro_use]
extern crate rocket;
use rand::Rng;
use rocket::data::{Data, ToByteUnit};
use rocket::http::Status;
use rocket::response::status;
use rocket::Build;
use rocket::Rocket;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tempfile::{self, tempdir};
use tokio::sync::Mutex;

const REGISTRY_URL:&str = "registry.opensuse.org/home/jcronenberg/migrate-wicked/containers/opensuse/migrate-wicked-git:latest";
const TABLE_NAME: &str = "entries";
const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

struct Entry {
    uuid: String,
    file_path: String,
    _creation_time: String,
}

fn get_row(uuid: &str, database: &Connection) -> rusqlite::Result<Entry> {
    let mut select_stmt = database.prepare(
        format!(
            "SELECT uuid, file_path, creation_time FROM {} WHERE uuid = (?1)",
            TABLE_NAME
        )
        .as_str(),
    )?;

    let row = select_stmt.query_row([&uuid], |row| {
        Ok(Entry {
            uuid: row.get(0)?,
            file_path: row.get(1)?,
            _creation_time: row.get(2)?,
        })
    })?;
    Ok(row)
}

#[get("/<path>")]
async fn return_config_file(
    path: PathBuf,
    shared_state: &rocket::State<Arc<Mutex<rusqlite::Connection>>>,
) -> status::Custom<String> {
    let database = shared_state.lock().await;

    let row = match get_row(&path.display().to_string(), &database) {
        Ok(row) => row,
        Err(e) => {
            return status::Custom(
                Status::BadRequest,
                format!("Error when attempting to access and read file: {}", e),
            )
        }
    };

    drop(database);

    let file_contents = match get_file_contents(Path::new("/tmp/").join(row.file_path)) {
        Ok(file_contents) => file_contents,
        Err(e) => {
            return status::Custom(
                Status::BadRequest,
                format!("Error when attempting to access and read file: {}", e),
            )
        }
    };
    status::Custom(Status::Ok, file_contents)
}

fn get_file_contents(path: PathBuf) -> Result<String, anyhow::Error> {
    let contents = std::fs::read_to_string(path)?;
    Ok(contents.to_string())
}

fn uuid_exists_in_table(uuid: &str, database: &Connection) -> rusqlite::Result<bool> {
    let mut query_exists_stmt =
        database.prepare(format!("SELECT uuid FROM {} WHERE uuid = (?1)", TABLE_NAME).as_str())?;
    let query_result = query_exists_stmt.query([&uuid])?;

    let rows = query_result.mapped(|row| {
        Ok(Entry {

            uuid: row.get(0)?,
            file_path: row.get(1)?,
            _creation_time: row.get(2)?,
        })
    });
    Ok(rows.count() > 0)
}

fn create_and_add_row(path: String, database: &Connection) -> rusqlite::Result<String> {
    let mut uuid = uuid::Uuid::new_v4().to_string();

    while uuid_exists_in_table(&uuid, &database)? {
        uuid = uuid::Uuid::new_v4().to_string();
    }
    let time: String = chrono::Local::now().format(TIME_FORMAT).to_string();

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

#[post("/", data = "<data>")]
async fn receive_data(
    data: Data<'_>,
    shared_state: &rocket::State<Arc<Mutex<rusqlite::Connection>>>,
) -> status::Custom<String> {
    let data_string: rocket::data::Capped<String> =
        match data.open(10.mebibytes()).into_string().await {
            Ok(str) => str,
            Err(e) => {
                println!("Error when retrieving data: {e}");
                return status::Custom(
                    Status::BadRequest,
                    format!("Error when receiving data: {}", e),
                );
            }
        };

    let path = match migrate(data_string.to_string()) {
        Ok(path) => path,
        Err(e) => {
            return status::Custom(Status::BadRequest, format!("Error failed migration: {}", e))
        }
    };


    let database: tokio::sync::MutexGuard<'_, Connection> = shared_state.lock().await;

    let uuid = match create_and_add_row(path, &database) {
        Ok(uuid) => uuid,
        Err(e) => {
            return status::Custom(
                Status::BadRequest,
                format!("Error failed adding entry in database: {}", e),
            )
        }
    };
    drop(database);
    status::Custom(Status::Created, uuid)
}

#[post("/download", data = "<data>")]
async fn redirect(
    data: Data<'_>,
    shared_state: &rocket::State<Arc<Mutex<rusqlite::Connection>>>,
) -> Result<rocket::response::Redirect, status::Custom<String>> {
    let data_string: rocket::data::Capped<String> =
        match data.open(10.mebibytes()).into_string().await {
            Ok(str) => str,
            Err(e) => {
                return Err(status::Custom(
                    Status::InternalServerError,
                    format!("Error when retrieving data: {}", e),
                ))
            }
        };

    let database: tokio::sync::MutexGuard<'_, Connection> = shared_state.lock().await;

    let path = match migrate(data_string.to_string()) {
        Ok(path) => path,
        Err(e) => {
            return Err(status::Custom(
                Status::InternalServerError,
                format!("Error when migrating: {}", e),
            ))
        }
    };

    let uuid = match create_and_add_row(path, &database) {
        Ok(uuid) => uuid,
        Err(e) => {
            return Err(status::Custom(
                Status::InternalServerError,
                format!("Error when creating database: {}", e),
            ))
        }
    };
    Ok(rocket::response::Redirect::to(format!("/{}", uuid)))
}

fn migrate(data_string: String) -> Result<String, anyhow::Error> {
    let tmp_dir: tempfile::TempDir = tempdir()?;

    let input_tmpfile: tempfile::NamedTempFile = tempfile::Builder::new()
        .suffix(".xml")
        .tempfile_in(tmp_dir.path())?;

    std::fs::write(&input_tmpfile, data_string.as_bytes())?;

    let output_tmpfile: tempfile::NamedTempFile = tempfile::Builder::new()
        .prefix("nm-migrated.")
        .suffix(".tar")
        .keep(true)
        .tempfile()?;
    let output_path_str: &str = output_tmpfile.path().to_str().unwrap();

    let input_path_filename: &str = input_tmpfile
        .path()
        .file_name()
        .ok_or(anyhow::anyhow!("Invalid filename"))?
        .to_str()
        .ok_or(anyhow::anyhow!("Invalid filename"))?;

    let arguments_str = format!("run --rm -v {}:/migration-tmpdir:z {} bash -c 
        \"migrate-wicked migrate -c /migration-tmpdir/{} && cp -r /etc/NetworkManager/system-connections /migration-tmpdir/NM-migrated\"", 
        tmp_dir.path().display(),
        REGISTRY_URL,
        input_path_filename
    );

    let command_output = Command::new("podman")
        .args(shlex::split(&arguments_str).unwrap())
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

    let migrated_file_location = format!("{}/NM-migrated", tmp_dir.path().display());

    Command::new("tar")
        .args(["cf", output_path_str, "-C", &migrated_file_location, "."])
        .output()?;

    Ok(output_path_str.to_string())
}

#[launch]
fn rocket() -> Rocket<Build> {
    let database = Connection::open_in_memory().unwrap();
    database
        .execute(
            format!(
                "CREATE TABLE IF NOT EXISTS {} (
                uuid TEXT PRIMARY KEY,
                file_path TEXT NOT NULL,
                creation_time TEXT
                )",
                TABLE_NAME
            )
            .as_str(),
            (),
        )
        .unwrap();

    let db_data = Arc::new(Mutex::new(database));

    rocket::build()
        .mount("/", routes![receive_data, return_config_file, redirect])
        .manage(db_data)
}
