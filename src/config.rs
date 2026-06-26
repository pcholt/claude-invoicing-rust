use serde::{Deserialize, Serialize};
use std::process;

use crate::store::{read_json, write_json};

const CONFIG_FILE: &str = "config.json";

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct Config {
    pub owner_name: Option<String>,
    pub abn: Option<String>,
    pub email: Option<String>,
    pub bank_name: Option<String>,
    pub bsb: Option<String>,
    pub account_number: Option<String>,
    pub payment_terms_days: Option<u32>,
    pub invoice_output_dir: Option<String>,
    pub default_days: Option<Vec<String>>,
    pub backup_scp_dest: Option<String>,
}

pub fn load_config() -> Config {
    read_json(CONFIG_FILE)
}

pub fn save_config(config: &Config) {
    write_json(CONFIG_FILE, config);
}

pub fn require_config() -> Config {
    let config = load_config();
    if config.owner_name.as_deref().unwrap_or("").is_empty() {
        eprintln!("Error: Not configured. Run 'inv setup' first.");
        process::exit(1);
    }
    config
}
