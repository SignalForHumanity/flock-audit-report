use flock_audit_report::report::{to_html, to_text};
use flock_audit_report::Analyzer;

const UNCP: &[u8] = include_bytes!("fixtures/uncp_network_audit_excerpt.csv");

/// Text report with column gaps (2+ spaces) collapsed to " | ".
fn cells(text: &str) -> String {
    text.lines()
        .map(|l| {
            let mut out = String::new();
            for (i, part) in l.split("  ").filter(|p| !p.trim().is_empty()).enumerate() {
                if i > 0 {
                    out.push_str(" | ");
                }
                out.push_str(part.trim());
            }
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn run(home: &str, files: &[(&str, &[u8])], chunk: usize) -> (Analyzer, String) {
    let mut a = Analyzer::new(home);
    for (name, data) in files {
        a.begin_file(name);
        for c in data.chunks(chunk) {
            a.push(c);
        }
        a.end_file();
    }
    let text = cells(&to_text(&a.sections()));
    (a, text)
}

#[test]
fn real_network_audit_excerpt_matches_independent_count() {
    // Expected values were counted separately with Python's csv module
    // (13 NC rows includes "WakeMed Campus PD and Public Safety (NC)").
    let (a, text) = run("NC", &[("uncp.csv", UNCP)], 1 << 20);
    assert_eq!(a.unique_searches(), 60);
    assert_eq!(a.duplicates(), 0);
    assert!(
        text.contains("uncp.csv | 60 | 2026-04-24 | 2026-04-24"),
        "{text}"
    );
    assert!(text.contains("41 distinct agencies"), "{text}");
    assert!(text.contains("Drugs/Narcotics | 14"), "{text}");
    assert!(text.contains("Houston TX PD"));
    assert!(text.contains("In NC | 13 | 21.7%"), "{text}");
    assert!(text.contains("lookup - Mobile | 6"), "{text}");
    assert!(text.contains("More than 1,000 | 56"), "{text}");
    assert!(text.contains("Case number redacted | 26"), "{text}");
    assert!(text.contains("2026-04 | 60"), "{text}");
}

#[test]
fn chunk_size_does_not_change_the_report() {
    let (_, whole) = run("NC", &[("uncp.csv", UNCP)], 1 << 20);
    for chunk in [1, 7, 64, 1000] {
        let (_, t) = run("NC", &[("uncp.csv", UNCP)], chunk);
        assert_eq!(t, whole, "chunk {chunk}");
    }
}

#[test]
fn same_file_twice_counts_each_search_once() {
    let (once, t1) = run("", &[("a.csv", UNCP)], 4096);
    let (twice, t2) = run("", &[("a.csv", UNCP), ("b.csv", UNCP)], 4096);
    assert_eq!(once.unique_searches(), twice.unique_searches());
    assert_eq!(twice.duplicates(), 60);
    // Everything after the file and totals sections is identical.
    let tail = |t: &str| t[t.find("Searches per month").unwrap()..].to_string();
    assert_eq!(tail(&t1), tail(&t2));
}

const ORG_AUDIT: &str = "\
ID,Name,Org Name,Camera Count,Time Frame,License Plate,Reason,Case #,Filters,Search Time,Search Type,Text Prompt,Moderation
11111111-0000-0000-0000-000000000001,J. Smithers,Springfield NC PD,40,x,ABC1234,Other - ICE assist,,,\"05/01/2026, 01:00:00 PM UTC\",lookup,,
11111111-0000-0000-0000-000000000002,J. Smithers,Springfield NC PD,40,x,ABC-1234,inv,,,\"05/02/2026, 01:00:00 PM UTC\",lookup,,
11111111-0000-0000-0000-000000000003,K. Quimby,Springfield NC PD,40,x,XYZ999,Other - protest downtown,24-001,,\"06/02/2026, 01:00:00 PM UTC\",search,,
11111111-0000-0000-0000-000000000004,K. Quimby,[Federal] FBI,7000,x,***,Traffic Infraction - ,***,,\"06/03/2026, 01:00:00 PM UTC\",search,,
11111111-0000-0000-0000-000000000005,J. Smithers,Dallas TX PD,7000,x,ABC1234,,,,\"06/04/2026, 01:00:00 PM UTC\",lookup,,
";

#[test]
fn names_and_plates_never_appear_in_any_output() {
    let mut a = Analyzer::new("NC");
    a.push(ORG_AUDIT.as_bytes());
    let sections = a.sections();
    let html = to_html(&sections);
    let text = cells(&to_text(&sections));
    let agencies = a.agencies_csv();
    for out in [&html, &text, &agencies] {
        for secret in ["Smithers", "Quimby", "ABC1234", "ABC-1234", "XYZ999"] {
            assert!(!out.contains(secret), "{secret} leaked");
        }
    }
    assert!(text.contains("2 distinct operators named."), "{text}");
    assert!(
        text.contains("Operator 1 | Springfield NC PD | 3"),
        "{text}"
    );
    assert!(text.contains("Distinct plates searched | 2"), "{text}");
    assert!(
        text.contains("Most searches of a single plate | 3"),
        "{text}"
    );
}

#[test]
fn org_audit_classification_and_flags() {
    let mut a = Analyzer::new("north carolina");
    a.push(ORG_AUDIT.as_bytes());
    let text = cells(&to_text(&a.sections()));
    assert!(text.contains("In NC | 3 | 60.0%"), "{text}");
    assert!(
        text.contains("Other states (outside NC) | 1 | 20.0%"),
        "{text}"
    );
    assert!(text.contains("Federal agencies | 1 | 20.0%"), "{text}");
    assert!(text.contains("Reason blank or redacted"), "{text}");
    assert!(
        text.contains("Immigration enforcement | 1 | 20.0%"),
        "{text}"
    );
    assert!(
        text.contains("Protest, religion, press or politics | 1 | 20.0%"),
        "{text}"
    );
    assert!(text.contains("Other - ICE assist"), "{text}");
    assert!(text.contains("2026-05 | 2"), "{text}");
    assert!(text.contains("2026-06 | 3"), "{text}");
    let csv = a.agencies_csv();
    assert!(
        csv.starts_with("agency,state,searches\nSpringfield NC PD,NC,3\n"),
        "{csv}"
    );
    assert!(csv.contains("[Federal] FBI,Federal,1"));
}

#[test]
fn vague_and_blank_reasons_are_counted() {
    let mut a = Analyzer::new("");
    a.push(ORG_AUDIT.as_bytes());
    let text = cells(&to_text(&a.sections()));
    // "inv" is vague; the empty reason is blank.
    assert!(
        text.contains("Reason blank or redacted | 1 | 20.0%"),
        "{text}"
    );
    assert!(
        text.contains("3 characters or fewer) | 1 | 20.0%"),
        "{text}"
    );
    assert!(text.contains("Case number given | 1"), "{text}");
}

#[test]
fn transparency_portal_format() {
    let portal = "\"id\",\"userId\",\"searchDate\",\"networkCount\",\"offenseType\"\n\
\"bdabb7f0-9a5c-4893-bea7-b0035666eafa\",\"***\",\"2026-10-22T00:16:00.239Z\",\"1\",\"Weapons Offense (Guns/Shots Fired)\"\n\
\"92af063f-157e-4cd8-8da3-8f225ab55994\",\"***\",\"2026-10-22T00:00:51.294Z\",\"587\",\"Drugs/Narcotics\"\n";
    let mut a = Analyzer::new("MI");
    a.push(portal.as_bytes());
    let text = cells(&to_text(&a.sections()));
    assert_eq!(a.unique_searches(), 2);
    assert!(text.contains("do not name the searching agency"), "{text}");
    assert!(text.contains("101–1,000 | 1"), "{text}");
    assert!(!text.contains("Operators and plates"));
}

#[test]
fn unrelated_csv_is_skipped_with_a_warning() {
    let mut a = Analyzer::new("");
    a.begin_file("budget.csv");
    a.push(b"item,amount\ncameras,100000\n");
    a.end_file();
    let text = cells(&to_text(&a.sections()));
    assert!(text.contains("\"budget.csv\" does not look like a Flock search audit"));
    assert!(text.contains("No searches found"));
}

#[test]
fn excel_truncated_file_is_flagged() {
    let mut a = Analyzer::new("");
    a.begin_file("cut.csv");
    a.push(b"ID,Org Name,Reason,Search Time\n");
    let mut chunk = Vec::with_capacity(1 << 22);
    for i in 0..1_048_575u64 {
        chunk.extend_from_slice(format!("{i},Town NC PD,Robbery,2026-01-01\n").as_bytes());
        if chunk.len() > (1 << 22) - 64 {
            a.push(&chunk);
            chunk.clear();
        }
    }
    a.push(&chunk);
    a.end_file();
    let text = cells(&to_text(&a.sections()));
    assert!(text.contains("Excel's limit"), "{}", &text[..500]);
    assert_eq!(a.unique_searches(), 1_048_575);
}

#[test]
fn plates_in_free_text_reasons_are_masked() {
    let data = "ID,Org Name,Reason,Text Prompt,Search Time\n\
1,Town NC PD,Other - stolen vehicle ABC1234 ICE assist,,2026-01-01\n\
2,Town NC PD,stolen veh XYZ-1234,,2026-01-01\n\
3,Town NC PD,Other - 287g protest,,2026-01-01\n";
    let mut a = Analyzer::new("");
    a.push(data.as_bytes());
    let s = a.sections();
    let text = to_text(&s) + &to_html(&s);
    for secret in ["ABC1234", "XYZ-1234", "XYZ"] {
        assert!(!text.contains(secret), "{secret} leaked:\n{text}");
    }
    assert!(text.contains("287g"), "{text}");
}

#[test]
fn semicolon_file_in_small_chunks() {
    let (a, text) = run(
        "",
        &[("s.csv", b"ID;Org Name;Reason\n1;Town NC PD;Robbery\n")],
        1,
    );
    assert_eq!(a.unique_searches(), 1, "{text}");
    assert!(text.contains("Robbery | 1"), "{text}");
}
