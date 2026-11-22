//! Classification rules: searching agency state and federal status, reason
//! categories, vague reasons, keyword flags, redaction, dates.

const CODES: [&str; 51] = [
    "AL", "AK", "AZ", "AR", "CA", "CO", "CT", "DE", "FL", "GA", "HI", "ID", "IL", "IN", "IA", "KS",
    "KY", "LA", "ME", "MD", "MA", "MI", "MN", "MS", "MO", "MT", "NE", "NV", "NH", "NJ", "NM", "NY",
    "NC", "ND", "OH", "OK", "OR", "PA", "RI", "SC", "SD", "TN", "TX", "UT", "VT", "VA", "WA", "WV",
    "WI", "WY", "DC",
];

// Two-word names first so "west virginia" wins over "virginia".
const NAMES: [(&str, &str); 51] = [
    ("new hampshire", "NH"),
    ("new jersey", "NJ"),
    ("new mexico", "NM"),
    ("new york", "NY"),
    ("north carolina", "NC"),
    ("north dakota", "ND"),
    ("rhode island", "RI"),
    ("south carolina", "SC"),
    ("south dakota", "SD"),
    ("west virginia", "WV"),
    ("district of columbia", "DC"),
    ("alabama", "AL"),
    ("alaska", "AK"),
    ("arizona", "AZ"),
    ("arkansas", "AR"),
    ("california", "CA"),
    ("colorado", "CO"),
    ("connecticut", "CT"),
    ("delaware", "DE"),
    ("florida", "FL"),
    ("georgia", "GA"),
    ("hawaii", "HI"),
    ("idaho", "ID"),
    ("illinois", "IL"),
    ("indiana", "IN"),
    ("iowa", "IA"),
    ("kansas", "KS"),
    ("kentucky", "KY"),
    ("louisiana", "LA"),
    ("maine", "ME"),
    ("maryland", "MD"),
    ("massachusetts", "MA"),
    ("michigan", "MI"),
    ("minnesota", "MN"),
    ("mississippi", "MS"),
    ("missouri", "MO"),
    ("montana", "MT"),
    ("nebraska", "NE"),
    ("nevada", "NV"),
    ("ohio", "OH"),
    ("oklahoma", "OK"),
    ("oregon", "OR"),
    ("pennsylvania", "PA"),
    ("tennessee", "TN"),
    ("texas", "TX"),
    ("utah", "UT"),
    ("vermont", "VT"),
    ("virginia", "VA"),
    ("washington", "WA"),
    ("wisconsin", "WI"),
    ("wyoming", "WY"),
];

const FEDERAL: [&str; 18] = [
    "united states marshals",
    "fbi",
    "dea",
    "atf",
    "usms",
    "us marshals",
    "u s marshals",
    "united states",
    "federal bureau",
    "hsi",
    "cbp",
    "border patrol",
    "customs and border",
    "immigration and customs",
    "postal inspection",
    "secret service",
    "ncis",
    "naval criminal",
];

/// Federal only when the name has no state in it ("Tennessee Office of
/// Homeland Security" is a state agency).
const FEDERAL_WEAK: [&str; 3] = [
    "homeland security",
    "department of justice",
    "us department",
];

/// True if a cell is empty or only says it was redacted.
pub fn is_redacted(v: &str) -> bool {
    let t = v.trim();
    t.is_empty()
        || t.chars().all(|c| c == '*')
        || t.eq_ignore_ascii_case("redacted")
        || t.eq_ignore_ascii_case("[redacted]")
}

/// Lowercase words separated by single spaces, padded with spaces at both
/// ends so phrases can be matched with `contains(" phrase ")`.
pub fn word_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push(' ');
    let mut last_space = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    if !last_space {
        out.push(' ');
    }
    out
}

fn has_phrase(words: &str, phrase: &str) -> bool {
    let mut p = String::with_capacity(phrase.len() + 2);
    p.push(' ');
    p.push_str(phrase);
    p.push(' ');
    words.contains(&p)
}

