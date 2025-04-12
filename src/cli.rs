use crate::{
    download::download_if_needed,
    libraries::download_libs,
    manifest::download_mc_jars,
    maven::maven_to_path,
    processors::run_processor,
    profile::{NeoProfile, setup_vars},
    side::Side,
    util::make_path_and_create,
};
use anyhow::Result;
use clap::{Parser, Subcommand};
use indicatif::ProgressIterator;
use libsui::find_section;
use std::{
    env::current_exe,
    fs::{self, File},
    io::Read,
    path::PathBuf,
};
use zip::ZipArchive;

pub const NEO_MAVEN: &str = "https://maven.neoforged.net/releases";
pub const EMBEDDED_VERSION_SECTION: &str = "__neo_version";

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct AutoCli {
    /// The side to install for.
    #[clap(short = 's', long = "side", value_enum)]
    side: Side,

    /// The target path to install to.
    #[clap(short = 'd', long = "dir", default_value = ".")]
    target: PathBuf,

    /// Keep files created during installation.
    #[clap(short = 'k', long = "keep")]
    keep: bool,

    /// The path to the Java executable to use when running processors.
    #[clap(short = 'j', long = "java", default_value = "java")]
    java: String,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Generate an automated installer for a specific version.
    Generate {
        /// The version of NeoForge to install.
        #[clap(short = 'n', long = "neo")]
        neo: String,

        /// The path to output the generated executable.
        #[clap(short = 'o', long = "output")]
        output: PathBuf,
    },

    /// Install NeoForge.
    Install {
        /// The side to install for.
        #[clap(short = 's', long = "side", value_enum)]
        side: Side,

        /// The version of NeoForge to install.
        #[clap(short = 'n', long = "neo")]
        neo: String,

        /// The target path to install to.
        #[clap(short = 'd', long = "dir", default_value = ".")]
        target: PathBuf,

        /// Keep files created during installation.
        #[clap(short = 'k', long = "keep")]
        keep: bool,

        /// The path to the Java executable to use when running processors.
        #[clap(short = 'j', long = "java", default_value = "java")]
        java: String,
    },
}

impl AutoCli {
    pub async fn exec() -> Result<()> {
        Self::parse().run().await
    }

    pub async fn run(self) -> Result<()> {
        let Some(neo) = libsui::find_section(EMBEDDED_VERSION_SECTION) else {
            return Err(anyhow!("Could not find embedded NeoForge version data!"));
        };

        let neo = String::from_utf8(neo.to_vec())?;

        Cli {
            command: Commands::Install {
                side: self.side,
                neo,
                target: self.target,
                keep: self.keep,
                java: self.java,
            },
        }
        .run()
        .await
    }
}

impl Cli {
    pub async fn exec() -> Result<()> {
        Self::parse().run().await
    }

    pub async fn run(self) -> Result<()> {
        match self.command {
            Commands::Generate { neo, output } => {
                let current = current_exe()?;
                let current = fs::read(current)?;
                let mut output = File::create(output)?;

                #[cfg(target_os = "linux")]
                {
                    libsui::Elf::new(&current).append(
                        EMBEDDED_VERSION_SECTION,
                        neo.as_bytes(),
                        &mut output,
                    )?;
                }

                #[cfg(target_os = "macos")]
                {
                    libsui::Macho::from(current)?
                        .write_section(EMBEDDED_VERSION_SECTION, neo.as_bytes().to_vec())?
                        .build_and_sign(&mut output)?;
                }

                #[cfg(target_os = "windows")]
                {
                    libsui::PortableExecutable::from(&current)?
                        .write_resource(EMBEDDED_VERSION_SECTION, neo.as_bytes().to_vec())?
                        .build(&mut output)?;
                }
            }

            Commands::Install {
                side,
                neo,
                target,
                keep,
                java,
            } => {
                let work_dir = make_path_and_create(target)?.canonicalize()?;
                let base_path = work_dir.join(".installer");
                let lib_path = work_dir.join("libraries");
                let data_path = base_path.join("data");
                let jar_path = base_path.join("installer.jar");
                let jar_artifact = format!("net.neoforged:neoforge:{}:installer", neo);
                let jar_url = format!("{NEO_MAVEN}/{}", maven_to_path(jar_artifact));

                download_if_needed(&jar_path, jar_url).await?;

                let jar_path = jar_path.canonicalize()?;
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

                let vars = setup_vars(&profile, side, &lib_path, &base_path, &jar_path);

                download_libs(&profile, &lib_path).await?;
                download_mc_jars(&profile, &vars, side, &lib_path, &base_path).await?;

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

                    run_processor(proc, &vars, &lib_path, &work_dir, &java).await?;
                }

                if !keep {
                    fs::remove_dir_all(base_path)?;
                }
            }
        }

        Ok(())
    }
}

pub fn is_auto() -> bool {
    find_section(EMBEDDED_VERSION_SECTION).is_some()
}

pub async fn run() -> Result<()> {
    match is_auto() {
        true => AutoCli::exec().await,
        false => Cli::exec().await,
    }
}
