//! A small streaming CSV reader. Bytes arrive in arbitrary chunks (the page
//! reads files in slices), so the parser keeps its state between calls.
//! Handles quoted fields with embedded delimiters, quotes and newlines, CRLF
//! line endings, a UTF-8 byte-order mark, and comma, semicolon or tab
//! delimiters (detected from the header line).

use std::borrow::Cow;

/// One parsed record. Fields are stored in one buffer to avoid an
/// allocation per field.
#[derive(Default)]
pub struct Record {
    buf: Vec<u8>,
    ends: Vec<usize>,
}

impl Record {
    pub fn len(&self) -> usize {
        self.ends.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }

    /// Field `i`, or "" when the record is shorter.
    pub fn get(&self, i: usize) -> Cow<'_, str> {
        if i >= self.ends.len() {
            return Cow::Borrowed("");
        }
        let start = if i == 0 { 0 } else { self.ends[i - 1] };
        String::from_utf8_lossy(&self.buf[start..self.ends[i]])
    }

    fn is_blank_line(&self) -> bool {
        self.ends.len() == 1 && self.buf.is_empty()
    }

    fn clear(&mut self) {
        self.buf.clear();
        self.ends.clear();
    }
}

#[derive(Default)]
pub struct CsvStream {
    record: Record,
    delimiter: Option<u8>,
    in_quotes: bool,
    after_quote: bool,
    field_started: bool,
    started: bool,
    bom_checked: usize,
    /// Bytes held back until the whole header line has arrived, so the
    /// delimiter is detected from the full line, not a partial chunk.
    pending: Vec<u8>,
}

impl CsvStream {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a chunk; `on_record` is called for every complete record.
    pub fn push(&mut self, mut bytes: &[u8], on_record: &mut dyn FnMut(&Record)) {
        // Strip a UTF-8 BOM, even if it is split across chunks.
        const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
        while self.bom_checked < 3 && !bytes.is_empty() {
            if bytes[0] == BOM[self.bom_checked] {
                bytes = &bytes[1..];
                self.bom_checked += 1;
            } else {
                // Not a BOM; anything already consumed was a real prefix,
                // which cannot happen for valid CSV text, so ignore.
                self.bom_checked = 3;
            }
        }
        if self.delimiter.is_none() {
            self.pending.extend_from_slice(bytes);
            if !self.pending.contains(&b'\n') {
                return;
            }
            let held = std::mem::take(&mut self.pending);
            self.delimiter = Some(detect_delimiter(&held));
            self.feed(&held, on_record);
        } else {
            self.feed(bytes, on_record);
        }
    }

    fn feed(&mut self, bytes: &[u8], on_record: &mut dyn FnMut(&Record)) {
        let delim = self.delimiter.unwrap_or(b',');
        for &b in bytes {
            self.started = true;
            if self.in_quotes {
                if b == b'"' {
                    self.in_quotes = false;
                    self.after_quote = true;
                } else {
                    self.record.buf.push(b);
                }
                continue;
            }
            if self.after_quote {
                self.after_quote = false;
                if b == b'"' {
                    self.record.buf.push(b'"');
                    self.in_quotes = true;
                    continue;
                }
            }
            if b == delim {
                self.end_field();
            } else if b == b'\n' {
                self.end_field();
                self.emit(on_record);
            } else if b == b'\r' {
                // Ignored outside quotes (CRLF line endings).
            } else if b == b'"' && !self.field_started {
                self.in_quotes = true;
                self.field_started = true;
            } else {
                self.field_started = true;
                self.record.buf.push(b);
            }
        }
    }

    /// Flush the last record when the file does not end with a newline.
    pub fn finish(&mut self, on_record: &mut dyn FnMut(&Record)) {
        if self.delimiter.is_none() && !self.pending.is_empty() {
            let held = std::mem::take(&mut self.pending);
            self.delimiter = Some(detect_delimiter(&held));
            self.feed(&held, on_record);
        }
        if self.started && (self.field_started || !self.record.ends.is_empty()) {
            self.end_field();
            self.emit(on_record);
        }
        *self = Self::default();
    }

    fn end_field(&mut self) {
        self.record.ends.push(self.record.buf.len());
        self.field_started = false;
    }

    fn emit(&mut self, on_record: &mut dyn FnMut(&Record)) {
        if !self.record.is_blank_line() {
            on_record(&self.record);
        }
        self.record.clear();
        self.field_started = false;
    }
}

fn detect_delimiter(bytes: &[u8]) -> u8 {
    let line_end = bytes
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(bytes.len());
    let line = &bytes[..line_end];
    let count = |c: u8| line.iter().filter(|&&b| b == c).count();
    let (commas, semis, tabs) = (count(b','), count(b';'), count(b'\t'));
    if tabs > commas && tabs > semis {
        b'\t'
    } else if semis > commas {
        b';'
    } else {
        b','
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_chunks(chunks: &[&[u8]]) -> Vec<Vec<String>> {
        let mut out = Vec::new();
        let mut s = CsvStream::new();
        let mut cb = |r: &Record| out.push((0..r.len()).map(|i| r.get(i).into_owned()).collect());
        for c in chunks {
            s.push(c, &mut cb);
        }
        s.finish(&mut cb);
        out
    }

    #[test]
    fn quoted_newlines_and_escaped_quotes() {
        let data = b"a,b,c\r\n1,\"x\ny\",\"say \"\"hi\"\"\"\r\n2,,3";
        let rows = parse_chunks(&[data]);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1], vec!["1", "x\ny", "say \"hi\""]);
        assert_eq!(rows[2], vec!["2", "", "3"]);
    }

    #[test]
    fn chunk_boundaries_do_not_matter() {
        let data: &[u8] = b"\xEF\xBB\xBFid,r\n1,\"a,\"\"b\"\"\nc\"\n2,d\n";
        let whole = parse_chunks(&[data]);
        for split in 0..data.len() {
            let (a, b) = data.split_at(split);
            assert_eq!(parse_chunks(&[a, b]), whole, "split at {split}");
        }
        assert_eq!(whole[0], vec!["id", "r"]);
        assert_eq!(whole[1], vec!["1", "a,\"b\"\nc"]);
    }

    #[test]
    fn semicolon_delimiter_and_blank_lines() {
        let rows = parse_chunks(&[b"a;b\n\n1;2\n"]);
        assert_eq!(rows, vec![vec!["a", "b"], vec!["1", "2"]]);
    }

    #[test]
    fn missing_field_reads_empty() {
        let mut s = CsvStream::new();
        let mut got = String::from("unset");
        s.push(b"x\n", &mut |r| got = r.get(5).into_owned());
        assert_eq!(got, "");
    }
}
