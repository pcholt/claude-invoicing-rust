mod clients;
mod config;
mod invoice;
mod pdf;
mod store;
mod worklog;

use chrono::{Datelike, Duration, Local, NaiveDate};
use clap::{Args, Parser, Subcommand};
use dialoguer::{Confirm, Input, Select};
use std::process;

use clients::{
    add_client, get_client, load_clients, slugify, update_client, Client,
};
use config::{load_config, require_config, save_config, Config};
use invoice::{
    calculate_invoice_totals, create_invoice, get_invoice, load_invoices, next_invoice_number,
    resolve_pdf_path, LineItem,
};
use pdf::generate_pdf;
use store::data_dir;
use worklog::{
    add_entry, delete_entry, filter_entries, get_entry, get_entry_by_date_client, get_last_entry,
    get_uninvoiced, mark_invoiced, update_entry,
};

// ── Formatting helpers ────────────────────────────────────────────────────────

fn fmt_short(date_str: &str) -> String {
    NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map(|d| format!("{} {}", d.day(), d.format("%b")))
        .unwrap_or_else(|_| date_str.to_string())
}

fn fmt_units(v: f64) -> String {
    let s = v.to_string();
    if s.contains('.') { s } else { format!("{}.0", s) }
}

fn fmt_currency(v: f64) -> String {
    pdf::fmt_currency(v)
}

fn green(s: &str) -> String { format!("\x1b[32m{}\x1b[0m", s) }
fn red(s: &str) -> String { format!("\x1b[31m{}\x1b[0m", s) }
fn yellow(s: &str) -> String { format!("\x1b[33m{}\x1b[0m", s) }
fn bold(s: &str) -> String { format!("\x1b[1m{}\x1b[0m", s) }
fn dim(s: &str) -> String { format!("\x1b[2m{}\x1b[0m", s) }
fn cyan(s: &str) -> String { format!("\x1b[36m{}\x1b[0m", s) }

// ── Table renderer ────────────────────────────────────────────────────────────

/// Visual length of a string, ignoring ANSI escape sequences.
fn visible_len(s: &str) -> usize {
    let mut len = 0;
    let mut in_esc = false;
    for c in s.chars() {
        if in_esc {
            if c == 'm' { in_esc = false; }
        } else if c == '\x1b' {
            in_esc = true;
        } else {
            len += 1;
        }
    }
    len
}

/// Borderless table: header on grey-240, body with 236/234 tiger stripes.
/// `right_align[i]` = true means right-align column i; defaults to false.
fn print_table(headers: &[&str], rows: &[Vec<String>], right_align: &[bool]) {
    // Column widths from header labels and visible cell widths
    let ncols = headers.len();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < ncols {
                widths[i] = widths[i].max(visible_len(cell));
            }
        }
    }

    let pad = "  ";
    let reset = "\x1b[0m";

    // Header
    print!("\x1b[48;5;240m\x1b[38;5;255m\x1b[1m");
    for (i, h) in headers.iter().enumerate() {
        let w = widths[i];
        if right_align.get(i).copied().unwrap_or(false) {
            print!("{}{:>w$}{}", pad, h, pad, w = w);
        } else {
            print!("{}{:<w$}{}", pad, h, pad, w = w);
        }
    }
    println!("{}", reset);

    // Body rows
    for (ri, row) in rows.iter().enumerate() {
        let bg = if ri % 2 == 0 { "\x1b[48;5;236m" } else { "\x1b[48;5;234m" };
        print!("{}\x1b[38;5;252m", bg);
        for (i, cell) in row.iter().enumerate() {
            let w = widths[i];
            let vlen = visible_len(cell);
            let fill = " ".repeat(w.saturating_sub(vlen));
            if right_align.get(i).copied().unwrap_or(false) {
                print!("{}{}{}{}", pad, fill, cell, pad);
            } else {
                print!("{}{}{}{}", pad, cell, fill, pad);
            }
        }
        println!("{}", reset);
    }
}

