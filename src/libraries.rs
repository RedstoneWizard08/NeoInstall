use crate::{maven::maven_to_path, mirrors::Mirror, profile::NeoProfile};
use anyhow::Result;
use indicatif::{ParallelProgressIterator, ProgressStyle};
use itertools::Itertools;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use reqwest::StatusCode;
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub name: String,
    pub downloads: LibraryDownloads,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryDownloads {
    pub artifact: LibraryDownload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryDownload {
    pub sha1: String,
    pub size: u64,
    pub url: String,
    pub path: String,
}

pub async fn download_libs(data: &NeoProfile, lib_path: &PathBuf) -> Result<()> {
    let mirrors = reqwest::get(&data.mirror_list)
        .await?
        .json::<Vec<Mirror>>()
        .await?;

    let mirror = mirrors.first().unwrap();
    let base_url = mirror.url.clone();
    let mc_libs_url = "https://libraries.minecraft.net";

    data.libraries
        .iter()
        .map(|v| v.name.clone())
        .collect_vec()
        .par_iter()
        .progress_with_style(
            ProgressStyle::default_bar()
                .progress_chars("=> ")
                .template("{msg} [{wide_bar:.cyan/blue}] {percent}% {pos:>7}/{len:7}")?,
        )
        .map(|lib| -> Result<()> {
            let base_path = maven_to_path(lib);
            let file_path = lib_path.join(&base_path);

            if fs::exists(&file_path)? {
                return Ok(());
            }

            let dir = file_path.parent().unwrap();
            let url = format!("{base_url}/{base_path}");
            let req = reqwest::blocking::get(&url)?;

            if req.status() == StatusCode::OK {
                fs::create_dir_all(dir)?;
                fs::write(file_path, req.bytes()?)?;
            } else {
                let url = format!("{mc_libs_url}/{base_path}");
                let req = reqwest::blocking::get(&url)?;

                if req.status() == StatusCode::OK {
                    fs::create_dir_all(dir)?;
                    fs::write(file_path, req.bytes()?)?;
                } else {
                    eprintln!("An error occured downloading from: {url}");
                }
            }

            Ok(())
        })
        .flatten()
        .collect::<Vec<_>>();

    Ok(())
}
