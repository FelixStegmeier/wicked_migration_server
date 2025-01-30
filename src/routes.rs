use crate::db_util::{delete_db_entry, generate_json, get_tar};
use crate::migration::migrate;
use crate::AppState;
use axum::extract::{Multipart, OriginalUri, Path, State};
use axum::response::{Redirect, Response};
use axum::{http::StatusCode, response::IntoResponse};
use rusqlite::Connection;
use std::str::{self, FromStr};

use crate::files::{File, FileType};

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

pub async fn return_config_json(
    Path(uuid): Path<String>,
    State(shared_state): State<AppState>,
) -> Response {
    let database = shared_state.database.lock().await;

    let json_string = match generate_json(&uuid, &database) {
        Ok(file_arr) => file_arr,
        Err(e) => {
            eprintln!("Could not retrieve files for {uuid}: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if let Err(e) = delete_db_entry(&uuid, &database) {
        eprintln!("Error when removing database entry {}: {}", uuid, e);
    }

    drop(database);

    axum::response::Json(json_string).into_response()
}

pub async fn return_config_file(
    Path(uuid): Path<String>,
    State(shared_state): State<AppState>,
) -> Response {
    let database = shared_state.database.lock().await;

    let file_contents = match get_tar(&uuid, &database) {
        Ok(file_contents) => file_contents,
        Err(e) => {
            eprintln!("Error when attempting to retrieve tar for {uuid}: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if let Err(e) = delete_db_entry(&uuid, &database) {
        eprintln!("Error when removing database entry {}: {}", uuid, e);
    }

    drop(database);

    file_contents.into_response()
}

pub async fn redirect_post_multipart_form(
    uri: OriginalUri,
    State(shared_state): State<AppState>,
    multipart: Multipart,
) -> Response {
    let database: tokio::sync::MutexGuard<'_, Connection> = shared_state.database.lock().await;

    let data_array = match read_multipart_post_data_to_file_arr(multipart).await {
        Ok(ok) => ok,
        Err(e) => {
            eprintln!("An error occurred when trying to read incoming data: {e}");
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

    let uuid = match migrate(data_array, &database) {
        Ok(uuid) => uuid,
        Err(e) => return e.into_response(),
    };

    if uri.to_string() == "/json" {
        Redirect::to(&format!("/json/{}", uuid)).into_response()
    } else {
        Redirect::to(&format!("/tar/{}", uuid)).into_response()
    }
}

pub async fn redirect(State(shared_state): State<AppState>, data_string: String) -> Response {
    let database: tokio::sync::MutexGuard<'_, Connection> = shared_state.database.lock().await;
    let data_arr: Vec<File> = vec![File {
        file_content: data_string,
        file_name: "wicked.xml".to_string(),
        file_type: FileType::Xml,
    }];

    let uuid = match migrate(data_arr, &database) {
        Ok(uuid) => uuid,
        Err(e) => return e.into_response(),
    };

    Redirect::to(&format!("/tar/{}", uuid)).into_response()
}