// ── CLI structure ─────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "inv", about = "inv — contractor invoice CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Configure business details
    Setup,
    /// Copy data and invoices to the configured SCP destination
    Backup,
    /// Manage clients
    Client {
        #[command(subcommand)]
        command: ClientCmd,
    },
    /// Log work days (or use subcommands: list, edit, delete)
    Log(LogArgs),
    /// Manage invoices
    Invoice {
        #[command(subcommand)]
        command: InvoiceCmd,
    },
}

#[derive(Args)]
struct LogArgs {
    /// Log a specific date (YYYY-MM-DD), defaults to today
    #[arg(long, value_name = "YYYY-MM-DD")]
    date: Option<String>,
    /// Log the full week containing the given date (or current week)
    #[arg(long)]
    week: bool,
    #[command(subcommand)]
    command: Option<LogCmd>,
}

#[derive(Subcommand)]
enum LogCmd {
    /// List work log entries
    List {
        #[arg(long)]
        client: Option<String>,
        #[arg(long)]
        uninvoiced: bool,
        #[arg(long, value_name = "YYYY-MM-DD")]
        from: Option<String>,
        #[arg(long, value_name = "YYYY-MM-DD")]
        to: Option<String>,
    },
    /// Edit a work log entry
    Edit { log_id: String },
    /// Delete a work log entry
    Delete { log_id: String },
}

#[derive(Subcommand)]
enum ClientCmd {
    /// Add a new client
    Add,
    /// List all clients
    List,
    /// Show details for a client
    Show { client_id: String },
    /// Edit a client
    Edit { client_id: String },
}

#[derive(Subcommand)]
enum InvoiceCmd {
    /// Create an invoice for a client
    Create {
        #[arg(long)]
        client: String,
        #[arg(long, value_name = "YYYY-MM-DD")]
        from: Option<String>,
        #[arg(long, value_name = "YYYY-MM-DD")]
        to: Option<String>,
    },
    /// List all invoices
    List {
        #[arg(long)]
        client: Option<String>,
        #[arg(long, value_name = "YYYY")]
        year: Option<String>,
    },
    /// Show full detail for an invoice
    Show { invoice_id: String },
    /// Regenerate PDF for an existing invoice
    Pdf { invoice_id: String },
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Setup => cmd_setup(),
        Commands::Backup => cmd_backup(),
        Commands::Client { command } => match command {
            ClientCmd::Add => cmd_client_add(),
            ClientCmd::List => cmd_client_list(),
            ClientCmd::Show { client_id } => cmd_client_show(&client_id),
            ClientCmd::Edit { client_id } => cmd_client_edit(&client_id),
        },
        Commands::Log(args) => match args.command {
            None => cmd_log(args.date.as_deref(), args.week),
            Some(LogCmd::List { client, uninvoiced, from, to }) => {
                cmd_log_list(client.as_deref(), uninvoiced, from.as_deref(), to.as_deref())
            }
            Some(LogCmd::Edit { log_id }) => cmd_log_edit(&log_id),
            Some(LogCmd::Delete { log_id }) => cmd_log_delete(&log_id),
        },
        Commands::Invoice { command } => match command {
            InvoiceCmd::Create { client, from, to } => {
                cmd_invoice_create(&client, from.as_deref(), to.as_deref())
            }
            InvoiceCmd::List { client, year } => {
                cmd_invoice_list(client.as_deref(), year.as_deref())
            }
            InvoiceCmd::Show { invoice_id } => cmd_invoice_show(&invoice_id),
            InvoiceCmd::Pdf { invoice_id } => cmd_invoice_pdf(&invoice_id),
        },
    }
}

// ── setup ─────────────────────────────────────────────────────────────────────

