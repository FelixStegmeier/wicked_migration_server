#[macro_use]
extern crate rocket;
use core::time;
use std::alloc::System;
use rocket::data::{Data, ToByteUnit};
use rocket::futures::future::Shared;
use rocket::http::Status;
use rocket::response::status;
use rocket::Build;
use rocket::Rocket;
use rusqlite::{Connection, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{self, tempdir};
use std::time::SystemTime;
//use std::sync::{Arc, Mutex};
use rand::Rng;
use rusqlite::params;
use std::sync::Arc;
use tokio::sync::Mutex;

const REGISTRY_URL:&str = "registry.opensuse.org/home/jcronenberg/migrate-wicked/containers/opensuse/migrate-wicked-git:latest";
const TABLE_NAME: &str = "entries";
const TIME_FORMAT:&str = "%Y-%m-%d %H:%M:%S";

//in Post file zurückgeben
//sqlite table mit id, filepath, timestamp oder einfach löschen, wenn der Verweis weg ist
//richtiges File zurückgeben

struct state {
    id: String,
    file_path: String,
    creation_time: String,
}

#[get("/<path>")]
async fn return_config_file(path: PathBuf) -> status::Custom<String> {
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

    let path = match do_migration(data_string.to_string()) {
        Ok(path) => path,
        Err(e) => {
            return status::Custom(Status::BadRequest, format!("Error failed migration: {}", e))
        }
    };

    let id = rand::thread_rng().gen_range(0..10000);
    let time: String = chrono::Local::now().format(TIME_FORMAT).to_string();
    let id_s = format!("{:0>4}", id);
    database.execute(format!(
        "SELECT EXISTS(SELECT 1 FROM {} WHERE (?1) = '{}')",
        TABLE_NAME, id_s
    ).as_str(), &[&id_s])
    .unwrap();

    //let mut stmt = a.prepare(format!("SELECT EXISTS(SELECT 1 FROM {} WHERE id = ?)", TABLE_NAME).as_str()).unwrap();
    //let exists: bool = stmt.query_row(params![id_s], |row| row.get(0)).unwrap();
    //println!("{}", exists);

    let query = format!("SELECT EXISTS(SELECT 1 FROM {} WHERE id =  1)", TABLE_NAME);

    let exists: bool = database
        .query_row(&query, params![id_s], |row| row.get(0))
        .unwrap_or(false); // returns false if an error occurs
    println!("id exists in database: {}", exists);
    
    let mut stmt = database
        .prepare(format!("SELECT id, file_path, creation_time FROM {}", TABLE_NAME).as_str())
        .unwrap();

    let rows = stmt
        .query_map([], |row| {
            Ok(state {
                id: row.get(0)?,
                file_path: row.get(1)?,
                creation_time: row.get(2)?,
            })
        })
        .unwrap();

    for row in rows {
        let r = row.unwrap();
        println!("id: {}\nfile_path: {}\ncreation_time: {}", r.id, r.file_path, r.creation_time);
    }
    //

    status::Custom(Status::Created, path)
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
    let path = match do_migration(data_string.to_string()) {
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

fn do_migration(data_string: String) -> Result<String, anyhow::Error> {
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
    Ok(output_tmpfile.path().file_name().unwrap().to_str().unwrap().to_string())
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
