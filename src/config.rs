use std::fs::File;
use std::io::Read;

use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub feeds: Vec<FeedConfig>,
    pub poll: Option<bool>,
    pub poll_sleep_dur: Option<u64>,
    pub webhook_urls: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct FeedConfig {
    pub kind: String,
    pub url: String,
    pub webhook_urls: Option<Vec<String>>,
}

pub async fn read_config_file(path: String) -> Result<Config> {
    let mut config_file = File::open(path)?;
    let mut config_string = String::new();

    config_file.read_to_string(&mut config_string)?;

    Ok(toml::from_str(&config_string)?)
}