fn cmd_setup() {
    let existing = load_config();
    println!("\n{}\n", bold("Invoice setup"));

    let owner_name: String = Input::new()
        .with_prompt("Owner name")
        .default(existing.owner_name.unwrap_or_default())
        .interact_text()
        .unwrap();
    let abn: String = Input::new()
        .with_prompt("ABN (e.g. 12 345 678 901)")
        .default(existing.abn.unwrap_or_default())
        .interact_text()
        .unwrap();
    let email: String = Input::new()
        .with_prompt("Email")
        .default(existing.email.unwrap_or_default())
        .interact_text()
        .unwrap();
    let bank_name: String = Input::new()
        .with_prompt("Bank name")
        .default(existing.bank_name.unwrap_or_default())
        .interact_text()
        .unwrap();
    let bsb: String = Input::new()
        .with_prompt("BSB")
        .default(existing.bsb.unwrap_or_default())
        .interact_text()
        .unwrap();
    let account_number: String = Input::new()
        .with_prompt("Account number")
        .default(existing.account_number.unwrap_or_default())
        .interact_text()
        .unwrap();
    let payment_terms: String = Input::new()
        .with_prompt("Payment terms (days)")
        .default(existing.payment_terms_days.unwrap_or(14).to_string())
        .interact_text()
        .unwrap();
    let default_invoice_dir = existing
        .invoice_output_dir
        .unwrap_or_else(|| "~/invoices".to_string());
    let invoice_output_dir: String = Input::new()
        .with_prompt("Invoice output directory")
        .default(default_invoice_dir)
        .interact_text()
        .unwrap();
    let weekdays_default = existing
        .default_days
        .unwrap_or_else(|| {
            vec!["Monday","Tuesday","Wednesday","Thursday","Friday"]
                .into_iter().map(String::from).collect()
        })
        .join(",");
    let default_days_str: String = Input::new()
        .with_prompt("Default working days (comma-separated)")
        .default(weekdays_default)
        .interact_text()
        .unwrap();
    let default_days: Vec<String> = default_days_str
        .split(',')
        .map(|s| {
            let t = s.trim();
            let mut c = t.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect();

    let backup_default = existing.backup_scp_dest.unwrap_or_default();
    let backup_scp_dest: String = Input::new()
        .with_prompt("Backup SCP destination (e.g. user@host:/backups, blank to skip)")
        .default(backup_default)
        .allow_empty(true)
        .interact_text()
        .unwrap();

    let cfg = Config {
        owner_name: Some(owner_name),
        abn: Some(abn),
        email: Some(email),
        bank_name: Some(bank_name),
        bsb: Some(bsb),
        account_number: Some(account_number),
        payment_terms_days: payment_terms.trim().parse().ok(),
        invoice_output_dir: Some(invoice_output_dir),
        default_days: Some(default_days),
        backup_scp_dest: if backup_scp_dest.is_empty() {
            None
        } else {
            Some(backup_scp_dest)
        },
    };
    save_config(&cfg);
    println!("\n{}", green("Configuration saved."));
}

// ── backup ────────────────────────────────────────────────────────────────────

fn cmd_backup() {
    let config = require_config();
    let dest = config.backup_scp_dest.as_deref().unwrap_or("").trim().to_string();
    if dest.is_empty() {
        println!(
            "{}",
            yellow("No backup destination set. Run 'inv setup' to configure one.")
        );
        process::exit(1);
    }

    let data = data_dir();
    let invoice_dir = invoice::expand_tilde(
        config.invoice_output_dir.as_deref().unwrap_or("~/invoices"),
    );
    let sources = [data, invoice_dir];

    for src in &sources {
        if !src.exists() {
            println!("{}", dim(&format!("Skipping {} (does not exist)", src.display())));
            continue;
        }
        println!(
            "Copying {} → {}",
            cyan(&src.to_string_lossy()),
            cyan(&dest)
        );
        let status = std::process::Command::new("scp")
            .args(["-r", &src.to_string_lossy().to_string(), &dest])
            .status()
            .expect("Failed to execute scp");
        if !status.success() {
            eprintln!("{}", red(&format!("scp failed (exit {:?})", status.code())));
            process::exit(status.code().unwrap_or(1));
        }
    }
    println!("{}", green("Backup complete."));
}

// ── client ────────────────────────────────────────────────────────────────────

fn cmd_client_add() {
    require_config();
    println!("\n{}\n", bold("Add client"));
    let name: String = Input::new().with_prompt("Company name").interact_text().unwrap();
    let mut client_id = slugify(&name);
    let existing_ids: std::collections::HashSet<String> =
        load_clients().iter().map(|c| c.id.clone()).collect();
    let base_id = client_id.clone();
    let mut n = 2u32;
    while existing_ids.contains(&client_id) {
        client_id = format!("{}-{}", base_id, n);
        n += 1;
    }
    let contact_name: String = Input::new().with_prompt("Contact name").interact_text().unwrap();
    let email: String = Input::new().with_prompt("Email").interact_text().unwrap();
    let address: String = Input::new().with_prompt("Address").interact_text().unwrap();
    let daily_rate: f64 = Input::new()
        .with_prompt("Daily rate (ex GST)")
        .interact_text()
        .unwrap();
    add_client(Client {
        id: client_id.clone(),
        name,
        contact_name,
        email,
        address,
        daily_rate,
    });
    println!("\n{}", green(&format!("Client {} added.", bold(&client_id))));
}

fn cmd_client_list() {
    require_config();
    let clients = load_clients();
    if clients.is_empty() {
        println!("No clients. Run 'inv client add' to add one.");
        return;
    }

    let rows: Vec<Vec<String>> = clients
        .iter()
        .map(|c| vec![c.id.clone(), c.name.clone(), c.email.clone(), fmt_currency(c.daily_rate)])
        .collect();
    print_table(
        &["ID", "Name", "Email", "Daily Rate (ex GST)"],
        &rows,
        &[false, false, false, true],
    );
}

fn cmd_client_show(client_id: &str) {
    require_config();
    let c = match get_client(client_id) {
        Some(c) => c,
        None => {
            eprintln!("{}", red(&format!("Client '{}' not found.", client_id)));
            process::exit(1);
        }
    };
    println!();
    println!("  {}  {}", bold("ID:"), c.id);
    println!("  {}  {}", bold("Name:"), c.name);
    println!("  {}  {}", bold("Contact:"), c.contact_name);
    println!("  {}  {}", bold("Email:"), c.email);
    println!("  {}  {}", bold("Address:"), c.address);
    println!("  {}  {} (ex GST)", bold("Daily Rate:"), fmt_currency(c.daily_rate));
}

fn cmd_client_edit(client_id: &str) {
    require_config();
    let c = match get_client(client_id) {
        Some(c) => c,
        None => {
            eprintln!("{}", red(&format!("Client '{}' not found.", client_id)));
            process::exit(1);
        }
    };
    println!("\n{}\n", bold(&format!("Edit client: {}", client_id)));
    let name: String = Input::new()
        .with_prompt("Company name")
        .default(c.name)
        .interact_text()
        .unwrap();
    let contact_name: String = Input::new()
        .with_prompt("Contact name")
        .default(c.contact_name)
        .interact_text()
        .unwrap();
    let email: String = Input::new()
        .with_prompt("Email")
        .default(c.email)
        .interact_text()
        .unwrap();
    let address: String = Input::new()
        .with_prompt("Address")
        .default(c.address)
        .interact_text()
        .unwrap();
    let daily_rate: f64 = Input::new()
        .with_prompt("Daily rate (ex GST)")
        .default(c.daily_rate)
        .interact_text()
        .unwrap();
    update_client(
        client_id,
        Client {
            id: client_id.to_string(),
            name,
            contact_name,
            email,
            address,
            daily_rate,
        },
    );
    println!("\n{}", green(&format!("Client {} updated.", bold(client_id))));
}

// ── log ───────────────────────────────────────────────────────────────────────

fn select_client<'a>(clients: &'a [Client]) -> &'a Client {
    if clients.len() == 1 {
        return &clients[0];
    }
    let names: Vec<&str> = clients.iter().map(|c| c.name.as_str()).collect();
    let idx = Select::new()
        .with_prompt("Select a client")
        .items(&names)
        .interact()
        .unwrap();
    &clients[idx]
}

