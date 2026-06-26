use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use chrono::{Datelike, NaiveDate};
use printpdf::*;

use crate::clients::Client;
use crate::config::Config;
use crate::invoice::Invoice;

const PAGE_W: f32 = 210.0;
const PAGE_H: f32 = 297.0;
const MARGIN_L: f32 = 20.0;
const MARGIN_R: f32 = 20.0;
const MARGIN_T: f32 = 18.0;
const CONTENT_W: f32 = PAGE_W - MARGIN_L - MARGIN_R; // 170mm

// Column x-positions (mm from left edge of page)
const COL_DATE: f32 = MARGIN_L;
const COL_DESC: f32 = MARGIN_L + 22.0;
const COL_DAYS_R: f32 = MARGIN_L + 118.0; // right edge of "Days" column
const COL_RATE_R: f32 = MARGIN_L + 148.0;
const COL_RIGHT: f32 = MARGIN_L + CONTENT_W;

fn y(from_top: f32) -> Mm {
    Mm(PAGE_H - from_top)
}

fn fmt_long(date_str: &str) -> String {
    NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map(|d| format!("{} {} {}", d.day(), month_long(d.month()), d.year()))
        .unwrap_or_else(|_| date_str.to_string())
}

fn fmt_short(date_str: &str) -> String {
    NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map(|d| format!("{} {}", d.day(), month_short(d.month())))
        .unwrap_or_else(|_| date_str.to_string())
}

fn month_long(m: u32) -> &'static str {
    match m {
        1 => "January", 2 => "February", 3 => "March", 4 => "April",
        5 => "May", 6 => "June", 7 => "July", 8 => "August",
        9 => "September", 10 => "October", 11 => "November", _ => "December",
    }
}

fn month_short(m: u32) -> &'static str {
    match m {
        1 => "Jan", 2 => "Feb", 3 => "Mar", 4 => "Apr",
        5 => "May", 6 => "Jun", 7 => "Jul", 8 => "Aug",
        9 => "Sep", 10 => "Oct", 11 => "Nov", _ => "Dec",
    }
}

pub fn fmt_currency(v: f64) -> String {
    let cents = (v.abs() * 100.0).round() as u64;
    let dollars = cents / 100;
    let cents_part = cents % 100;
    let s = dollars.to_string();
    let mut with_commas = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            with_commas.push(',');
        }
        with_commas.push(c);
    }
    format!("${}.{:02}", with_commas, cents_part)
}

fn fmt_units(v: f64) -> String {
    let s = v.to_string();
    if s.contains('.') { s } else { format!("{}.0", s) }
}

fn gray(v: f32) -> Color {
    Color::Rgb(Rgb::new(v, v, v, None))
}

#[allow(dead_code)]
struct Ctx<'a> {
    layer: &'a PdfLayerReference,
    reg: &'a IndirectFontRef,
    bold: &'a IndirectFontRef,
}

impl<'a> Ctx<'a> {
    fn text(&self, s: &str, x: f32, from_top: f32, font: &IndirectFontRef, size: f32) {
        self.layer.use_text(s, size, Mm(x), y(from_top), font);
    }

    /// Right-align text: right_x is the right boundary in mm from left
    fn text_right(&self, s: &str, right_x: f32, from_top: f32, font: &IndirectFontRef, size: f32) {
        // Helvetica average char width ≈ 0.55 × size_pt × (25.4/72) mm
        let char_w = size * 0.55 * (25.4 / 72.0);
        let text_w = s.len() as f32 * char_w;
        let x = (right_x - text_w).max(MARGIN_L);
        self.layer.use_text(s, size, Mm(x), y(from_top), font);
    }

    fn hline(&self, x1: f32, x2: f32, from_top: f32, thickness: f32) {
        self.layer.set_outline_thickness(thickness);
        self.layer.set_outline_color(gray(0.13));
        let line = Line {
            points: vec![
                (Point::new(Mm(x1), y(from_top)), false),
                (Point::new(Mm(x2), y(from_top)), false),
            ],
            is_closed: false,
        };
        self.layer.add_line(line);
    }

