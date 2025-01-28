use rusqlite::Connection;
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

use crate::files::{file_arr_from_path, return_as_tar};

const TABLE_NAME: &str = "entries";
const FILE_EXPIRATION_IN_SEC: u64 = 5 * 60;

pub fn get_tar(uuid: &str, database: &Connection) -> anyhow::Result<String> {
    let path_log: (String, String) = read_from_db(uuid, database)?;

    let tar_tempfile = return_as_tar(path_log.0.clone() + "/NM-migrated")?;

    Ok(std::fs::read_to_string(tar_tempfile.path())?)
}

///Creates a json even if args are left empty
pub fn generate_json(uuid: &str, database: &Connection) -> anyhow::Result<String> {
    let path_log: (String, String) = read_from_db(uuid, database)?;

    let files = file_arr_from_path(path_log.0.clone())?;

    let mut data = json::JsonValue::new_object();
    data["log"] = path_log.1.into();
    data["files"] = json::JsonValue::new_array();
    for file in files {
        let mut file_data = json::JsonValue::new_object();
        file_data["fileName"] = file.file_name.into();
        file_data["fileContent"] = file.file_content.into();
        data["files"].push(file_data).unwrap();
    }

    Ok(data.dump())
}

pub fn create_db(db_path: &str) -> Connection {
    let database =
        Connection::open(db_path).unwrap_or_else(|err| panic!("Couldn't create database: {err}"));
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
    database
}

pub async fn rm_file_after_expiration(
    database: &Arc<Mutex<Connection>>,
) -> Result<(), anyhow::Error> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let diff = now - FILE_EXPIRATION_IN_SEC;

    let db = database.lock().await;
    let mut stmt =
        db.prepare(format!("SELECT * FROM {} WHERE creation_time < (?1)", TABLE_NAME).as_str())?;
    let rows = stmt.query([diff])?;
    let rows = rows.mapped(|row| Ok(row.get(0)));

    for row in rows {
        let row = row?;
        let uuid: String = row?;

        delete_db_entry(&uuid, &db)?;
    }
    Ok(())
}

pub fn add_migration_result_to_db(
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

/// Returns a tuple with (file_path, log) associated with a given UUID.
pub fn read_from_db(uuid: &str, database: &Connection) -> anyhow::Result<(String, String)> {
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

///removes path from database and file system
pub fn delete_db_entry(uuid: &str, database: &Connection) -> anyhow::Result<()> {
    std::fs::remove_dir_all(read_from_db(uuid, database)?.0)?;

    let mut stmt: rusqlite::Statement<'_> =
        database.prepare(format!("DELETE FROM {} WHERE uuid = (?1)", TABLE_NAME).as_str())?;
    stmt.execute([uuid])?;

    Ok(())
}

pub async fn async_db_cleanup(db_clone: Arc<Mutex<Connection>>) -> ! {
    loop {
        match rm_file_after_expiration(&db_clone).await {
            Ok(ok) => ok,
            Err(e) => eprintln!("Error when running file cleanup: {}", e),
        };
        std::thread::sleep(std::time::Duration::from_secs(15));
    }
}
