use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::store::{read_json, write_json};

const CLIENTS_FILE: &str = "clients.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Client {
    pub id: String,
    pub name: String,
    pub contact_name: String,
    pub email: String,
    pub address: String,
    pub daily_rate: f64,
}

pub fn load_clients() -> Vec<Client> {
    read_json(CLIENTS_FILE)
}

pub fn save_clients(clients: &[Client]) {
    write_json(CLIENTS_FILE, clients);
}

pub fn get_client(client_id: &str) -> Option<Client> {
    load_clients().into_iter().find(|c| c.id == client_id)
}

pub fn slugify(name: &str) -> String {
    let s = name.to_lowercase();
    let re_special = Regex::new(r"[^\w\s-]").unwrap();
    let s = re_special.replace_all(&s, "");
    let re_spaces = Regex::new(r"[\s_]+").unwrap();
    let s = re_spaces.replace_all(&s, "-");
    s.trim_matches('-').to_string()
}

pub fn add_client(client: Client) {
    let mut clients = load_clients();
    clients.push(client);
    save_clients(&clients);
}

pub fn update_client(client_id: &str, updated: Client) -> bool {
    let mut clients = load_clients();
    for c in &mut clients {
        if c.id == client_id {
            *c = updated;
            save_clients(&clients);
            return true;
        }
    }
    false
}