fn has_upper_token(s: &str, tokens: &[&str]) -> bool {
    s.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|t| tokens.contains(&t))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Jurisdiction {
    Federal,
    State(&'static str),
    Unknown,
}

impl Jurisdiction {
    pub fn label(self) -> &'static str {
        match self {
            Jurisdiction::Federal => "Federal",
            Jurisdiction::State(s) => s,
            Jurisdiction::Unknown => "?",
        }
    }
}

/// Classify a searching agency by its Flock organization name.
pub fn jurisdiction(org: &str) -> Jurisdiction {
    let org = org.trim();
    let words = word_string(org);
    if org.starts_with("[Federal]") {
        return Jurisdiction::Federal;
    }
    let federal_word = FEDERAL.iter().any(|f| has_phrase(&words, f));
    // "Federal Way WA PD" is a city police department.
    if federal_word && !has_phrase(&words, "federal way") {
        return Jurisdiction::Federal;
    }
    if let Some(code) = state_in(org, &words) {
        return Jurisdiction::State(code);
    }
    if FEDERAL_WEAK.iter().any(|f| has_phrase(&words, f)) {
        return Jurisdiction::Federal;
    }
    Jurisdiction::Unknown
}

fn state_in(org: &str, words: &str) -> Option<&'static str> {
    for t in org.split(|c: char| !c.is_ascii_alphanumeric()) {
        if t.len() == 2 {
            if let Some(code) = CODES.iter().find(|c| **c == t) {
                return Some(code);
            }
        }
    }
    NAMES
        .iter()
        .find(|(name, _)| has_state_name(words, name))
        .map(|(_, code)| *code)
}

/// Cities named after their state that are in that state.
const SAME_STATE_CITIES: [&str; 5] = [
    "oklahoma city",
    "iowa city",
    "texas city",
    "florida city",
    "delaware city",
];

/// A state name used as a state, not as part of a county, township or city
/// name ("Washington County" exists in 30 states, "Kansas City" in two).
fn has_state_name(words: &str, name: &str) -> bool {
    let p = format!(" {name} ");
    let mut from = 0;
    while let Some(pos) = words[from..].find(&p) {
        let at = from + pos;
        let next = words[at + p.len()..].split(' ').next().unwrap_or("");
        let ok = match next {
            "county" | "co" | "township" | "twp" | "parish" => false,
            "city" => SAME_STATE_CITIES.contains(&format!("{name} city").as_str()),
            _ => true,
        };
        if ok {
            return true;
        }
        from = at + p.len() - 1;
    }
    false
}

/// Normalize a user-entered home state ("nc", "North Carolina") to a code.
pub fn parse_state(input: &str) -> Option<&'static str> {
    let t = input.trim();
    if t.is_empty() {
        return None;
    }
    let up = t.to_ascii_uppercase();
    if let Some(c) = CODES.iter().find(|c| **c == up) {
        return Some(c);
    }
    let words = word_string(t);
    NAMES
        .iter()
        .find(|(n, _)| words.trim() == *n)
        .map(|(_, c)| *c)
}

/// Split a Flock reason into its dropdown category and the free text after
/// " - ". Old-style free-text reasons have no category; the whole text is
/// the category then.
pub fn reason_category(reason: &str) -> (String, String) {
    let r = reason.trim();
    match r.split_once(" - ") {
        Some((cat, rest)) => (cat.trim().to_string(), rest.trim().to_string()),
        None => (r.trim_end_matches('-').trim().to_string(), String::new()),
    }
}

const VAGUE: [&str; 22] = [
    "investigation",
    "investigations",
    "invest",
    "inv",
    "invst",
    "other",
    "none",
    "n a",
    "na",
    "test",
    "testing",
    "unknown",
    "misc",
    "general",
    "case",
    "see case",
    "criminal",
    "crime",
    "criminal investigation",
    "law enforcement",
    "le",
    "ongoing investigation",
];

fn is_vague_text(t: &str) -> bool {
    let w = word_string(t);
    let w = w.trim();
    let alnum = w.chars().filter(|c| c.is_alphanumeric()).count();
    alnum <= 3 || VAGUE.contains(&w)
}

