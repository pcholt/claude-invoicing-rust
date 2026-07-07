# inv — contractor invoice CLI

A fast, single-binary CLI for Australian software contractors to log work days and generate GST-compliant PDF invoices. Written in Rust.

## Install

Download the latest `inv-linux-x86_64` binary from [Releases](https://github.com/pcholt/claude-invoicing-rust/releases/tag/latest), make it executable, and put it on your PATH:

```sh
curl -L https://github.com/pcholt/claude-invoicing-rust/releases/latest/download/inv-linux-x86_64 -o inv
chmod +x inv
mv inv ~/.local/bin/
```

Or build from source (requires Rust):

```sh
cargo build --release
# binary at target/release/inv
```

Data is stored in `~/.local/share/inv/`. PDFs default to `~/invoices/`.

---

## Quick start

```sh
inv setup                        # enter your business details once
inv client add                   # add a client
inv log                          # log today's work
inv invoice create --client acme # generate invoice PDF
```

---

## Commands

### `inv setup`

Interactive wizard to configure your business details: name, ABN, email, bank account, payment terms, invoice output directory, default working days, and backup destination.

---

### `inv client`

| Subcommand | Description |
|---|---|
| `inv client add` | Add a new client (interactive) |
| `inv client list` | List all clients |
| `inv client show <id>` | Show full details for a client |
| `inv client edit <id>` | Edit a client's details |

Client IDs are slugified from the company name (e.g. `acme-corp`).

---

### `inv log`

Log work and manage work log entries.

**Log work**

```sh
inv log                          # log today (interactive)
inv log --date 2026-07-04        # log a specific date
inv log --week                   # log Mon–Fri of the current week
inv log --week --date 2026-06-30 # log the week containing that date
```

**Subcommands**

| Subcommand | Description |
|---|---|
| `inv log list` | List work log entries (last 30 days by default) |
| `inv log cal` | Calendar view of the current month |
| `inv log edit <id>` | Edit a log entry (date, description, units) |
| `inv log delete <id>` | Delete an uninvoiced log entry |

**`inv log list` options**

| Flag | Description |
|---|---|
| `--days <N>` | Show entries from the last N days (default: 30) |
| `--from <YYYY-MM-DD>` | Start date filter (overrides `--days`) |
| `--to <YYYY-MM-DD>` | End date filter (overrides `--days`) |
| `--client <id>` | Filter by client |
| `--uninvoiced` | Show only uninvoiced entries |

**`inv log cal` options**

| Flag | Description |
|---|---|
| `--month <YYYY-MM>` | Month to display (default: current month) |

The calendar uses 256-color ANSI backgrounds:

| Color | Meaning |
|---|---|
| Green | Logged, uninvoiced (full day ≥ 1.0 units) |
| Dark green | Logged, uninvoiced, partial day |
| Red | Logged, invoiced (full day) |
| Dark red | Logged, invoiced, partial day |
| Dark grey | Unlogged weekday |
| Dim | Weekend |

Today's date is shown in bold.

---

### `inv invoice`

| Subcommand | Description |
|---|---|
| `inv invoice create --client <id>` | Create an invoice from uninvoiced work days |
| `inv invoice list` | List all invoices |
| `inv invoice show <id>` | Show line items and totals for an invoice |
| `inv invoice pdf <id>` | Regenerate the PDF for an existing invoice |

**`inv invoice create` options**

```sh
inv invoice create --client acme-corp
inv invoice create --client acme-corp --from 2026-06-01 --to 2026-06-30
```

Shows a preview table of line items with subtotal, GST (10%), and total before asking for confirmation. Generates a PDF and marks the work days as invoiced.

**`inv invoice list` options**

| Flag | Description |
|---|---|
| `--client <id>` | Filter by client |
| `--year <YYYY>` | Filter by year |

---

### `inv backup`

Copies `~/.local/share/inv/` and the invoice output directory to the configured SCP destination.

```sh
inv backup
```

Configure the destination with `inv setup` (e.g. `user@host:/backups`).

---

## PDF invoices

Invoices are generated as A4 PDFs with:

- Your business name, ABN, email, and bank details
- Client name, contact, address
- Line items: date, description, days, rate (ex GST), amount (ex GST)
- Subtotal, GST (10%), and total (inc GST)
- Invoice number, issue date, due date, and payment reference

Invoice numbers increment automatically (`INV-0001`, `INV-0002`, …).

---

## Data storage

All data is stored as JSON in `~/.local/share/inv/`:

| File | Contents |
|---|---|
| `config.json` | Business configuration |
| `clients.json` | Client list |
| `worklog.json` | Work log entries |
| `invoices.json` | Invoice records |