fn cmd_log(date_str: Option<&str>, week: bool) {
    let config = require_config();
    let clients = load_clients();
    if clients.is_empty() {
        println!("No clients. Run 'inv client add' first.");
        process::exit(1);
    }
    let selected = select_client(&clients).clone();
    if week {
        cmd_log_week(&config, &selected, date_str);
    } else {
        cmd_log_single(&selected, date_str);
    }
}

fn cmd_log_single(client: &Client, date_str: Option<&str>) {
    let ref_date = date_str.unwrap_or_else(|| {
        // We'll use a static string for today; evaluated lazily
        ""
    });
    let today_str = Local::now().date_naive().format("%Y-%m-%d").to_string();
    let ref_date = if ref_date.is_empty() { &today_str } else { ref_date };

    if let Some(existing) = get_entry_by_date_client(ref_date, &client.id) {
        println!(
            "{} Entry already exists for {}.",
            yellow("Warning:"),
            ref_date
        );
        if !Confirm::new().with_prompt("Overwrite?").interact().unwrap() {
            return;
        }
        delete_entry(&existing.id);
    }

    let last_desc = get_last_entry(&client.id)
        .map(|e| e.description)
        .unwrap_or_default();
    let description: String = Input::new()
        .with_prompt("Description")
        .default(last_desc)
        .interact_text()
        .unwrap();
    let units: f64 = Input::new()
        .with_prompt("Days")
        .default(1.0)
        .interact_text()
        .unwrap();
    let entry = add_entry(ref_date, &client.id, &description, units);
    println!(
        "{}",
        green(&format!("Logged {}: {} — {}", entry.id, ref_date, description))
    );
}

