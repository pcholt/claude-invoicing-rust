use serde::{Deserialize, Serialize};

use crate::store::{read_json, write_json};

const WORKLOG_FILE: &str = "worklog.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkEntry {
    pub id: String,
    pub date: String,
    pub client_id: String,
    pub description: String,
    pub units: f64,
    pub invoice_id: Option<String>,
}

pub fn load_worklog() -> Vec<WorkEntry> {
    read_json(WORKLOG_FILE)
}

pub fn save_worklog(entries: &[WorkEntry]) {
    write_json(WORKLOG_FILE, entries);
}

fn next_log_id(entries: &[WorkEntry]) -> String {
    let max = entries
        .iter()
        .filter(|e| e.id.starts_with("wl-"))
        .filter_map(|e| e.id.split('-').nth(1)?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("wl-{:03}", max + 1)
}

pub fn get_entry(log_id: &str) -> Option<WorkEntry> {
    load_worklog().into_iter().find(|e| e.id == log_id)
}

pub fn get_entry_by_date_client(date_str: &str, client_id: &str) -> Option<WorkEntry> {
    load_worklog()
        .into_iter()
        .find(|e| e.date == date_str && e.client_id == client_id)
}

pub fn add_entry(date_str: &str, client_id: &str, description: &str, units: f64) -> WorkEntry {
    let mut entries = load_worklog();
    let entry = WorkEntry {
        id: next_log_id(&entries),
        date: date_str.to_string(),
        client_id: client_id.to_string(),
        description: description.to_string(),
        units,
        invoice_id: None,
    };
    entries.push(entry.clone());
    save_worklog(&entries);
    entry
}

pub fn update_entry(log_id: &str, date: &str, description: &str, units: f64) {
    let mut entries = load_worklog();
    for e in &mut entries {
        if e.id == log_id {
            e.date = date.to_string();
            e.description = description.to_string();
            e.units = units;
        }
    }
    save_worklog(&entries);
}

pub fn delete_entry(log_id: &str) {
    let entries: Vec<WorkEntry> = load_worklog()
        .into_iter()
        .filter(|e| e.id != log_id)
        .collect();
    save_worklog(&entries);
}

pub fn get_last_entry(client_id: &str) -> Option<WorkEntry> {
    load_worklog()
        .into_iter()
        .filter(|e| e.client_id == client_id)
        .last()
}

pub fn get_uninvoiced(
    client_id: &str,
    from_date: Option<&str>,
    to_date: Option<&str>,
) -> Vec<WorkEntry> {
    let mut result: Vec<WorkEntry> = load_worklog()
        .into_iter()
        .filter(|e| {
            e.client_id == client_id
                && e.invoice_id.is_none()
                && from_date.map_or(true, |d| e.date.as_str() >= d)
                && to_date.map_or(true, |d| e.date.as_str() <= d)
        })
        .collect();
    result.sort_by(|a, b| a.date.cmp(&b.date));
    result
}

pub fn mark_invoiced(log_ids: &[String], invoice_id: &str) {
    let mut entries = load_worklog();
    for e in &mut entries {
        if log_ids.contains(&e.id) {
            e.invoice_id = Some(invoice_id.to_string());
        }
    }
    save_worklog(&entries);
}

pub fn filter_entries(
    client_id: Option<&str>,
    uninvoiced_only: bool,
    from_date: Option<&str>,
    to_date: Option<&str>,
) -> Vec<WorkEntry> {
    let mut result: Vec<WorkEntry> = load_worklog()
        .into_iter()
        .filter(|e| {
            if let Some(cid) = client_id {
                if e.client_id != cid {
                    return false;
                }
            }
            if uninvoiced_only && e.invoice_id.is_some() {
                return false;
            }
            if let Some(d) = from_date {
                if e.date.as_str() < d {
                    return false;
                }
            }
            if let Some(d) = to_date {
                if e.date.as_str() > d {
                    return false;
                }
            }
            true
        })
        .collect();
    result.sort_by(|a, b| a.date.cmp(&b.date));
    result
}