    fn fill_rect(&self, x1: f32, x2: f32, top: f32, bottom: f32, g: f32) {
        self.layer.set_fill_color(gray(g));
        let poly = Polygon {
            rings: vec![vec![
                (Point::new(Mm(x1), y(top)), false),
                (Point::new(Mm(x2), y(top)), false),
                (Point::new(Mm(x2), y(bottom)), false),
                (Point::new(Mm(x1), y(bottom)), false),
            ]],
            mode: PolygonMode::Fill,
            winding_order: WindingOrder::NonZero,
        };
        self.layer.add_polygon(poly);
        self.layer.set_fill_color(gray(0.0));
    }

    fn set_gray(&self, g: f32) {
        self.layer.set_fill_color(gray(g));
    }
}

pub fn generate_pdf(invoice: &Invoice, client: &Client, config: &Config, output_path: &Path) {
    let (doc, page1, layer1) =
        PdfDocument::new("Tax Invoice", Mm(PAGE_W), Mm(PAGE_H), "Invoice");
    let layer = doc.get_page(page1).get_layer(layer1);

    let reg = doc.add_builtin_font(BuiltinFont::Helvetica).unwrap();
    let bold = doc.add_builtin_font(BuiltinFont::HelveticaBold).unwrap();

    let ctx = Ctx { layer: &layer, reg: &reg, bold: &bold };

    let owner_name = config.owner_name.as_deref().unwrap_or("");
    let abn = config.abn.as_deref().unwrap_or("");
    let email = config.email.as_deref().unwrap_or("");
    let bank_name = config.bank_name.as_deref().unwrap_or("");
    let bsb = config.bsb.as_deref().unwrap_or("");
    let account_number = config.account_number.as_deref().unwrap_or("");
    let payment_terms = config.payment_terms_days.unwrap_or(14);

    // ── Header ─────────────────────────────────────────────────────────────────
    let mut top = MARGIN_T;

    // Left: owner details
    ctx.text(owner_name, MARGIN_L, top, &bold, 13.0);
    top += 6.0;
    ctx.text(&format!("ABN: {}", abn), MARGIN_L, top, &reg, 9.5);
    top += 5.0;
    ctx.text(email, MARGIN_L, top, &reg, 9.5);
    top += 5.0;
    ctx.text(
        &format!("{}, BSB: {}, Account: {}", bank_name, bsb, account_number),
        MARGIN_L, top, &reg, 9.5,
    );

    // Right: "TAX INVOICE" block (anchored to top margin)
    let hdr = MARGIN_T;
    ctx.text_right("TAX INVOICE", COL_RIGHT, hdr, &bold, 20.0);
    ctx.text_right(
        &format!("Invoice #: {}", invoice.invoice_id),
        COL_RIGHT, hdr + 9.0, &reg, 9.5,
    );
    ctx.text_right(
        &format!("Date: {}", fmt_long(&invoice.date_issued)),
        COL_RIGHT, hdr + 14.5, &reg, 9.5,
    );
    ctx.text_right(
        &format!("Due: {}", fmt_long(&invoice.due_date)),
        COL_RIGHT, hdr + 20.0, &reg, 9.5,
    );

    top += 7.0;
    ctx.hline(MARGIN_L, COL_RIGHT, top, 0.7);
    top += 8.0;

    // ── Bill To ────────────────────────────────────────────────────────────────
    ctx.set_gray(0.467);
    ctx.text("BILL TO", MARGIN_L, top, &bold, 7.5);
    ctx.set_gray(0.0);
    top += 5.0;

    ctx.text(&client.name, MARGIN_L, top, &bold, 10.5);
    top += 5.5;
    if !client.contact_name.is_empty() {
        ctx.text(&client.contact_name, MARGIN_L, top, &reg, 9.5);
        top += 4.5;
    }
    if !client.address.is_empty() {
        ctx.text(&client.address, MARGIN_L, top, &reg, 9.5);
        top += 4.5;
    }
    if !client.email.is_empty() {
        ctx.text(&client.email, MARGIN_L, top, &reg, 9.5);
        top += 4.5;
    }
    top += 5.0;

    // ── Line Items Table ────────────────────────────────────────────────────────
    let header_h = 7.5;
    ctx.fill_rect(MARGIN_L, COL_RIGHT, top, top + header_h, 0.949);

    let txt_y = top + 5.5;
    ctx.set_gray(0.2);
    ctx.text("Date", COL_DATE, txt_y, &bold, 8.5);
    ctx.text("Description", COL_DESC, txt_y, &bold, 8.5);
    ctx.text_right("Days", COL_DAYS_R, txt_y, &bold, 8.5);
    ctx.text_right("Rate (ex GST)", COL_RATE_R, txt_y, &bold, 8.5);
    ctx.text_right("Amount (ex GST)", COL_RIGHT, txt_y, &bold, 8.5);
    ctx.set_gray(0.0);

    ctx.hline(MARGIN_L, COL_RIGHT, top + header_h, 0.4);
    top += header_h;

    for (i, item) in invoice.line_items.iter().enumerate() {
        let row_h = 6.5;
        let txt_y = top + 5.0;
        let desc = truncate_str(&item.description, 44);
        ctx.text(&fmt_short(&item.date), COL_DATE, txt_y, &reg, 9.5);
        ctx.text(&desc, COL_DESC, txt_y, &reg, 9.5);
        ctx.text_right(&fmt_units(item.units), COL_DAYS_R, txt_y, &reg, 9.5);
        ctx.text_right(&fmt_currency(item.rate), COL_RATE_R, txt_y, &reg, 9.5);
        ctx.text_right(&fmt_currency(item.amount_ex_gst), COL_RIGHT, txt_y, &reg, 9.5);

        let thickness = if i + 1 == invoice.line_items.len() { 0.4 } else { 0.2 };
        ctx.hline(MARGIN_L, COL_RIGHT, top + row_h, thickness);
        top += row_h;
    }

    top += 6.0;

    // ── Totals ──────────────────────────────────────────────────────────────────
    let lbl_x = COL_RATE_R - 45.0;

    ctx.text("Subtotal (ex GST)", lbl_x, top, &reg, 9.5);
    ctx.text_right(&fmt_currency(invoice.subtotal), COL_RIGHT, top, &reg, 9.5);
    top += 5.5;

    ctx.text("GST (10%)", lbl_x, top, &reg, 9.5);
    ctx.text_right(&fmt_currency(invoice.gst), COL_RIGHT, top, &reg, 9.5);
    top += 2.0;

    ctx.hline(lbl_x, COL_RIGHT, top, 0.6);
    top += 4.0;

    ctx.text("Total (inc GST)", lbl_x, top, &bold, 10.5);
    ctx.text_right(&fmt_currency(invoice.total), COL_RIGHT, top, &bold, 10.5);
    top += 12.0;

    // ── Footer ──────────────────────────────────────────────────────────────────
    ctx.hline(MARGIN_L, COL_RIGHT, top, 0.3);
    top += 5.0;
    ctx.set_gray(0.333);
    ctx.text(
        &format!("Payment terms: NET {} days", payment_terms),
        MARGIN_L, top, &reg, 8.5,
    );
    top += 4.5;
    ctx.text(
        &format!(
            "Please include invoice number {} as your payment reference.",
            invoice.invoice_id
        ),
        MARGIN_L, top, &reg, 8.5,
    );
    ctx.set_gray(0.0);

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).expect("Cannot create invoice output directory");
    }
    let file = File::create(output_path).expect("Cannot create PDF file");
    doc.save(&mut BufWriter::new(file)).expect("Cannot write PDF");
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_string()
    } else {
        let t: String = chars[..max_chars - 1].iter().collect();
        format!("{}…", t)
    }
}
