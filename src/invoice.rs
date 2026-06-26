use chrono::{Duration, Local};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::clients::get_client;
use crate::config::load_config;
use crate::store::{read_json, write_json};
use crate::worklog::WorkEntry;

const INVOICES_FILE: &str = "invoices.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LineItem {
    pub date: String,
    pub description: String,
    pub units: f64,
    pub rate: f64,
    pub amount_ex_gst: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Invoice {
    pub invoice_id: String,
    pub client_id: String,
    pub date_issued: String,
    pub due_date: String,
    pub line_items: Vec<LineItem>,
    pub subtotal: f64,
    pub gst: f64,
    pub total: f64,
    pub pdf_path: String,
}

pub fn load_invoices() -> Vec<Invoice> {
    read_json(INVOICES_FILE)
}

pub fn save_invoices(invoices: &[Invoice]) {
    write_json(INVOICES_FILE, invoices);
}

pub fn get_invoice(invoice_id: &str) -> Option<Invoice> {
    load_invoices()
        .into_iter()
        .find(|inv| inv.invoice_id == invoice_id)
}

pub fn next_invoice_number(invoices: &[Invoice]) -> String {
    let max = invoices
        .iter()
        .filter_map(|inv| {
            let parts: Vec<&str> = inv.invoice_id.splitn(2, '-').collect();
            if parts.len() == 2 {
                parts[1].parse::<u32>().ok()
            } else {
                None
            }
        })
        .max()
        .unwrap_or(0);
    format!("INV-{:04}", max + 1)
}

pub fn calculate_invoice_totals(line_items: &[LineItem]) -> (f64, f64, f64) {
    let subtotal =
        (line_items.iter().map(|i| i.units * i.rate).sum::<f64>() * 100.0).round() / 100.0;
    let gst = (subtotal * 0.1 * 100.0).round() / 100.0;
    let total = subtotal + gst;
    (subtotal, gst, total)
}

pub fn create_invoice(client_id: &str, work_entries: &[WorkEntry]) -> Invoice {
    let config = load_config();
    let client = get_client(client_id).expect("Client not found");
    let mut invoices = load_invoices();
    let invoice_id = next_invoice_number(&invoices);
    let today = Local::now().date_naive();
    let payment_terms = config.payment_terms_days.unwrap_or(14) as i64;
    let due = today + Duration::days(payment_terms);

    let rate = client.daily_rate;
    let line_items: Vec<LineItem> = work_entries
        .iter()
        .map(|e| {
            let amount = (e.units * rate * 100.0).round() / 100.0;
            LineItem {
                date: e.date.clone(),
                description: e.description.clone(),
                units: e.units,
                rate,
                amount_ex_gst: amount,
            }
        })
        .collect();

    let (subtotal, gst, total) = calculate_invoice_totals(&line_items);

    let output_dir = expand_tilde(
        config
            .invoice_output_dir
            .as_deref()
            .unwrap_or("~/invoices"),
    );
    let client_slug = client.name.replace(' ', "-");
    let pdf_path = output_dir
        .join(format!("{}-{}.pdf", invoice_id, client_slug))
        .to_string_lossy()
        .to_string();

    let invoice = Invoice {
        invoice_id,
        client_id: client_id.to_string(),
        date_issued: today.format("%Y-%m-%d").to_string(),
        due_date: due.format("%Y-%m-%d").to_string(),
        line_items,
        subtotal,
        gst,
        total,
        pdf_path,
    };

    invoices.push(invoice.clone());
    save_invoices(&invoices);
    invoice
}

pub fn resolve_pdf_path(pdf_path: &str) -> PathBuf {
    expand_tilde(pdf_path)
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        dirs::home_dir().unwrap().join(rest)
    } else if path == "~" {
        dirs::home_dir().unwrap()
    } else {
        PathBuf::from(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn li(units: f64, rate: f64) -> LineItem {
        LineItem {
            date: "2026-01-01".to_string(),
            description: "Work".to_string(),
            units,
            rate,
            amount_ex_gst: units * rate,
        }
    }

    #[test]
    fn test_totals_single_full_day() {
        let (subtotal, gst, total) = calculate_invoice_totals(&[li(1.0, 800.0)]);
        assert_eq!(subtotal, 800.0);
        assert_eq!(gst, 80.0);
        assert_eq!(total, 880.0);
    }

    #[test]
    fn test_totals_half_day() {
        let (subtotal, gst, total) = calculate_invoice_totals(&[li(0.5, 800.0)]);
        assert_eq!(subtotal, 400.0);
        assert_eq!(gst, 40.0);
        assert_eq!(total, 440.0);
    }

    #[test]
    fn test_totals_multiple_days() {
        let items = vec![li(1.0, 800.0), li(1.0, 800.0), li(0.5, 800.0), li(1.0, 800.0)];
        let (subtotal, gst, total) = calculate_invoice_totals(&items);
        assert_eq!(subtotal, 2800.0);
        assert_eq!(gst, 280.0);
        assert_eq!(total, 3080.0);
    }

    #[test]
    fn test_totals_gst_is_ten_percent() {
        let (subtotal, gst, total) = calculate_invoice_totals(&[li(1.0, 1234.56)]);
        assert_eq!(gst, (subtotal * 0.1 * 100.0).round() / 100.0);
        assert_eq!(total, subtotal + gst);
    }

    #[test]
    fn test_next_invoice_number_empty() {
        assert_eq!(next_invoice_number(&[]), "INV-0001");
    }

    #[test]
    fn test_next_invoice_number_sequential() {
        let invoices = vec![
            Invoice {
                invoice_id: "INV-0001".to_string(),
                client_id: String::new(),
                date_issued: String::new(),
                due_date: String::new(),
                line_items: vec![],
                subtotal: 0.0,
                gst: 0.0,
                total: 0.0,
                pdf_path: String::new(),
            },
            Invoice {
                invoice_id: "INV-0002".to_string(),
                client_id: String::new(),
                date_issued: String::new(),
                due_date: String::new(),
                line_items: vec![],
                subtotal: 0.0,
                gst: 0.0,
                total: 0.0,
                pdf_path: String::new(),
            },
        ];
        assert_eq!(next_invoice_number(&invoices), "INV-0003");
    }

    #[test]
    fn test_next_invoice_number_gap() {
        let make = |id: &str| Invoice {
            invoice_id: id.to_string(),
            client_id: String::new(),
            date_issued: String::new(),
            due_date: String::new(),
            line_items: vec![],
            subtotal: 0.0,
            gst: 0.0,
            total: 0.0,
            pdf_path: String::new(),
        };
        let invoices = vec![make("INV-0001"), make("INV-0005")];
        assert_eq!(next_invoice_number(&invoices), "INV-0006");
    }

    #[test]
    fn test_next_invoice_number_pads_to_four_digits() {
        let invoices: Vec<Invoice> = (1..=9)
            .map(|i| Invoice {
                invoice_id: format!("INV-{:04}", i),
                client_id: String::new(),
                date_issued: String::new(),
                due_date: String::new(),
                line_items: vec![],
                subtotal: 0.0,
                gst: 0.0,
                total: 0.0,
                pdf_path: String::new(),
            })
            .collect();
        assert_eq!(next_invoice_number(&invoices), "INV-0010");
    }
}