/// A reason that names no specific purpose: blank free text under a vague
/// category, a vague word such as "investigation", or 3 characters or fewer.
pub fn is_vague(reason: &str) -> bool {
    let (cat, rest) = reason_category(reason);
    if rest.is_empty() || is_redacted(&rest) {
        is_vague_text(&cat)
    } else {
        is_vague_text(&cat) && is_vague_text(&rest)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flag {
    Immigration,
    FirstAmendment,
    Reproductive,
}

pub const FLAGS: [Flag; 3] = [Flag::Immigration, Flag::FirstAmendment, Flag::Reproductive];

impl Flag {
    pub fn label(self) -> &'static str {
        match self {
            Flag::Immigration => "Immigration enforcement",
            Flag::FirstAmendment => "Protest, religion, press or politics",
            Flag::Reproductive => "Abortion or reproductive care",
        }
    }

    fn phrases(self) -> &'static [&'static str] {
        match self {
            Flag::Immigration => &[
                "immigration",
                "immigrant",
                "immigrants",
                "deport",
                "deportation",
                "deportee",
                "illegal alien",
                "illegal aliens",
                "alien",
                "aliens",
                "undocumented",
                "border patrol",
                "removal operations",
                "homeland security investigations",
                "287g",
                "287 g",
                "enforcement and removal",
            ],
            Flag::FirstAmendment => &[
                "protest",
                "protests",
                "protester",
                "protesters",
                "protestor",
                "protestors",
                "rally",
                "demonstration",
                "demonstrators",
                "demonstrator",
                "picket",
                "boycott",
                "vigil",
                "church",
                "mosque",
                "synagogue",
                "journalist",
                "journalists",
                "reporter",
                "reporters",
                "activist",
                "activists",
                "organizer",
                "political",
                "antifa",
                "first amendment",
                "no kings",
            ],
            Flag::Reproductive => &[
                "abortion",
                "abortions",
                "reproductive",
                "planned parenthood",
                "miscarriage",
            ],
        }
    }

    /// Uppercase acronyms matched case-sensitively, so the drug slang "ice"
    /// does not count as ICE.
    fn acronyms(self) -> &'static [&'static str] {
        match self {
            Flag::Immigration => &["ICE", "HSI", "ERO", "CBP", "USBP"],
            _ => &[],
        }
    }

    pub fn matches(self, text: &str) -> bool {
        let words = word_string(text);
        self.phrases().iter().any(|p| has_phrase(&words, p))
            || has_upper_token(text, self.acronyms())
    }
}

fn plate_like(t: &str) -> bool {
    (5..=8).contains(&t.len())
        && t.bytes().any(|b| b.is_ascii_alphabetic())
        && t.bytes().any(|b| b.is_ascii_digit())
}

/// Replace plate-like tokens in free text ("ABC1234", "XYZ-1234") with
/// "[plate]", so reasons shown in the report do not reveal plate numbers.
/// A plate-like token has 5 to 8 letters and digits, with at least one of
/// each, optionally split once by a hyphen into parts of 2 to 4.
pub fn mask_plates(s: &str) -> String {
    // Split into alternating runs of ASCII alphanumerics and other text.
    let mut parts: Vec<(bool, &str)> = Vec::new();
    let mut start = 0;
    let mut kind = None;
    for (i, c) in s.char_indices() {
        let k = c.is_ascii_alphanumeric();
        if kind.is_some_and(|p| p != k) {
            parts.push((kind.unwrap_or(false), &s[start..i]));
            start = i;
        }
        kind = Some(k);
    }
    if let Some(k) = kind {
        parts.push((k, &s[start..]));
    }
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < parts.len() {
        let (alnum, t) = parts[i];
        if alnum && i + 2 < parts.len() && parts[i + 1].1 == "-" && parts[i + 2].0 {
            let u = parts[i + 2].1;
            if (2..=4).contains(&t.len())
                && (2..=4).contains(&u.len())
                && plate_like(&format!("{t}{u}"))
            {
                out.push_str("[plate]");
                i += 3;
                continue;
            }
        }
        if alnum && plate_like(t) {
            out.push_str("[plate]");
        } else {
            out.push_str(t);
        }
        i += 1;
    }
    out
}

