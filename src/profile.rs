use crate::{
    libraries::Library,
    maven::maven_to_path,
    processors::Processor,
    side::{Side, Sided},
};
use std::{collections::HashMap, path::PathBuf};

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
    vars.insert("MINECRAFT_VERSION".into(), data.minecraft.clone());
    vars.insert("LIBRARY_DIR".into(), lib_path.to_str().unwrap().into());

    for (key, _) in &data.data {
        vars.insert(key.into(), data.data(key, side, lib_path, base_path));
    }

    vars
}
