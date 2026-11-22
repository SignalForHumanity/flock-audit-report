//! Streams Flock audit CSVs and accumulates aggregate statistics.

use crate::classify::{
    fnv64, id_key, is_redacted, is_vague, jurisdiction, mask_plates, parse_date, parse_state,
    reason_category, Jurisdiction, FLAGS,
};
use crate::csv::{CsvStream, Record};
use crate::report::{num, pct, Section, Table};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Excel's worksheet limit is 1,048,576 rows, one of them the header.
const EXCEL_DATA_ROWS: u64 = 1_048_575;
/// Plates searched at least this often are counted as repeated.
const REPEAT_PLATE: u32 = 10;
const TOP_AGENCIES: usize = 25;
const TOP_REASONS: usize = 25;
const TOP_FLAG_REASONS: usize = 20;
/// Cap on distinct flagged reason texts kept, to bound memory.
const FLAG_REASON_CAP: usize = 2_000;
/// Longest reason text shown in a table cell.
const MAX_CELL: usize = 160;
const MULTI: &str = "Several categories ticked at once";

#[derive(Default, Clone, Copy)]
struct Columns {
    id: Option<usize>,
    name: Option<usize>,
    org: Option<usize>,
    networks: Option<usize>,
    reason: Option<usize>,
    case: Option<usize>,
    time: Option<usize>,
    stype: Option<usize>,
    plate: Option<usize>,
    prompt: Option<usize>,
}

