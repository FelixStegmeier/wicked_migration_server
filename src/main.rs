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

    //let file_path = tmp_dir.path().to_str().unwrap();

    let input_tmpfile: tempfile::NamedTempFile = tempfile::Builder::new()
        .suffix(".xml")
        .tempfile_in(tmp_dir.path())?;

    std::fs::write(&input_tmpfile, data_string.as_bytes())?;

    let output_tmpfile: tempfile::NamedTempFile = tempfile::Builder::new()
        .prefix("nm-migrated.")
        .suffix(".tar")
        .keep(true)
        .tempfile()?;
    let output_path_str:&str = output_tmpfile.path().to_str().unwrap();

    let input_path_filename:&str = input_tmpfile
        .path()
        .file_name()
        .ok_or(anyhow::anyhow!("Invalid filename"))?
        .to_str()
        .ok_or(anyhow::anyhow!("Invalid filename"))?;


    //podman run --rm -v /tmp/$tempdir:/migration-tmpdir registry.opensuse.org/home/jcronenberg/migrate-wicked/containers/opensuse/migrate-wicked-git:latest \
    //   bash -c "migrate-wicked migrate /migration-tmpdir/tmp.xml && cp -r /etc/NetworkManager/system-connections /migration-tmpdir/NM-migrated"
    Command::new("podman")
    .args([
        "run",
        "-rm",
        "-v",
        &(tmp_dir.path().to_str().unwrap().to_string() + ":/migration-tmpdir registry.opensuse.org/home/jcronenberg/migrate-wicked/containers/opensuse/migrate-wicked-git:latest"),
        "bash",
        "-c",
        &("migrate-wicked migrate /migration-tmpdir".to_string() + input_path_filename + "&& cp -r /etc/NetworkManager/system-connections /migration-tmpdir/NM-migrated"),
    ])
    .output()?;

    //tar sollte passen?
    Command::new("tar")
        .args([
            "cf",
            output_path_str,
            "-C",
            tmp_dir.path().to_str().unwrap(),
            input_path_filename,
        ])
        .output()?;

    
    


    // Command::new("tar")
    //     .args([
    //         "cf",
    //         target_path_str,
    //         "-C",
    //         tmp_dir.path().to_str().unwrap(),
    //         path_filename,
    //     ])
    //     .output()?;
    println!("{}", output_path_str);
    Ok(output_tmpfile.path().file_name().unwrap().to_str().unwrap().to_string())
}