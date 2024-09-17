#[macro_use]
extern crate rocket;
use rand::Rng;
use rocket::data::{Data, ToByteUnit};
use rocket::http::Status;
use rocket::response::status;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tempfile::{self, tempdir};
use tokio::sync::Mutex;

const REGISTRY_URL:&str = "registry.opensuse.org/home/jcronenberg/migrate-wicked/containers/opensuse/migrate-wicked-git:latest";
const TABLE_NAME: &str = "entries";
const TIME_FORMAT: &str = "%Y %b %d %H:%M:%S%.3f %z";
const FILE_EXPIRATION_IN_SEC: i64 = 5*60;
struct Entry {
    uuid: String,
    file_path: String,
    creation_time: String,
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
            creation_time: row.get(2)?,
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

fn create_and_add_row(path: String, database: &Connection) -> rusqlite::Result<String> {
    let uuid = uuid::Uuid::new_v4().to_string();

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

async fn rm_file_after_expiration(database: &Arc<Mutex<Connection>>) -> Result<(), anyhow::Error> {
    let database = database.lock().await;
    let mut stmt = database.prepare(format!("SELECT * FROM {}", TABLE_NAME).as_str())?;
    let rows = stmt.query([])?;
    let rows = rows.mapped(|row| {
        Ok(Entry {
            uuid: row.get(0)?,
            file_path: row.get(1)?,
            creation_time: row.get(2)?,
        })
    });
    for row in rows {
        let row = row?;
        let time_passed_since_creation = chrono::Local::now().time()
            - chrono::DateTime::parse_from_str(&row.creation_time.as_str(), TIME_FORMAT)?.time();
        let t_delta = match time_passed_since_creation
            .checked_sub(&chrono::TimeDelta::new(FILE_EXPIRATION_IN_SEC, 0).unwrap())
        {
            Some(t_delta) => t_delta,
            None => chrono::TimeDelta::zero(),
        };
        if t_delta > chrono::TimeDelta::zero() {
            std::fs::remove_file(row.file_path)?;

            let mut stmt = database
                .prepare(format!("DELETE FROM {} WHERE uuid = (?1)", TABLE_NAME).as_str())?;
            stmt.execute([row.uuid])?;
        }
    }
    Ok(())
}

async fn async_db_cleanup(db_clone: Arc<Mutex<Connection>>) {
    loop {
        match rm_file_after_expiration(&db_clone).await {
            Ok(ok) => ok,
            Err(e) => println!("{}", e),
        };
        rocket::tokio::time::sleep(std::time::Duration::from_secs(3*60)).await;
    }
}

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
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
    let db_clone = Arc::clone(&db_data);

    rocket::tokio::spawn(async_db_cleanup(db_clone));

    rocket::build()
        .mount("/", routes![receive_data, return_config_file, redirect])
        .manage(db_data)
        .launch()
        .await?;
    Ok(())
}