/// Find a calendar date in a Flock timestamp: "04/24/2026, 05:34:58 AM UTC",
/// "2026-10-22T00:16:00.239Z" or "2026-10-22 00:16:00".
pub fn parse_date(s: &str) -> Option<(u16, u8, u8)> {
    let s = s.trim();
    let b = s.as_bytes();
    let num = |a: usize, n: usize| -> Option<u32> {
        let part = s.get(a..a + n)?;
        if part.bytes().all(|c| c.is_ascii_digit()) {
            part.parse().ok()
        } else {
            None
        }
    };
    let (y, m, d) = if b.len() >= 10 && b[4] == b'-' && b[7] == b'-' {
        (num(0, 4)?, num(5, 2)?, num(8, 2)?)
    } else {
        // M/D/YYYY with one- or two-digit month and day.
        let date = s.split([',', ' ', 'T']).next()?;
        let mut it = date.split('/');
        let m: u32 = it.next()?.parse().ok()?;
        let d: u32 = it.next()?.parse().ok()?;
        let y: u32 = it.next()?.parse().ok()?;
        if it.next().is_some() {
            return None;
        }
        // Excel often re-saves dates with a two-digit year ("4/24/26").
        let y = if y < 100 { 2000 + y } else { y };
        (y, m, d)
    };
    if !(1990..=2100).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some((y as u16, m as u8, d as u8))
}

/// Parse a Flock search UUID into 128 bits; other IDs are hashed.
pub fn id_key(id: &str) -> u128 {
    let t = id.trim();
    let hex: String = t.chars().filter(|c| *c != '-').collect();
    if hex.len() == 32 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        if let Ok(v) = u128::from_str_radix(&hex, 16) {
            return v;
        }
    }
    let a = fnv64(t.as_bytes(), 0xcbf2_9ce4_8422_2325);
    let b = fnv64(t.as_bytes(), 0x84222325_cbf29ce4);
    ((a as u128) << 64) | b as u128
}

