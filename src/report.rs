//! A report is a list of sections with notes and tables, rendered either as
//! an HTML fragment for the page or as plain text to paste elsewhere.

pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// Columns (by index) that hold numbers and are right-aligned.
    pub numeric: Vec<usize>,
}

impl Table {
    pub fn new(headers: &[&str], numeric: &[usize]) -> Self {
        Table {
            headers: headers.iter().map(|h| h.to_string()).collect(),
            rows: Vec::new(),
            numeric: numeric.to_vec(),
        }
    }

    pub fn row(&mut self, cells: Vec<String>) {
        self.rows.push(cells);
    }
}

#[derive(Default)]
pub struct Section {
    pub title: String,
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
    pub tables: Vec<Table>,
}

impl Section {
    pub fn new(title: &str) -> Self {
        Section {
            title: title.to_string(),
            ..Default::default()
        }
    }
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

pub fn to_html(sections: &[Section]) -> String {
    let mut h = String::new();
    for s in sections {
        h.push_str("<section>\n<h2>");
        h.push_str(&escape(&s.title));
        h.push_str("</h2>\n");
        for w in &s.warnings {
            h.push_str("<p class=\"warn\">");
            h.push_str(&escape(w));
            h.push_str("</p>\n");
        }
        for n in &s.notes {
            h.push_str("<p>");
            h.push_str(&escape(n));
            h.push_str("</p>\n");
        }
        for t in &s.tables {
            h.push_str("<div class=\"scroll\"><table>\n<thead><tr>");
            for (i, c) in t.headers.iter().enumerate() {
                let cls = if t.numeric.contains(&i) {
                    " class=\"n\""
                } else {
                    ""
                };
                h.push_str(&format!("<th{cls}>{}</th>", escape(c)));
            }
            h.push_str("</tr></thead>\n<tbody>\n");
            for r in &t.rows {
                h.push_str("<tr>");
                for (i, c) in r.iter().enumerate() {
                    let cls = if t.numeric.contains(&i) {
                        " class=\"n\""
                    } else {
                        ""
                    };
                    h.push_str(&format!("<td{cls}>{}</td>", escape(c)));
                }
                h.push_str("</tr>\n");
            }
            h.push_str("</tbody></table></div>\n");
        }
        h.push_str("</section>\n");
    }
    h
}

pub fn to_text(sections: &[Section]) -> String {
    let mut out = String::new();
    for s in sections {
        out.push_str(&s.title);
        out.push('\n');
        out.push_str(&"=".repeat(s.title.chars().count()));
        out.push('\n');
        for w in &s.warnings {
            out.push_str("WARNING: ");
            out.push_str(w);
            out.push('\n');
        }
        for n in &s.notes {
            out.push_str(n);
            out.push('\n');
        }
        for t in &s.tables {
            out.push('\n');
            let cols = t.headers.len();
            let mut widths: Vec<usize> = t.headers.iter().map(|h| h.chars().count()).collect();
            for r in &t.rows {
                for (i, c) in r.iter().enumerate().take(cols) {
                    widths[i] = widths[i].max(c.chars().count());
                }
            }
            let line = |cells: &[String], out: &mut String| {
                let mut l = String::new();
                for (i, c) in cells.iter().enumerate().take(cols) {
                    let pad = widths[i] - c.chars().count();
                    if i > 0 {
                        l.push_str("  ");
                    }
                    if t.numeric.contains(&i) {
                        l.push_str(&" ".repeat(pad));
                        l.push_str(c);
                    } else {
                        l.push_str(c);
                        l.push_str(&" ".repeat(pad));
                    }
                }
                out.push_str(l.trim_end());
                out.push('\n');
            };
            line(&t.headers, &mut out);
            let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
            line(&rule, &mut out);
            for r in &t.rows {
                line(r, &mut out);
            }
        }
        out.push('\n');
    }
    out
}

/// 1234567 -> "1,234,567"
pub fn num(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Share as a percentage with one decimal, e.g. "12.5%".
pub fn pct(part: u64, whole: u64) -> String {
    if whole == 0 {
        return "–".to_string();
    }
    format!("{:.1}%", part as f64 * 100.0 / whole as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(num(0), "0");
        assert_eq!(num(999), "999");
        assert_eq!(num(1000), "1,000");
        assert_eq!(num(1234567), "1,234,567");
        assert_eq!(pct(1, 8), "12.5%");
        assert_eq!(pct(1, 0), "–");
    }

    #[test]
    fn html_is_escaped_and_text_aligned() {
        let mut s = Section::new("A <b>");
        s.notes.push("x & y".into());
        let mut t = Table::new(&["Name", "Count"], &[1]);
        t.row(vec!["<script>".into(), "5".into()]);
        t.row(vec!["b".into(), "1,000".into()]);
        s.tables.push(t);
        let html = to_html(std::slice::from_ref(&s));
        assert!(html.contains("A &lt;b&gt;"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
        let text = to_text(&[s]);
        assert!(text.contains("<script>      5"));
        assert!(text.contains("b         1,000"));
    }
}
