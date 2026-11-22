# flock-audit-report

**Open the page: https://signalforhumanity.github.io/flock-audit-report/**

A web page that summarizes the Flock Safety license-plate-reader search
audits you get through a public-records request. It shows who searched the
cameras, from which state, how many were federal agencies, what reasons were
given, how many reasons were blank or vague, how many mention immigration,
protests or abortion, and how far each search reached. It works on files of
millions of rows that Excel and Google Sheets cannot open, and it combines
several files without counting the same search twice.

Your files stay on your device. The page reads them in your browser and
never uploads them. It makes no network requests after it loads and works
offline.

## Who it is for

Residents, local DeFlock groups, journalists and council staff who
received Flock audit CSVs and do not want to write Python to read them.
Guides such as DeFlockILM's
["How to Read a Flock Search Trail"](https://deflockilm.org/read-flock-audit-logs/)
suggest a pivot table for small files and pandas for big ones. Network
Audits often have 1–3 million rows a month. This page replaces the pandas
step. See [PROBLEM.md](PROBLEM.md) for the research behind it.

## How to use it

1. Get the audit files: the **Organization Audit** and **Network Audit**
   (CSV exports from the Insights tab of Flock), or a transparency portal's
   **Public Search Audit**. Have I Been Flocked's
   [audit log guide](https://haveibeenflocked.com/about/audit-logs) has
   request language.
2. Open the page, pick one or more CSV files (pick all monthly files at once
   to combine them), optionally type the cameras' home state, and press
   **Make report**.
3. Copy the report as text, save it, print it, or save the full list of
   searching agencies as CSV.

If a file came as `.xlsx`, open it and save it as CSV first. An `.xlsx`
sheet holds at most 1,048,576 rows anyway.

## What the report contains

- **Files read**: rows and date range per file. There is a warning when a file
  has exactly 1,048,575 rows. That is Excel's limit, which usually means
  someone opened and saved it in Excel and the rest was cut off.
- **Searches**: rows read, duplicate search IDs removed, and unique
  searches. One Flock search fans out across the network, so the same ID
  appears in every audit it touched. Everything else counts unique
  searches.
- **Searches per month.**
- **Who searched**: in-state, out-of-state, federal and unrecognized
  searches, searches per state, and the 25 busiest agencies. The state is
  read from the agency name ("Houston TX PD", "Texas Department of Public
  Safety"). Federal means a `[Federal]` prefix or names such as FBI, DEA,
  ATF, HSI, CBP, US Marshals or Postal Inspection.
- **Stated reasons**: blank or redacted reasons, vague reasons
  ("investigation", "other", "inv", 3 characters or fewer), whether case
  numbers were given, and reason categories (the part before " - ").
  Reasons with three or more categories ticked at once are grouped.
- **Keyword flags**: reasons or text prompts that mention immigration
  enforcement (including ICE, HSI, ERO, CBP in capitals, so the drug
  slang "ice" does not count), protest, religion, the press or politics, or
  abortion. It lists the matching reasons and the agencies that wrote them.
- **Search scope**: how many networks each search reached (more than 1,000
  usually means statewide or nationwide) and the search types.
- **Operators and plates, counts only**: when names or plates are not
  redacted, the page shows how concentrated the searching is ("Operator 1:
  3,000 searches", "plates searched 10 or more times"). It never shows the
  Name or License Plate columns. Plate-like words inside reasons (such as
  `ABC1234` or `XYZ-123`) are replaced with `[plate]`.

## Data and formats

It reads the column layouts Flock uses as of 2026:

- Organization and Network Audit: `ID, Name, Org Name, Total Networks
  Searched` (or `Camera Count`), `Time Frame, License Plate, Reason, Case #,
  Filters, Search Time, Search Type, Text Prompt, Moderation`;
- transparency portal: `id, userId, searchDate, networkCount, offenseType`.

Columns are found by header name, so their order does not matter. Files may
use commas, semicolons or tabs. Dates such as `04/24/2026, 05:34:58 AM UTC`
`2026-10-22T00:16:00Z` and Excel's `4/24/26 5:34` are recognized. The test fixture is an excerpt
of a real UNC Pembroke Network Audit published by DeFlockILM at the
[Internet Archive](https://archive.org/details/deflockilm-unc-pembroke-alpr-2026).

## Limitations

- Audit logs show only what officers typed and what Flock logs. A keyword
  match or a vague reason is a lead to check, not proof of misuse.
- Months are taken from the timestamp as written (usually UTC), so a few
  searches near midnight may land in the neighboring day or month.
- The state comes from the agency name. Names without a state (for example
  "MOCIC" or "Stark County SO") are counted as "state not recognized". So
  are names where a state name is part of a county, township or city name
  ("Washington County SO", "Kansas City PD"), since those exist in many
  states.
- Reasons and text prompts are shown as the officer wrote them, apart from
  plate-like words. Free text can still name people (victims, suspects).
  Read the flagged-reason tables before you publish the report.
- `ICE`, `HSI`, `ERO` and `CBP` are matched in capitals, so a reason typed
  all in capitals, such as `DRUGS - ICE`, is flagged even when it means
  the drug. Check flagged reasons before citing them.
- `.xlsx` files are not read; save them as CSV.
- Deduplication keeps every search ID it has seen in memory, about 30–40
  bytes each. Tens of millions of rows at once may be too much for a phone,
  so analyze them in batches.

## Build from source

The logic is plain Rust in `src/`; `src/lib.rs` has a thin
`wasm-bindgen` layer for the page in `web/`.

```
cargo test
cargo build --lib --release --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir docs/pkg \
  target/wasm32-unknown-unknown/release/flock_audit_report.wasm
cp -R web/. docs/
```

Then serve `docs/` with any static web server.

Source: https://github.com/SignalForHumanity/flock-audit-report

## License

MIT OR Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.
