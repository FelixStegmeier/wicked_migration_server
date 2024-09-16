#[macro_use]
extern crate rocket;
use rocket::data::{Data, ToByteUnit};
use rocket::http::Status;
use rocket::response::status;
use rocket::Build;
use rocket::Rocket;
use rusqlite::{Connection, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{self, tempdir};
//use std::sync::{Arc, Mutex};
use rand::Rng;
use std::sync::Arc;
use tokio::sync::Mutex;

const REGISTRY_URL:&str = "registry.opensuse.org/home/jcronenberg/migrate-wicked/containers/opensuse/migrate-wicked-git:latest";
const TABLE_NAME: &str = "entries";
const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

//in Post file zurückgeben
//sqlite table mit id, filepath, timestamp oder einfach löschen, wenn der Verweis weg ist
//richtiges File zurückgeben

struct Entry {
    id: String,
    file_path: String,
    creation_time: String,
}

fn get_row(id_s: &str, database: &Connection) -> Result<Entry> {
    let mut select_stmt = database.prepare(
        format!(
            "SELECT id, file_path, creation_time FROM {} WHERE id = (?1)",
            TABLE_NAME
        )
        .as_str(),
    )?;

    let mut rows = select_stmt.query_map([&id_s], |row| {
        Ok(Entry {
            id: row.get(0)?,
            file_path: row.get(1)?,
            creation_time: row.get(2)?,
        })
    })?;
    let row = rows.nth(0).unwrap()?; //was ist hier der korrekte Weg das option None zu handlen?
    Ok(row)
}

#[get("/<path>")]
async fn return_config_file(
    path: PathBuf,
    shared_state: &rocket::State<Arc<Mutex<rusqlite::Connection>>>,
) -> status::Custom<String> {
    //path = id in DB und holt file_path aus DB
    let database = shared_state.lock().await;

    let id_s = path.file_name().unwrap().to_str().unwrap();

    let row = match get_row(&id_s, &database) {
        Ok(row) => row,
        Err(e) => {
            return status::Custom(
                Status::BadRequest,
                format!("Error when attempting to access and read file: {}", e),
            )
        }
    };
    //println!("{}", row.file_path);

    //////////////////////////////////////////////////////

    let path = row.file_path;

    let file_contents = match get_file_contents(Path::new("/tmp/").join(path)) {
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
fn id_exists_in_table(id_s: &str, database: &Connection) -> Result<bool> {
    let mut query_exists_stmt =
        database.prepare(format!("SELECT id FROM {} WHERE id = (?1)", TABLE_NAME).as_str())?;
    let query_result = query_exists_stmt.query([&id_s])?;

    let rows = query_result.mapped(|row| {
        Ok(Entry {
            id: row.get(0)?,
            file_path: row.get(1)?,
            creation_time: row.get(2)?,
        })
    });
    Ok(if rows.count() > 0 { true } else { false })
}

fn create_and_add_row(path: String, database: &Connection) -> Result<String> {
    let mut id = rand::thread_rng().gen_range(0..1000000000);
    let mut id_s = format!("{:0>9}", id);

    while id_exists_in_table(&id_s, &database)? {
        id = rand::thread_rng().gen_range(0..1000000000);
        id_s = format!("{:0>9}", id);
    }
    let time: String = chrono::Local::now().format(TIME_FORMAT).to_string();

    let mut add_stmt = database.prepare(
        format!(
            "INSERT INTO {} (id, file_path, creation_time) VALUES (?1, ?2, ?3)",
            TABLE_NAME
        )
        .as_str(),
    )?;
    add_stmt.execute([&id_s, &path, &time])?;
    Ok(id_s)
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

    let database: tokio::sync::MutexGuard<'_, Connection> = shared_state.lock().await;

    let path = match migrate(data_string.to_string()) {
        Ok(path) => path,
        Err(e) => {
            return status::Custom(Status::BadRequest, format!("Error failed migration: {}", e))
        }
    };

    let id_s = match create_and_add_row(path, &database) {
        Ok(id_s) => id_s,
        Err(e) => {
            return status::Custom(
                Status::BadRequest,
                format!("Error failed adding entry in database: {}", e),
            )
        }
    };

    status::Custom(Status::Created, id_s)
    //status::Custom(Status::Created, path)
}

#[post("/alternative_post", data = "<data>")]
async fn receive_and_return_data(data: Data<'_>) -> status::Custom<String> //RawHtml<&'static str>
{
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

    let file_contents = match get_file_contents(Path::new("/tmp/").join(path)) {
        Ok(file_contents) => file_contents,
        Err(e) => {
            return status::Custom(
                Status::BadRequest,
                format!("Error when attempting to access and read file: {}", e),
            )
        }
    };
    println!("file_contents: {}", file_contents);
    status::Custom(Status::Ok, file_contents)
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

    //-e MIGRATE_WICKED_CONTINUE_MIGRATION=true
    let arguments_str = format!("run --rm -v {}:/migration-tmpdir:z {} bash -c 
        \"migrate-wicked migrate /migration-tmpdir/{} && cp -r /etc/NetworkManager/system-connections /migration-tmpdir/NM-migrated\"", 
        tmp_dir.path().display().to_string(),
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

    //  /tmp/$tmpdir/NM-migrated, ich weiß nicht, ob das der richtige Path ist
    let migrated_file_location =
        format!("/tmp/{}/NM-migrated", tmp_dir.path().display().to_string()).to_string();

    Command::new("tar")
        .args([
            "cf",
            output_path_str,
            //&format!("/migration-tmpdir/NM-migrated/"),
            "-C",
            // tmp_dir.path().to_str().unwrap(),
            &migrated_file_location,
            // input_path_filename,
        ])
        .output()?;

    println!("output_path_str: {}", output_path_str);
    Ok(output_tmpfile
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string())
}

#[launch]
fn rocket() -> Rocket<Build> {
    let database = Connection::open_in_memory().unwrap(); //error handling, Felix... Error handling
    database
        .execute(
            format!(
                "CREATE TABLE IF NOT EXISTS {} (
            id TEXT PRIMARY KEY,
            file_path TEXT NOT NULL,
            creation_time TEXT
        )",
                TABLE_NAME
            )
            .as_str(),
            (), // empty list of parameters.
        )
        .unwrap();

    let db_data = Arc::new(Mutex::new(database));

    rocket::build()
        .mount(
            "/",
            routes![receive_data, return_config_file, receive_and_return_data],
        )
        .manage(db_data)
}
