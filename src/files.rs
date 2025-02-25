use std::process::Command;
use std::{fs, str::FromStr};

#[derive(PartialEq)]
pub enum FileType {
    Xml,
    Sysconfig,
}

impl FromStr for FileType {
    type Err = anyhow::Error;
    fn from_str(file_name: &str) -> Result<Self, Self::Err> {
        if file_name.contains("ifroute") {
            return Ok(FileType::Sysconfig);
        }
        if file_name.contains("ifcfg") {
            return Ok(FileType::Sysconfig);
        }
        if file_name.contains("routes") {
            return Ok(FileType::Sysconfig);
        }
        Ok(FileType::Xml)
    }
}

pub struct File {
    pub file_content: String,
    pub file_name: String,
    pub file_type: FileType,
}

pub fn return_as_tar(path: String) -> anyhow::Result<tempfile::NamedTempFile> {
    let output_tmpfile: tempfile::NamedTempFile = tempfile::Builder::new()
        .prefix("nm-migrated.")
        .suffix(".tar")
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

pub fn file_arr_from_path(dir_path: String) -> Result<Vec<File>, anyhow::Error> {
    let mut file_arr: Vec<File> = vec![];

    let dir = fs::read_dir(dir_path.clone() + "/NM-migrated/system-connections")?;

    for dir_entry in dir {
        let path = dir_entry?.path();
        let file_type = match path.extension() {
            Some(file_type) => match file_type.to_str().unwrap() {
                "xml" => FileType::Xml,
                _ => FileType::Sysconfig,
            },
            None => {
                return Err(anyhow::anyhow!(
                    "There is no such file or file has no file extension"
                ));
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
