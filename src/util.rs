use anyhow::Result;
use std::{fs, path::PathBuf};

pub fn make_path_and_create(path: impl Into<PathBuf>) -> Result<PathBuf> {
    let path = path.into();

    fs::create_dir_all(&path)?;
    Ok(path)
}