fn cmd_log_week(config: &Config, client: &Client, date_str: Option<&str>) {
    let ref_date = date_str
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .unwrap_or_else(|| Local::now().date_naive());

    let days_from_monday = ref_date.weekday().num_days_from_monday() as i64;
    let monday = ref_date - Duration::days(days_from_monday);
    let friday = monday + Duration::days(4);

    let default_days = config
        .default_days
        .clone()
        .unwrap_or_else(|| {
            vec!["Monday","Tuesday","Wednesday","Thursday","Friday"]
                .into_iter().map(String::from).collect()
        });

    let mon_str = format!(
        "{} {}",
        monday.day(),
        monday.format("%b")
    );
    let fri_str = format!(
        "{} {}",
        friday.day(),
        friday.format("%b")
    );
    println!("\nWeek of {} – {} {}\n", mon_str, fri_str, monday.year());

    let mut last_desc = get_last_entry(&client.id)
        .map(|e| e.description)
        .unwrap_or_default();

    let weekday_names = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday"];
    let mut collected: Vec<(String, String)> = Vec::new();

    for (i, &day_name) in weekday_names.iter().enumerate() {
        if !default_days.iter().any(|d| d == day_name) {
            continue;
        }
        let day = monday + Duration::days(i as i64);
        let label = format!("{} {} {}", day.format("%a"), day.day(), day.format("%b"));
        let day_iso = day.format("%Y-%m-%d").to_string();

        if let Some(existing) = get_entry_by_date_client(&day_iso, &client.id) {
            println!(
                "  {}  {} {}",
                dim(&label),
                yellow("Already logged:"),
                existing.description
            );
            if !Confirm::new()
                .with_prompt(format!("  Overwrite {}?", label))
                .default(false)
                .interact()
                .unwrap()
            {
                continue;
            }
            delete_entry(&existing.id);
        }

        let desc: String = Input::new()
            .with_prompt(format!("  {}  [{}]  Description", label, client.id))
            .default(last_desc.clone())
            .interact_text()
            .unwrap();

        let trimmed = desc.trim().to_string();
        if !trimmed.is_empty() {
            last_desc = trimmed.clone();
            collected.push((day_iso, trimmed));
        }
    }

    if collected.is_empty() {
        println!("\nNo work days to save.");
        return;
    }
    println!();
    let n = collected.len();
    let s = if n == 1 { "" } else { "s" };
    if Confirm::new()
        .with_prompt(format!("Save {} work day{}?", n, s))
        .interact()
        .unwrap()
    {
        for (date, desc) in &collected {
            add_entry(date, &client.id, desc, 1.0);
        }
        println!("{}", green(&format!("Saved {} work day{}.", n, s)));
    }
}

