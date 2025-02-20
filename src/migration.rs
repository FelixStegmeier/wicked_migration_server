use crate::{
    db_util::add_migration_result_to_db,
    files::{File, FileType},
};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rusqlite::Connection;
use std::process::Command;
use tempfile::Builder;
use thiserror::Error;

const REGISTRY_URL: &str =
    "registry.opensuse.org/home/jcronenberg/migrate-wicked/containers/opensuse/wicked2nm:latest";

#[derive(Error, Debug)]
pub enum MigrateError {
    #[error("Server error: '{0}'")]
    ServerError(String),
    #[error("Failed to migrate files: '{0}'")]
    MigrationError(String),
}

impl From<anyhow::Error> for MigrateError {
    fn from(value: anyhow::Error) -> Self {
        Self::ServerError(value.to_string())
    }
}

impl MigrateError {
    pub fn into_response(self) -> Response {
        match self {
            MigrateError::MigrationError(e) => Response::builder()
                .status(422)
                .header("Content-Type", "text/plain")
                .body(e.into())
                .unwrap(),
            MigrateError::ServerError(e) => {
                eprintln!("{}", e);
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

//migrates the files and returns the output for logging in the result
fn migrate_files(
    files: &Vec<File>,
    migration_target_path: String,
) -> Result<std::process::Output, anyhow::Error> {
    for file in files {
        std::fs::write(
            format!("{}/{}", migration_target_path, file.file_name),
            file.file_content.as_bytes(),
        )?;
    }

    let arguments_str = if files[0].file_type == FileType::Ifcfg {
        format!(
            "run -e \"W2NM_CONTINUE_MIGRATION=true\" -e \"W2NM_WITHOUT_NETCONFIG=true\" --rm -v {}:/etc/sysconfig/network:z {}",
            migration_target_path,
            REGISTRY_URL
        )
    } else {
        format!("run --rm -v {}:/migration-tmpdir:z {} bash -c
            \"wicked2nm migrate -c --without-netconfig /migration-tmpdir/ && mkdir /migration-tmpdir/NM-migrated && cp -r /etc/NetworkManager/system-connections /migration-tmpdir/NM-migrated\"",
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

pub fn migrate(files: Vec<File>, database: &Connection) -> Result<String, MigrateError> {
    let migration_target_path = match Builder::new().keep(true).tempdir() {
        Ok(tempdir) => tempdir.path().to_string_lossy().into_owned(),
        Err(e) => return Err(MigrateError::ServerError(e.to_string())),
    };

    let output = migrate_files(&files, migration_target_path.clone())?;
    let log = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(MigrateError::MigrationError(log));
    }

    let uuid = add_migration_result_to_db(migration_target_path, log, database)?;
    Ok(uuid)
}

pub fn pull_latest_migration_image() -> anyhow::Result<()> {
    let output = Command::new("podman")
        .args(shlex::split(&format!("pull {}", REGISTRY_URL)).unwrap())
        .output()?;
    if !output.status.success() {
        anyhow::bail!("{}", String::from_utf8_lossy(&output.stderr));
    }
    Ok(())
}
