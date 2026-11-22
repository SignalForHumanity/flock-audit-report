//! Summarize Flock ALPR search-audit CSVs (Organization Audit, Network Audit
//! and transparency-portal audits) obtained through public-records requests.
//!
//! The logic is plain Rust ([`Analyzer`]); the `#[wasm_bindgen]` layer below
//! only passes bytes and strings to and from the web page.

pub mod analyze;
pub mod classify;
pub mod csv;
pub mod report;

pub use analyze::Analyzer;

use wasm_bindgen::prelude::*;

/// One analysis run in the browser: feed files chunk by chunk, then ask for
/// the report.
#[wasm_bindgen]
pub struct Session {
    inner: Analyzer,
}

#[wasm_bindgen]
impl Session {
    #[wasm_bindgen(constructor)]
    pub fn new(home_state: &str) -> Session {
        Session {
            inner: Analyzer::new(home_state),
        }
    }

    pub fn begin_file(&mut self, name: &str) {
        self.inner.begin_file(name);
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.inner.push(bytes);
    }

    pub fn end_file(&mut self) {
        self.inner.end_file();
    }

    pub fn report_html(&mut self) -> String {
        report::to_html(&self.inner.sections())
    }

    pub fn report_text(&mut self) -> String {
        report::to_text(&self.inner.sections())
    }

    pub fn agencies_csv(&mut self) -> String {
        self.inner.agencies_csv()
    }
}
