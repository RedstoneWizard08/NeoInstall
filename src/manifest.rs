use crate::{
    download::download_if_needed,
    meta::{MOJANG_META_URL, MetaIndex},
    profile::NeoProfile,
    side::Side,
};
use anyhow::Result;
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDownload {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDownloads {
    pub client: FileDownload,
    pub client_mappings: FileDownload,
    pub server: FileDownload,
    pub server_mappings: FileDownload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionManifest {
    // There are a lot of other fields here but I don't really care about them.
    pub downloads: VersionDownloads,
}

impl VersionManifest {
    pub fn jar(&self, side: Side) -> String {
        match side {
            Side::Client => self.downloads.client.url.clone(),
            Side::Server => self.downloads.server.url.clone(),
        }
    }

    pub fn mappings(&self, side: Side) -> String {
        match side {
            Side::Client => self.downloads.client_mappings.url.clone(),
            Side::Server => self.downloads.server_mappings.url.clone(),
        }
    }
}

pub async fn download_mc_jars(
    data: &NeoProfile,
    vars: &HashMap<String, String>,
    side: Side,
    lib_path: &PathBuf,
    base_path: &PathBuf,
) -> Result<()> {
    let meta = reqwest::get(MOJANG_META_URL)
        .await?
        .json::<MetaIndex>()
        .await?;

    let version_info = reqwest::get(
        meta.versions
            .into_iter()
            .find(|v| v.id == data.minecraft)
            .ok_or(anyhow!("Failed to find Minecraft version info!"))?
            .url,
    )
    .await?
    .json::<VersionManifest>()
    .await?;

    download_if_needed(
        data.data("MINECRAFT_JAR", side, lib_path, base_path),
        version_info.jar(side),
    )
    .await?;

    download_if_needed(
        data.data("MOJMAPS", side, lib_path, base_path),
        version_info.mappings(side),
    )
    .await?;

    if side == Side::Server {
        let mut server_path = data.server_jar_path.clone();

        for (k, v) in vars {
            server_path = server_path.replace(&format!("{{{k}}}"), v);
        }

        download_if_needed(server_path, version_info.jar(Side::Server)).await?;
    }

    Ok(())
}
