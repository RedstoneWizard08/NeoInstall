use crate::{maven::maven_to_path, side::Side};
use anyhow::Result;
use itertools::Itertools;
use std::{collections::HashMap, fs::File, io::Read, path::PathBuf, process::Command};
use zip::ZipArchive;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Processor {
    pub sides: Option<Vec<Side>>,
    pub jar: String,
    pub classpath: Vec<String>,
    pub args: Vec<String>,
}

pub async fn run_processor(
    proc: &Processor,
    vars: &HashMap<String, String>,
    lib_path: &PathBuf,
    work_dir: &PathBuf,
    java: &String,
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

    cmd.push(java.into());
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
