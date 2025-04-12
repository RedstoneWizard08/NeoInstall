#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mirror {
    pub name: String,
    pub image: Option<String>,
    pub homepage: String,
    pub url: String,
    pub advertised: bool,
}