fn cmd_log_list(
    client_id: Option<&str>,
    uninvoiced: bool,
    from_date: Option<&str>,
    to_date: Option<&str>,
) {
    require_config();
    let entries = filter_entries(client_id, uninvoiced, from_date, to_date);
    if entries.is_empty() {
        println!("No entries found.");
        return;
    }
    let rows: Vec<Vec<String>> = entries
        .iter()
        .map(|e| {
            let status = match &e.invoice_id {
                Some(id) => dim(id),
                None => green("Uninvoiced"),
            };
            vec![
                e.id.clone(), e.date.clone(), e.client_id.clone(),
                e.description.clone(), fmt_units(e.units), status,
            ]
        })
        .collect();
    print_table(
        &["ID", "Date", "Client", "Description", "Days", "Status"],
        &rows,
        &[false, false, false, false, true, false],
    );
}

fn cmd_log_edit(log_id: &str) {
    require_config();
    let entry = match get_entry(log_id) {
        Some(e) => e,
        None => {
            eprintln!("{}", red(&format!("Entry '{}' not found.", log_id)));
            process::exit(1);
        }
    };
    if entry.invoice_id.is_some() {
        eprintln!(
            "{} entry is part of invoice {}.",
            red("Cannot edit:"),
            entry.invoice_id.unwrap()
        );
        process::exit(1);
    }
    let description: String = Input::new()
        .with_prompt("Description")
        .default(entry.description)
        .interact_text()
        .unwrap();
    let units: f64 = Input::new()
        .with_prompt("Days")
        .default(entry.units)
        .interact_text()
        .unwrap();
    let date: String = Input::new()
        .with_prompt("Date (YYYY-MM-DD)")
        .default(entry.date)
        .interact_text()
        .unwrap();
    update_entry(log_id, &date, &description, units);
    println!("{}", green(&format!("Entry {} updated.", log_id)));
}

fn cmd_log_delete(log_id: &str) {
    require_config();
    let entry = match get_entry(log_id) {
        Some(e) => e,
        None => {
            eprintln!("{}", red(&format!("Entry '{}' not found.", log_id)));
            process::exit(1);
        }
    };
    if entry.invoice_id.is_some() {
        eprintln!(
            "{} entry is part of invoice {}.",
            red("Cannot delete:"),
            entry.invoice_id.unwrap()
        );
        process::exit(1);
    }
    println!(
        "  {} — {} (days: {})",
        entry.date,
        entry.description,
        fmt_units(entry.units)
    );
    if Confirm::new()
        .with_prompt(format!("Delete entry {}?", log_id))
        .interact()
        .unwrap()
    {
        delete_entry(log_id);
        println!("{}", green(&format!("Entry {} deleted.", log_id)));
    }
}

// ── invoice ───────────────────────────────────────────────────────────────────

fn cmd_invoice_create(client_id: &str, from_date: Option<&str>, to_date: Option<&str>) {
    let config = require_config();
    let c = match get_client(client_id) {
        Some(c) => c,
        None => {
            eprintln!("{}", red(&format!("Client '{}' not found.", client_id)));
            process::exit(1);
        }
    };

    let entries = get_uninvoiced(client_id, from_date, to_date);
    if entries.is_empty() {
        println!("No uninvoiced work days found for client '{}'.", client_id);
        process::exit(0);
    }

    let rate = c.daily_rate;
    let line_items: Vec<LineItem> = entries
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
    let next_id = next_invoice_number(&load_invoices());

    let rows: Vec<Vec<String>> = line_items
        .iter()
        .map(|item| vec![
            fmt_short(&item.date), item.description.clone(),
            fmt_units(item.units), fmt_currency(item.rate),
            fmt_currency(item.amount_ex_gst),
        ])
        .collect();
    print_table(
        &["Date", "Description", "Days", "Rate", "Amount"],
        &rows,
        &[false, false, true, true, true],
    );

    let col = 50usize;
    println!("{:>col$}   {:>12}", "Subtotal", fmt_currency(subtotal), col = col);
    println!("{:>col$}   {:>12}", "GST (10%)", fmt_currency(gst), col = col);
    println!("{}", bold(&format!("{:>col$}   {:>12}", "Total", fmt_currency(total), col = col)));
    println!();

    if !Confirm::new()
        .with_prompt(format!("Generate invoice {} for {}?", next_id, c.name))
        .interact()
        .unwrap()
    {
        return;
    }

    let inv = create_invoice(client_id, &entries);
    let log_ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
    mark_invoiced(&log_ids, &inv.invoice_id);

    let full_path = resolve_pdf_path(&inv.pdf_path);
    println!("Generating PDF…");
    generate_pdf(&inv, &c, &config, &full_path);
    println!(
        "{}",
        green(&format!("Invoice {} saved to {}", inv.invoice_id, inv.pdf_path))
    );
}

