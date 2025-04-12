use anyhow::Result;
use std::{fs, path::PathBuf};

pub async fn download_if_needed(path: impl Into<PathBuf>, url: impl AsRef<str>) -> Result<()> {
    let path = path.into();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    if !fs::exists(&path)? {
        fs::write(path, reqwest::get(url.as_ref()).await?.bytes().await?)?;
    }

    Ok(())
}
