//! Filename timestamp patterns, as data. A pattern is a template where `{Y}` is 4 digits,
//! `{M}` `{D}` `{h}` `{m}` `{s}` are 2 digits and every other char is a literal (ASCII
//! case-insensitive). A template may match anywhere in the file stem. Unmatched names
//! (e.g. `Screenshot (123).png`) fall back to created time, then mtime.

/// Patterns tried in order; the first valid match wins. Times are local wall-clock time.
pub const PATTERNS: &[&str] = &[
    // Windows Snipping Tool / Win+Shift+S, en and vi locales:
    // "Screenshot 2026-11-23 095012", "Ảnh chụp màn hình 2026-11-23 095012"
    "{Y}-{M}-{D} {h}{m}{s}",
    // macOS: "Screenshot 2026-11-23 at 09.50.12", "Screen Shot 2026-11-23 at 9.50.12" (2-digit hour only)
    "{Y}-{M}-{D} at {h}.{m}.{s}",
    // Android / many apps: "Screenshot_20261123_095012", "IMG_20261123_095012"
    "{Y}{M}{D}_{h}{m}{s}",
    // "Screenshot_2026-11-23-09-50-12"
    "{Y}-{M}-{D}-{h}-{m}-{s}",
    // Snipaste: "Snipaste_2026-11-23_09-50-12"
    "{Y}-{M}-{D}_{h}-{m}-{s}",
    // ShareX: "chrome_2026-11-23_09-50-12" is covered above; "2026-11-23 09_50_12"
    "{Y}-{M}-{D} {h}_{m}_{s}",
    // ISO-ish: "2026-11-23T09-50-12", "2026-11-23 09.50.12"
    "{Y}-{M}-{D}T{h}-{m}-{s}",
    "{Y}-{M}-{D} {h}.{m}.{s}",
    // Date only (midnight local): "Screenshot 2026-11-23"
    "{Y}-{M}-{D}",
];

/// A local (naive) wall-clock timestamp parsed from a file name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NaiveLocal {
    pub year: u32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl NaiveLocal {
    /// `YYYY-MM-DD HH:MM:SS`, the form SQLite date functions accept.
    pub fn to_sqlite(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    fn is_valid(&self) -> bool {
        let leap = (self.year % 4 == 0 && self.year % 100 != 0) || self.year % 400 == 0;
        let days = match self.month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return false,
        };
        (1990..=2100).contains(&self.year)
            && (1..=days).contains(&self.day)
            && self.hour < 24
            && self.minute < 60
            && self.second < 60
    }
}

/// Timestamp from a file name (extension ignored), or `None`.
pub fn parse_file_name(file_name: &str) -> Option<NaiveLocal> {
    let stem = match file_name.rfind('.') {
        Some(i) if i > 0 => &file_name[..i],
        _ => file_name,
    };
    let chars: Vec<char> = stem.chars().collect();
    PATTERNS.iter().find_map(|pat| {
        let pat: Vec<char> = pat.chars().collect();
        (0..chars.len())
            // A match may not start inside a digit run ("12026-11-23" holds no year 2026).
            .filter(|&start| start == 0 || !chars[start - 1].is_ascii_digit())
            .find_map(|start| match_at(&chars[start..], &pat).filter(NaiveLocal::is_valid))
    })
}

fn match_at(text: &[char], pat: &[char]) -> Option<NaiveLocal> {
    let mut t = NaiveLocal { year: 0, month: 0, day: 0, hour: 0, minute: 0, second: 0 };
    let (mut ti, mut pi) = (0, 0);
    while pi < pat.len() {
        if pat[pi] == '{' {
            let field = *pat.get(pi + 1)?;
            let width = if field == 'Y' { 4 } else { 2 };
            let digits = text.get(ti..ti + width)?;
            if !digits.iter().all(char::is_ascii_digit) {
                return None;
            }
            let n: u32 = digits.iter().collect::<String>().parse().ok()?;
            match field {
                'Y' => t.year = n,
                'M' => t.month = n,
                'D' => t.day = n,
                'h' => t.hour = n,
                'm' => t.minute = n,
                's' => t.second = n,
                _ => return None,
            }
            ti += width;
            pi += 3;
        } else {
            let c = *text.get(ti)?;
            if !c.eq_ignore_ascii_case(&pat[pi]) {
                return None;
            }
            ti += 1;
            pi += 1;
        }
    }
    // A trailing digit right after the match means a longer number, not this pattern.
    if text.get(ti).is_some_and(char::is_ascii_digit) {
        return None;
    }
    Some(t)
}