pub fn fnv64(bytes: &[u8], seed: u64) -> u64 {
    let mut h = seed;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jurisdictions() {
        assert_eq!(jurisdiction("Houston TX PD"), Jurisdiction::State("TX"));
        assert_eq!(
            jurisdiction("Grant County AR SO "),
            Jurisdiction::State("AR")
        );
        assert_eq!(
            jurisdiction("Texas Department of Public Safety"),
            Jurisdiction::State("TX")
        );
        assert_eq!(
            jurisdiction("West Virginia State Police"),
            Jurisdiction::State("WV")
        );
        assert_eq!(
            jurisdiction("Arkansas State Police"),
            Jurisdiction::State("AR")
        );
        assert_eq!(jurisdiction("[Federal] FBI"), Jurisdiction::Federal);
        assert_eq!(
            jurisdiction("FBI Charlotte Field Office"),
            Jurisdiction::Federal
        );
        assert_eq!(
            jurisdiction("US Postal Inspection Service"),
            Jurisdiction::Federal
        );
        assert_eq!(jurisdiction("Federal Way WA PD"), Jurisdiction::State("WA"));
        assert_eq!(
            jurisdiction("Quapaw Nation OK Marshals Service"),
            Jurisdiction::State("OK")
        );
        assert_eq!(jurisdiction("US Marshals Service"), Jurisdiction::Federal);
        assert_eq!(jurisdiction("MOCIC "), Jurisdiction::Unknown);
        // County, township and city names that are also state names.
        assert_eq!(
            jurisdiction("Washington County Sheriff's Office"),
            Jurisdiction::Unknown
        );
        assert_eq!(jurisdiction("Delaware County SO"), Jurisdiction::Unknown);
        assert_eq!(jurisdiction("Ohio County SO"), Jurisdiction::Unknown);
        assert_eq!(
            jurisdiction("Washington Township PD"),
            Jurisdiction::Unknown
        );
        assert_eq!(
            jurisdiction("Kansas City Police Department"),
            Jurisdiction::Unknown
        );
        assert_eq!(
            jurisdiction("Michigan City Police Department"),
            Jurisdiction::Unknown
        );
        assert_eq!(
            jurisdiction("Oklahoma City Police Department"),
            Jurisdiction::State("OK")
        );
        assert_eq!(
            jurisdiction("Washington State Patrol"),
            Jurisdiction::State("WA")
        );
        assert_eq!(
            jurisdiction("Washington County AR SO"),
            Jurisdiction::State("AR")
        );
        assert_eq!(jurisdiction("Ohio DPS"), Jurisdiction::State("OH"));
        assert_eq!(
            jurisdiction("Tennessee Office of Homeland Security"),
            Jurisdiction::State("TN")
        );
        assert_eq!(
            jurisdiction("Department of Homeland Security"),
            Jurisdiction::Federal
        );
    }

    #[test]
    fn home_state_input() {
        assert_eq!(parse_state("nc"), Some("NC"));
        assert_eq!(parse_state(" North Carolina "), Some("NC"));
        assert_eq!(parse_state(""), None);
        assert_eq!(parse_state("Narnia"), None);
    }

    #[test]
    fn reasons() {
        assert_eq!(
            reason_category("Drugs/Narcotics - Drug investigation"),
            ("Drugs/Narcotics".into(), "Drug investigation".into())
        );
        assert_eq!(
            reason_category("Traffic Infraction - "),
            ("Traffic Infraction".into(), String::new())
        );
        assert_eq!(reason_category("inv"), ("inv".into(), String::new()));
        assert!(is_vague("inv"));
        assert!(is_vague("Investigation"));
        assert!(is_vague("Other - "));
        assert!(is_vague("Other - investigation"));
        assert!(is_vague("x"));
        assert!(!is_vague("Other - stolen trailer"));
        assert!(!is_vague("Traffic Infraction - "));
        assert!(!is_vague("Robbery"));
    }

    #[test]
    fn flags() {
        assert!(Flag::Immigration.matches("Other - ICE assist"));
        assert!(Flag::Immigration.matches("immigration violation"));
        assert!(!Flag::Immigration.matches("Drugs/Narcotics - ice and meth"));
        assert!(!Flag::Immigration.matches("Police service"));
        assert!(Flag::FirstAmendment.matches("Other - protest downtown"));
        assert!(!Flag::FirstAmendment.matches("Theft in March"));
        assert!(Flag::Reproductive.matches("abortion"));
        assert!(!Flag::Reproductive.matches("Robbery"));
    }

    #[test]
    fn plate_masking() {
        assert_eq!(
            mask_plates("Other - veh ABC1234, also xyz-123"),
            "Other - veh [plate], also [plate]"
        );
        assert_eq!(
            mask_plates("287g ICE 24-001 F-150"),
            "287g ICE 24-001 F-150"
        );
        assert_eq!(mask_plates("Robbery – café"), "Robbery – café");
    }

    #[test]
    fn dates() {
        assert_eq!(
            parse_date("04/24/2026, 05:34:58 AM UTC"),
            Some((2026, 4, 24))
        );
        assert_eq!(parse_date("2026-10-22T00:16:00.239Z"), Some((2026, 10, 22)));
        assert_eq!(parse_date("2026-10-22 00:16:00"), Some((2026, 10, 22)));
        assert_eq!(parse_date("4/2/2025 1:00 PM"), Some((2025, 4, 2)));
        assert_eq!(parse_date("4/24/26 5:34"), Some((2026, 4, 24)));
        assert_eq!(parse_date("***"), None);
        assert_eq!(parse_date("13/40/2026"), None);
    }

    #[test]
    fn ids_and_redaction() {
        assert_eq!(
            id_key("bf12ee05-ad76-4f21-b264-b1c6a1997597"),
            0xbf12ee05ad764f21b264b1c6a1997597
        );
        assert_eq!(id_key("abc"), id_key(" abc "));
        assert_ne!(id_key("abc"), id_key("abd"));
        assert!(is_redacted("***"));
        assert!(is_redacted(" "));
        assert!(is_redacted("REDACTED"));
        assert!(!is_redacted("ABC123"));
    }
}
