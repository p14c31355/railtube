use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub system: Option<SystemSection>,
    pub apt: Option<Section>,
    pub snap: Option<Section>,
    pub flatpak: Option<Section>,
    pub cargo: Option<Section>,
    pub deb: Option<DebSection>,
    pub scripts: Option<ScriptsSection>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SystemSection {
    #[serde(default)]
    pub update: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Section {
    #[serde(default)]
    pub list: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DebSection {
    #[serde(default)]
    pub urls: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ScriptsSection {
    #[serde(flatten)]
    pub commands: HashMap<String, String>,
}