fn norm_header(h: &str) -> String {
    h.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

impl Columns {
    fn from_header(r: &Record) -> Self {
        let mut c = Columns::default();
        for i in 0..r.len() {
            let h = norm_header(&r.get(i));
            let slot = match h.as_str() {
                "id" | "searchid" => &mut c.id,
                "name" | "userid" | "user" | "operator" => &mut c.name,
                "orgname" | "organization" | "organizationname" | "agency" => &mut c.org,
                "totalnetworkssearched" | "networkcount" | "cameracount" | "networks" => {
                    &mut c.networks
                }
                "reason" | "offensetype" | "searchreason" => &mut c.reason,
                "case" | "casenumber" | "caseno" => &mut c.case,
                "searchtime" | "searchdate" | "timestamp" | "date" => &mut c.time,
                "searchtype" => &mut c.stype,
                "licenseplate" | "plate" => &mut c.plate,
                "textprompt" => &mut c.prompt,
                _ => continue,
            };
            if slot.is_none() {
                *slot = Some(i);
            }
        }
        c
    }

    fn recognized(&self) -> bool {
        self.reason.is_some() && (self.id.is_some() || self.time.is_some())
    }
}

struct FileSummary {
    name: String,
    rows: u64,
    recognized: bool,
    first: Option<(u16, u8, u8)>,
    last: Option<(u16, u8, u8)>,
}

struct Current {
    summary: FileSummary,
    cols: Option<Columns>,
}

#[derive(Default)]
struct FlagStats {
    count: u64,
    reasons: HashMap<String, u64>,
    agencies: HashMap<String, u64>,
}

#[derive(Default)]
pub struct Analyzer {
    home: Option<&'static str>,
    csv: CsvStream,
    cur: Option<Current>,
    files: Vec<FileSummary>,

    seen: HashSet<u128>,
    rows: u64,
    unique: u64,
    duplicates: u64,
    no_id: u64,

    agencies: HashMap<String, u64>,
    no_org: u64,
    org_column: bool,
    states: HashMap<&'static str, u64>,
    federal: u64,
    unknown_state: u64,
    in_state: u64,
    out_state: u64,

    months: BTreeMap<(u16, u8), u64>,
    first: Option<(u16, u8, u8)>,
    last: Option<(u16, u8, u8)>,
    undated: u64,

    categories: HashMap<String, u64>,
    blank_reason: u64,
    vague_reason: u64,

    case_column: bool,
    case_given: u64,
    case_redacted: u64,
    case_blank: u64,

    flags: [FlagStats; 3],

    scope: [u64; 6],
    search_types: HashMap<String, u64>,

    operators: HashMap<String, (u64, String)>,
    plates: HashMap<u64, u32>,
}

impl Analyzer {
    /// `home_state` is a code or name such as "NC"; empty for none.
    pub fn new(home_state: &str) -> Self {
        Analyzer {
            home: parse_state(home_state),
            ..Default::default()
        }
    }

    pub fn begin_file(&mut self, name: &str) {
        self.end_file();
        self.cur = Some(Current {
            summary: FileSummary {
                name: name.to_string(),
                rows: 0,
                recognized: false,
                first: None,
                last: None,
            },
            cols: None,
        });
    }

    pub fn push(&mut self, bytes: &[u8]) {
        if self.cur.is_none() {
            self.begin_file("(unnamed)");
        }
        let mut csv = std::mem::take(&mut self.csv);
        csv.push(bytes, &mut |r| self.record(r));
        self.csv = csv;
    }

    pub fn end_file(&mut self) {
        let mut csv = std::mem::take(&mut self.csv);
        csv.finish(&mut |r| self.record(r));
        if let Some(cur) = self.cur.take() {
            self.files.push(cur.summary);
        }
    }

    fn record(&mut self, r: &Record) {
        let Some(cur) = self.cur.as_mut() else { return };
        let cols = match cur.cols {
            Some(c) => c,
            None => {
                let c = Columns::from_header(r);
                cur.cols = Some(c);
                cur.summary.recognized = c.recognized();
                if c.org.is_some() {
                    self.org_column = true;
                }
                if c.case.is_some() {
                    self.case_column = true;
                }
                return;
            }
        };
        if !cur.summary.recognized {
            return;
        }
        cur.summary.rows += 1;
        self.rows += 1;

        let date = cols.time.and_then(|i| parse_date(&r.get(i)));
        if let Some(d) = date {
            cur.summary.first = Some(cur.summary.first.map_or(d, |f| f.min(d)));
            cur.summary.last = Some(cur.summary.last.map_or(d, |l| l.max(d)));
        }

        // Count each search once, even if it appears in several files.
        match cols.id.map(|i| r.get(i)) {
            Some(id) if !is_redacted(&id) => {
                if !self.seen.insert(id_key(&id)) {
                    self.duplicates += 1;
                    return;
                }
                self.unique += 1;
            }
            _ => {
                self.no_id += 1;
                self.unique += 1;
            }
        }

        match date {
            Some(d) => {
                *self.months.entry((d.0, d.1)).or_default() += 1;
                self.first = Some(self.first.map_or(d, |f| f.min(d)));
                self.last = Some(self.last.map_or(d, |l| l.max(d)));
            }
            None => self.undated += 1,
        }

        let org = cols
            .org
            .map(|i| r.get(i).trim().to_string())
            .unwrap_or_default();
        if is_redacted(&org) {
            self.no_org += 1;
        } else {
            let j = jurisdiction(&org);
            match j {
                Jurisdiction::Federal => self.federal += 1,
                Jurisdiction::State(s) => {
                    *self.states.entry(s).or_default() += 1;
                    if let Some(home) = self.home {
                        if s == home {
                            self.in_state += 1;
                        } else {
                            self.out_state += 1;
                        }
                    }
                }
                Jurisdiction::Unknown => self.unknown_state += 1,
            }
            match self.agencies.get_mut(org.as_str()) {
                Some(n) => *n += 1,
                None => {
                    self.agencies.insert(org.clone(), 1);
                }
            }
        }

        let reason = cols
            .reason
            .map(|i| r.get(i).trim().to_string())
            .unwrap_or_default();
        if is_redacted(&reason) {
            self.blank_reason += 1;
        } else {
            let (cat, _) = reason_category(&reason);
            // Old-style reasons are free text; keep plates out of the report.
            let cat = mask_plates(&cat);
            // Some operators tick many categories at once; group those.
            let picked = cat.matches(", ").count() + 1;
            let cat = if picked >= 3 {
                format!("{MULTI} ({picked} categories)")
            } else {
                cat
            };
            if cat.is_empty() {
                self.blank_reason += 1;
            } else {
                *self.categories.entry(cat).or_default() += 1;
                if is_vague(&reason) {
                    self.vague_reason += 1;
                }
            }
        }

        let prompt = cols
            .prompt
            .map(|i| r.get(i).into_owned())
            .unwrap_or_default();
        let text = format!("{reason} {prompt}");
        for (k, flag) in FLAGS.iter().enumerate() {
            if flag.matches(&text) {
                let f = &mut self.flags[k];
                f.count += 1;
                let shown = if reason.is_empty() {
                    prompt.trim()
                } else {
                    &reason
                };
                let shown = mask_plates(shown);
                if f.reasons.len() < FLAG_REASON_CAP || f.reasons.contains_key(&shown) {
                    *f.reasons.entry(shown).or_default() += 1;
                }
                let a = if is_redacted(&org) {
                    "(not given)"
                } else {
                    &org
                };
                *f.agencies.entry(a.to_string()).or_default() += 1;
            }
        }

        if let Some(i) = cols.case {
            let v = r.get(i);
            if v.trim().is_empty() {
                self.case_blank += 1;
            } else if is_redacted(&v) {
                self.case_redacted += 1;
            } else {
                self.case_given += 1;
            }
        }

        let bucket = cols
            .networks
            .and_then(|i| r.get(i).trim().replace(',', "").parse::<u64>().ok())
            .map(|n| match n {
                0..=1 => 0,
                2..=10 => 1,
                11..=100 => 2,
                101..=1000 => 3,
                _ => 4,
            })
            .unwrap_or(5);
        self.scope[bucket] += 1;

        if let Some(i) = cols.stype {
            let t = r.get(i);
            let t = t.trim();
            let t = if t.is_empty() { "(blank)" } else { t };
            *self.search_types.entry(t.to_string()).or_default() += 1;
        }

        if let Some(i) = cols.name {
            let n = r.get(i);
            if !is_redacted(&n) {
                let e = self
                    .operators
                    .entry(n.trim().to_string())
                    .or_insert((0, org.clone()));
                e.0 += 1;
            }
        }
        if let Some(i) = cols.plate {
            let p = r.get(i);
            if !is_redacted(&p) {
                let key: String = p
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .map(|c| c.to_ascii_uppercase())
                    .collect();
                if !key.is_empty() {
                    *self
                        .plates
                        .entry(fnv64(key.as_bytes(), 0xcbf2_9ce4_8422_2325))
                        .or_default() += 1;
                }
            }
        }
    }

    pub fn unique_searches(&self) -> u64 {
        self.unique
    }

    pub fn duplicates(&self) -> u64 {
        self.duplicates
    }

    /// The report, as sections for the HTML and text renderers.
    pub fn sections(&mut self) -> Vec<Section> {
        self.end_file();
        let mut out = Vec::new();
        let u = self.unique;

        // Files
        let mut s = Section::new("Files read");
        let mut t = Table::new(&["File", "Rows", "From", "To"], &[1]);
        for f in &self.files {
            if !f.recognized {
                s.warnings.push(format!(
                    "\"{}\" does not look like a Flock search audit (no Reason and ID or Search Time columns). It was skipped.",
                    f.name
                ));
                continue;
            }
            if f.rows == EXCEL_DATA_ROWS || f.rows == EXCEL_DATA_ROWS + 1 {
                s.warnings.push(format!(
                    "\"{}\" has exactly {} rows, Excel's limit. It was probably cut off when someone opened and saved it in Excel. Ask the agency for the original CSV export.",
                    f.name,
                    num(f.rows)
                ));
            }
            t.row(vec![
                f.name.clone(),
                num(f.rows),
                fmt_date(f.first),
                fmt_date(f.last),
            ]);
        }
        s.tables.push(t);
        out.push(s);

        if self.rows == 0 {
            let mut s = Section::new("No searches found");
            s.notes
                .push("None of the files contained Flock search rows.".into());
            out.push(s);
            return out;
        }

        // Totals
        let mut s = Section::new("Searches");
        let mut t = Table::new(&["", "Count"], &[1]);
        t.row(vec!["Rows read".into(), num(self.rows)]);
        t.row(vec![
            "Duplicate search IDs removed".into(),
            num(self.duplicates),
        ]);
        t.row(vec!["Unique searches".into(), num(u)]);
        if self.no_id > 0 {
            t.row(vec!["  of which have no search ID".into(), num(self.no_id)]);
        }
        s.tables.push(t);
        s.notes.push(format!(
            "Searches from {} to {}. One search across the shared network appears in every audit it touched, so the same ID can be in several files; it is counted once here. All numbers below count unique searches.",
            fmt_date(self.first),
            fmt_date(self.last)
        ));
        out.push(s);

        // Months
        let mut s = Section::new("Searches per month");
        let mut t = Table::new(&["Month", "Searches"], &[1]);
        for ((y, m), n) in &self.months {
            t.row(vec![format!("{y}-{m:02}"), num(*n)]);
        }
        if self.undated > 0 {
            t.row(vec!["(no date)".into(), num(self.undated)]);
        }
        s.notes.push(
            "A month at either end may be partial, depending on the dates the records cover."
                .into(),
        );
        s.tables.push(t);
        out.push(s);

        // Agencies and states
        let mut s = Section::new("Who searched");
        if !self.org_column {
            s.notes.push("These files do not name the searching agency (transparency-portal audits list only the agency's own searches). Ask for the Organization Audit and Network Audit to see who searched.".into());
        } else {
            let named = u - self.no_org;
            s.notes.push(format!(
                "{} distinct agencies ran the {} searches that name one.",
                num(self.agencies.len() as u64),
                num(named)
            ));
            if self.no_org > 0 {
                s.warnings.push(format!(
                    "{} searches ({}) have the searching agency blank or redacted.",
                    num(self.no_org),
                    pct(self.no_org, u)
                ));
            }
            let mut t = Table::new(&["Where the searcher is", "Searches", "Share"], &[1, 2]);
            if let Some(home) = self.home {
                t.row(vec![
                    format!("In {home}"),
                    num(self.in_state),
                    pct(self.in_state, named),
                ]);
                t.row(vec![
                    format!("Other states (outside {home})"),
                    num(self.out_state),
                    pct(self.out_state, named),
                ]);
            }
            t.row(vec![
                "Federal agencies".into(),
                num(self.federal),
                pct(self.federal, named),
            ]);
            t.row(vec![
                "State not recognized".into(),
                num(self.unknown_state),
                pct(self.unknown_state, named),
            ]);
            s.tables.push(t);
            if self.home.is_none() {
                s.notes.push(
                    "Enter the home state to see the in-state and out-of-state split.".into(),
                );
            }

            let mut states: Vec<_> = self.states.iter().collect();
            states.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let mut t = Table::new(&["State", "Searches", "Share"], &[1, 2]);
            for (st, n) in states {
                t.row(vec![st.to_string(), num(*n), pct(*n, named)]);
            }
            s.tables.push(t);

            let mut ag: Vec<_> = self.agencies.iter().collect();
            ag.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let mut t = Table::new(&["Agency", "State", "Searches", "Share"], &[2, 3]);
            for (a, n) in ag.iter().take(TOP_AGENCIES) {
                t.row(vec![
                    a.to_string(),
                    jurisdiction(a).label().into(),
                    num(**n),
                    pct(**n, named),
                ]);
            }
            s.notes.push(format!(
                "The {} busiest agencies are listed; download the agency CSV for all of them. The state is read from the agency name.",
                TOP_AGENCIES.min(ag.len())
            ));
            s.tables.push(t);
        }
        out.push(s);

        // Reasons
        let mut s = Section::new("Stated reasons");
        let mut t = Table::new(&["", "Searches", "Share"], &[1, 2]);
        t.row(vec![
            "Reason blank or redacted".into(),
            num(self.blank_reason),
            pct(self.blank_reason, u),
        ]);
        t.row(vec![
            "Vague reason (\"investigation\", \"other\", 3 characters or fewer)".into(),
            num(self.vague_reason),
            pct(self.vague_reason, u),
        ]);
        if self.case_column {
            t.row(vec![
                "Case number given".into(),
                num(self.case_given),
                pct(self.case_given, u),
            ]);
            t.row(vec![
                "Case number redacted".into(),
                num(self.case_redacted),
                pct(self.case_redacted, u),
            ]);
            t.row(vec![
                "Case number blank".into(),
                num(self.case_blank),
                pct(self.case_blank, u),
            ]);
        }
        s.tables.push(t);
        let mut cats: Vec<_> = self.categories.iter().collect();
        cats.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let mut t = Table::new(&["Reason category", "Searches", "Share"], &[1, 2]);
        for (c, n) in cats.iter().take(TOP_REASONS) {
            t.row(vec![shorten(c), num(**n), pct(**n, u)]);
        }
        s.notes.push("Categories are the part of the reason before \" - \" (Flock's drop-down list); the officer's free text follows it. Check your state's law for reasons it does not allow, such as traffic infractions.".into());
        s.tables.push(t);
        out.push(s);

        // Flags
        let mut s = Section::new("Keyword flags");
        s.notes.push("Searches whose reason or text prompt contains words about immigration enforcement (including the acronyms ICE, HSI, ERO, CBP in capitals), protest, religion, the press or politics, or abortion. A keyword match is a lead to check, not proof of misuse.".into());
        let mut t = Table::new(&["Flag", "Searches", "Share"], &[1, 2]);
        for (k, flag) in FLAGS.iter().enumerate() {
            t.row(vec![
                flag.label().into(),
                num(self.flags[k].count),
                pct(self.flags[k].count, u),
            ]);
        }
        s.tables.push(t);
        out.push(s);
        for (k, flag) in FLAGS.iter().enumerate() {
            let f = &self.flags[k];
            if f.count == 0 {
                continue;
            }
            let mut s = Section::new(&format!("Flagged: {}", flag.label()));
            let mut reasons: Vec<_> = f.reasons.iter().collect();
            reasons.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let mut t = Table::new(&["Reason as written", "Searches"], &[1]);
            for (r, n) in reasons.iter().take(TOP_FLAG_REASONS) {
                t.row(vec![shorten(r), num(**n)]);
            }
            s.tables.push(t);
            let mut ag: Vec<_> = f.agencies.iter().collect();
            ag.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let mut t = Table::new(&["Agency", "Searches"], &[1]);
            for (a, n) in ag.iter().take(TOP_FLAG_REASONS) {
                t.row(vec![a.to_string(), num(**n)]);
            }
            s.tables.push(t);
            out.push(s);
        }

        // Scope
        let mut s = Section::new("Search scope");
        s.notes.push("How many networks each search reached (or cameras, for files that give only a camera count). Searches of more than 1,000 networks are usually statewide or nationwide.".into());
        let labels = [
            "1",
            "2–10",
            "11–100",
            "101–1,000",
            "More than 1,000",
            "Not given",
        ];
        let mut t = Table::new(&["Networks searched", "Searches", "Share"], &[1, 2]);
        for (i, l) in labels.iter().enumerate() {
            if i == 5 && self.scope[5] == 0 {
                continue;
            }
            t.row(vec![
                l.to_string(),
                num(self.scope[i]),
                pct(self.scope[i], u),
            ]);
        }
        s.tables.push(t);
        if !self.search_types.is_empty() {
            let mut st: Vec<_> = self.search_types.iter().collect();
            st.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let mut t = Table::new(&["Search type", "Searches", "Share"], &[1, 2]);
            for (n, c) in st {
                t.row(vec![n.to_string(), num(*c), pct(*c, u)]);
            }
            s.tables.push(t);
        }
        out.push(s);

        // Operators and plates: counts only.
        if !self.operators.is_empty() || !self.plates.is_empty() {
            let mut s = Section::new("Operators and plates (counts only)");
            s.notes.push("These files include operator names or plate numbers. This report never shows them: only how concentrated the searching is. Look up specific rows in the original file if you need to.".into());
            if !self.operators.is_empty() {
                let mut ops: Vec<_> = self.operators.values().collect();
                ops.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
                s.notes.push(format!(
                    "{} distinct operators named.",
                    num(ops.len() as u64)
                ));
                let mut t = Table::new(&["Operator", "Agency (first seen)", "Searches"], &[2]);
                for (i, (n, org)) in ops.iter().take(10).enumerate() {
                    t.row(vec![format!("Operator {}", i + 1), org.clone(), num(*n)]);
                }
                s.tables.push(t);
            }
            if !self.plates.is_empty() {
                let repeated = self.plates.values().filter(|n| **n >= REPEAT_PLATE).count() as u64;
                let max = self.plates.values().copied().max().unwrap_or(0) as u64;
                let mut t = Table::new(&["Plates", "Count"], &[1]);
                t.row(vec![
                    "Distinct plates searched".into(),
                    num(self.plates.len() as u64),
                ]);
                t.row(vec![
                    format!("Plates searched {REPEAT_PLATE} or more times"),
                    num(repeated),
                ]);
                t.row(vec!["Most searches of a single plate".into(), num(max)]);
                s.tables.push(t);
            }
            out.push(s);
        }

        let mut s = Section::new("About these numbers");
        s.notes.push("Audit logs record only what officers typed and what Flock chose to log. Use these counts to decide where to dig and what to ask for, not as a verdict on any search or person.".into());
        out.push(s);
        out
    }

    /// Every searching agency with its classification and count, as CSV.
    pub fn agencies_csv(&mut self) -> String {
        self.end_file();
        let mut ag: Vec<_> = self.agencies.iter().collect();
        ag.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let mut out = String::from("agency,state,searches\n");
        for (a, n) in ag {
            out.push_str(&csv_cell(a));
            out.push(',');
            out.push_str(jurisdiction(a).label());
            out.push(',');
            out.push_str(&n.to_string());
            out.push('\n');
        }
        out
    }
}

fn csv_cell(s: &str) -> String {
    // Neutralize spreadsheet formulas and quote when needed.
    let s = if s.starts_with(['=', '+', '-', '@']) {
        format!("'{s}")
    } else {
        s.to_string()
    };
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

fn shorten(s: &str) -> String {
    if s.chars().count() <= MAX_CELL {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(MAX_CELL).collect();
        t.push('…');
        t
    }
}

fn fmt_date(d: Option<(u16, u8, u8)>) -> String {
    match d {
        Some((y, m, d)) => format!("{y}-{m:02}-{d:02}"),
        None => "–".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_cells_are_safe() {
        assert_eq!(csv_cell("Houston TX PD"), "Houston TX PD");
        assert_eq!(csv_cell("A, B"), "\"A, B\"");
        assert_eq!(csv_cell("=cmd"), "'=cmd");
    }

    #[test]
    fn long_reasons_are_shortened() {
        assert_eq!(shorten("abc"), "abc");
        let long = "x".repeat(500);
        assert_eq!(shorten(&long).chars().count(), MAX_CELL + 1);
    }

    #[test]
    fn many_categories_are_grouped() {
        let mut a = Analyzer::new("");
        a.push(b"ID,Reason\n1,\"Arson, Robbery, Stalking - \"\n2,\"Arson, Robbery - x\"\n");
        let text = crate::report::to_text(&a.sections());
        assert!(text.contains("Several categories ticked at once (3 categories)"));
        assert!(text.contains("Arson, Robbery "));
    }

    #[test]
    fn header_aliases() {
        let mut s = CsvStream::new();
        let mut cols = Columns::default();
        s.push(b" ID, Name, Org Name, Case #, Search Time\n", &mut |r| {
            cols = Columns::from_header(r)
        });
        assert_eq!(cols.id, Some(0));
        assert_eq!(cols.org, Some(2));
        assert_eq!(cols.case, Some(3));
        assert_eq!(cols.time, Some(4));
        assert!(!cols.recognized());
    }
}
