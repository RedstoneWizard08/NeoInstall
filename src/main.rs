use anyhow::{Context, Result, anyhow};
use clap::{Parser, ValueEnum};
use indicatif::{ParallelProgressIterator, ProgressIterator, ProgressStyle};
use itertools::Itertools;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Read,
    path::PathBuf,
    process::Command,
};
use zip::ZipArchive;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sided<T> {
    pub client: T,
    pub server: T,
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, ValueEnum,
)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Server,
    Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Processor {
    pub sides: Option<Vec<Side>>,
    pub jar: String,
    pub classpath: Vec<String>,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryDownload {
    pub sha1: String,
    pub size: u64,
    pub url: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryDownloads {
    pub artifact: LibraryDownload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub name: String,
    pub downloads: LibraryDownloads,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NeoProfile {
    pub spec: u16,
    pub profile: String,
    pub version: String,
    pub icon: String,
    pub minecraft: String,
    pub json: String,
    pub logo: String,
    pub welcome: String,
    pub mirror_list: String,
    pub hide_extract: bool,
    pub data: HashMap<String, Sided<String>>,
    pub processors: Vec<Processor>,
    pub libraries: Vec<Library>,
    pub server_jar_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mirror {
    pub name: String,
    pub image: Option<String>,
    pub homepage: String,
    pub url: String,
    pub advertised: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestMeta {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub time: String,
    pub release_time: String,
    pub sha1: String,
    pub compliance_level: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaIndex {
    pub latest: LatestMeta,
    pub versions: Vec<Version>,
}

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

impl Side {
    pub fn get(&self) -> &'static str {
        match *self {
            Self::Client => "client",
            Self::Server => "server",
        }
    }
}

impl NeoProfile {
    pub fn data(
        &self,
        name: impl AsRef<str>,
        side: Side,
        lib_path: &PathBuf,
        base_path: &PathBuf,
    ) -> String {
        let it = match side {
            Side::Client => self
                .data
                .get(name.as_ref())
                .unwrap()
                .client
                .clone()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_string(),

            Side::Server => self
                .data
                .get(name.as_ref())
                .unwrap()
                .server
                .clone()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_string(),
        };

        if it.starts_with('/') {
            base_path
                .join(it.trim_start_matches('/'))
                .to_str()
                .unwrap()
                .into()
        } else if it.starts_with('\'') {
            it
        } else {
            lib_path.join(maven_to_path(it)).to_str().unwrap().into()
        }
    }

    pub fn add_minecraft(&mut self) {
        self.data.insert(
            "MINECRAFT_JAR".into(),
            Sided {
                client: self
                    .data
                    .get("MC_EXTRA")
                    .unwrap()
                    .client
                    .replace(":extra", ""),
                server: self
                    .data
                    .get("MC_EXTRA")
                    .unwrap()
                    .server
                    .replace(":extra", ""),
            },
        );
    }
}

pub fn maven_to_path(mvn: impl AsRef<str>) -> String {
    let mut parts = mvn.as_ref().split('@');
    let first = parts.next().unwrap().to_string();
    let ext = parts.next().map(|v| v.to_string()).unwrap_or("jar".into());
    let mut parts = first.split(':');
    let group = parts.next().unwrap().replace(".", "/");
    let artifact = parts.next().unwrap();
    let version = parts.next().unwrap();
    let classifier = parts.next().map(|v| format!("-{}", v)).unwrap_or_default();

    format!("{group}/{artifact}/{version}/{artifact}-{version}{classifier}.{ext}")
}

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

pub const MOJANG_META_URL: &str =
    "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";

pub async fn download_mc_jars(
    data: &NeoProfile,
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

    Ok(())
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

pub async fn run_processor(
    proc: &Processor,
    vars: &HashMap<String, String>,
    lib_path: &PathBuf,
    work_dir: &PathBuf,
) -> Result<()> {
    let jar = lib_path.join(maven_to_path(&proc.jar));

    if !jar.exists() {
        return Err(anyhow!("Failed to find processor JAR: {}", proc.jar));
    }

    let mut classpath = Vec::new();

    for item in &proc.classpath {
        let item_path = lib_path.join(maven_to_path(item));

        if !item_path.exists() {
            eprintln!("Failed to find classpath JAR: {}", item);
            continue;
        }

        classpath.push(
            item_path
                .strip_prefix(work_dir)
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
        );
    }

    let args = proc
        .args
        .iter()
        .map(|s| {
            let mut s = s.clone();

            for (k, v) in vars {
                s = s.replace(&format!("{{{k}}}"), v);
            }

            if s.starts_with('[') && s.ends_with(']') {
                s = lib_path
                    .join(maven_to_path(
                        s.trim_start_matches('[').trim_end_matches(']'),
                    ))
                    .strip_prefix(work_dir)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string();
            }

            s
        })
        .collect_vec();

    classpath.push(jar.to_str().unwrap().into());

    let file = File::open(&jar)?;
    let mut zip = ZipArchive::new(file)?;
    let entry = zip.by_name("META-INF/MANIFEST.MF");

    if entry.is_err() {
        return Err(anyhow!(
            "Failed to find META-INF/MANIFEST.MF in JAR: {}",
            proc.jar
        ));
    }

    let mut entry = entry?;
    let mut manifest = String::new();

    entry.read_to_string(&mut manifest)?;

    drop(entry);
    drop(zip);

    let manifest = manifest
        .split("\n")
        .map(|v| v.replace("\r", "").to_string())
        .collect_vec();

    let main = manifest
        .iter()
        .find(|v| v.starts_with("Main-Class:"))
        .map(|v| v.split(": ").last())
        .flatten()
        .map(|v| v.to_string())
        .ok_or(anyhow!("Failed to find main class in JAR: {}", proc.jar))?;

    let classpath = classpath.join(":");
    let mut cmd = Vec::new();

    cmd.push("java".into());
    cmd.push("-cp".into());
    cmd.push(classpath);
    cmd.push(main);
    cmd.extend(args);

    println!("Exec: {}", cmd.join(" "));

    let res = Command::new(cmd.remove(0))
        .args(cmd)
        .current_dir(work_dir)
        .spawn()?
        .wait()?;

    if !res.success() {
        return Err(anyhow!("Processor failed!"));
    }

    Ok(())
}

pub fn setup_vars(
    data: &NeoProfile,
    side: Side,
    lib_path: &PathBuf,
    base_path: &PathBuf,
    jar_path: &PathBuf,
) -> HashMap<String, String> {
    let mut vars = HashMap::<String, String>::new();

    vars.insert("INSTALLER".into(), jar_path.to_str().unwrap().into());
    vars.insert("ROOT".into(), ".".into());
    vars.insert("SIDE".into(), side.get().into());

    for (key, _) in &data.data {
        vars.insert(key.into(), data.data(key, side, lib_path, base_path));
    }

    vars
}

pub fn make_path_and_create(path: impl Into<PathBuf>) -> Result<PathBuf> {
    let path = path.into();

    fs::create_dir_all(&path)?;
    Ok(path)
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// The side to install for.
    #[clap(short, long, value_enum)]
    pub side: Side,

    /// The path to the original installer JAR.
    #[clap(short = 'J', long = "jar")]
    pub jar_path: PathBuf,

    /// The target path to install to.
    #[clap(short = 'd', long = "dir", default_value = ".")]
    pub target: PathBuf,
}

#[tokio::main]
pub async fn main() -> Result<()> {
    let args = Cli::parse();
    let side = args.side;
    let jar_path = args.jar_path.canonicalize().context("Getting JAR path")?;
    let work_dir = make_path_and_create(args.target)?.canonicalize()?;
    let base_path = work_dir.join(".installer");
    let lib_path = base_path.join("libraries");
    let data_path = base_path.join("data");

    let jar_file = File::open(&jar_path)?;
    let mut jar_zip = ZipArchive::new(jar_file)?;
    let mut profile_entry = jar_zip.by_name("install_profile.json")?;
    let mut profile_json = String::new();

    profile_entry.read_to_string(&mut profile_json)?;

    drop(profile_entry);

    let mut data_files = Vec::new();

    for entry in jar_zip.file_names() {
        if entry.starts_with("data/") && entry != "data/" {
            data_files.push(entry.to_string());
        }
    }

    if data_files.len() > 0 {
        fs::create_dir_all(data_path)?;

        for file in data_files.iter().progress() {
            let mut entry = jar_zip.by_name(file)?;
            let mut content = Vec::new();

            entry.read_to_end(&mut content)?;

            fs::write(base_path.join(file), content)?;
        }
    }

    let mut profile = serde_json::from_str::<NeoProfile>(&profile_json)?;

    profile.add_minecraft();
    download_libs(&profile, &lib_path).await?;
    download_mc_jars(&profile, side, &lib_path, &base_path).await?;

    let vars = setup_vars(&profile, side, &lib_path, &base_path, &jar_path);

    for proc in &profile.processors {
        if let Some(sides) = &proc.sides {
            if !sides.contains(&side) {
                eprintln!(
                    "Processor skipped due to being on the wrong side: {}",
                    proc.jar
                );

                continue;
            }
        }

        run_processor(proc, &vars, &lib_path, &work_dir).await?;
    }

    Ok(())
}
