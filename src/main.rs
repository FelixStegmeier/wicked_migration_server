#[macro_use]
extern crate rocket;
use rocket::data::{Data, ToByteUnit};
use rocket::http::Status;
use rocket::response::status;
use rocket::Build;
use rocket::Rocket;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{self, tempdir};

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
async fn receive_data(data: Data<'_>) -> status::Custom<String> //RawHtml<&'static str>
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
    status::Custom(Status::Created, path)
}

#[launch]
fn rocket() -> Rocket<Build> {
    rocket::build().mount("/", routes![receive_data, return_config_file])
}

fn do_migration(data_string: String) -> Result<String, anyhow::Error> {
    let tmp_dir: tempfile::TempDir = tempdir()?;

    let file_path = tmp_dir.path().to_str().unwrap();

    let path = tempfile::Builder::new()
        .suffix(".xml")
        .tempfile_in(tmp_dir.path())?;

    std::fs::write(&path, data_string.as_bytes())?;

    let target_file = tempfile::Builder::new()
        .prefix("nm-migrated.")
        .suffix(".tar")
        .keep(true)
        .tempfile()?;
    let target_path_str = target_file.path().to_str().unwrap();

    let path_filename = path
        .path()
        .file_name()
        .ok_or(anyhow::anyhow!("Invalid filename"))?
        .to_str()
        .ok_or(anyhow::anyhow!("Invalid filename"))?;

    Command::new("tar")
        .args([
            "cf",
            target_path_str,
            "-C",
            tmp_dir.path().to_str().unwrap(),
            path_filename,
        ])
        .output()?;
    println!("{}", target_path_str);
    Ok(target_file.path().file_name().unwrap().to_str().unwrap().to_string())
}