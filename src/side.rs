use clap::ValueEnum;

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
pub struct Sided<T> {
    pub client: T,
    pub server: T,
}

impl Side {
    pub fn get(&self) -> &'static str {
        match *self {
            Self::Client => "client",
            Self::Server => "server",
        }
    }
}