fn cmd_invoice_list(client_id: Option<&str>, year: Option<&str>) {
    require_config();
    let mut invoices = load_invoices();
    if let Some(cid) = client_id {
        invoices.retain(|i| i.client_id == cid);
    }
    if let Some(y) = year {
        invoices.retain(|i| i.date_issued.starts_with(y));
    }
    if invoices.is_empty() {
        println!("No invoices found.");
        return;
    }
    let rows: Vec<Vec<String>> = invoices
        .iter()
        .map(|inv| vec![
            inv.invoice_id.clone(), inv.client_id.clone(),
            inv.date_issued.clone(), inv.due_date.clone(),
            fmt_currency(inv.total), dim(&inv.pdf_path),
        ])
        .collect();
    print_table(
        &["Invoice ID", "Client", "Date Issued", "Due Date", "Total", "PDF"],
        &rows,
        &[false, false, false, false, true, false],
    );
}

fn cmd_invoice_show(invoice_id: &str) {
    require_config();
    let inv = match get_invoice(invoice_id) {
        Some(i) => i,
        None => {
            eprintln!("{}", red(&format!("Invoice '{}' not found.", invoice_id)));
            process::exit(1);
        }
    };

    println!("\n{}  {}", bold(&inv.invoice_id), inv.client_id);
    println!("Issued: {}  |  Due: {}\n", inv.date_issued, inv.due_date);

    let rows: Vec<Vec<String>> = inv
        .line_items
        .iter()
        .map(|item| vec![
            fmt_short(&item.date), item.description.clone(),
            fmt_units(item.units), fmt_currency(item.rate),
            fmt_currency(item.amount_ex_gst),
        ])
        .collect();
    print_table(
        &["Date", "Description", "Days", "Rate", "Amount (ex GST)"],
        &rows,
        &[false, false, true, true, true],
    );

    let col = 50usize;
    println!("{:>col$}   {:>12}", "Subtotal", fmt_currency(inv.subtotal), col = col);
    println!("{:>col$}   {:>12}", "GST (10%)", fmt_currency(inv.gst), col = col);
    println!("{}", bold(&format!("{:>col$}   {:>12}", "Total", fmt_currency(inv.total), col = col)));
    println!("\n{}", dim(&format!("PDF: {}", inv.pdf_path)));
}

fn cmd_invoice_pdf(invoice_id: &str) {
    let config = require_config();
    let inv = match get_invoice(invoice_id) {
        Some(i) => i,
        None => {
            eprintln!("{}", red(&format!("Invoice '{}' not found.", invoice_id)));
            process::exit(1);
        }
    };
    let c = match get_client(&inv.client_id) {
        Some(c) => c,
        None => {
            eprintln!("{}", red(&format!("Client '{}' not found.", inv.client_id)));
            process::exit(1);
        }
    };
    let full_path = resolve_pdf_path(&inv.pdf_path);
    generate_pdf(&inv, &c, &config, &full_path);
    println!("{}", green(&format!("PDF regenerated: {}", inv.pdf_path)));
}
