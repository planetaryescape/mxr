//! Finds the dates a detector could read in an email's text. Shared by the
//! seed-clock checks of the curated set and of the bulk families.

pub(super) const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Every date written out in `text`, each as a short canonical token: an
/// ISO date ("2026-10-29"), a current-era year ("2026"), a day and month
/// ("3 November", also from "1st of December") or a month and day
/// ("October 14"). A bare month ("tenant news for October") is not a
/// date a detector can expire; nor are a copyright year or a year inside
/// a URL path.
pub(super) fn written_dates(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let take = |at: usize, len: usize| -> String {
        chars[at..(at + len).min(chars.len())].iter().collect()
    };
    let mut found = Vec::new();
    for at in 0..chars.len() {
        let before = at.checked_sub(1).map(|i| chars[i]);
        if before.is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        if chars[at].is_ascii_digit() {
            let digits: String = chars[at..]
                .iter()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if digits.len() == 4
                && digits
                    .parse::<u32>()
                    .is_ok_and(|year| (2024..=2040).contains(&year))
                && before != Some('/')
                && !(before == Some(' ') && at >= 2 && chars[at - 2] == '©')
            {
                let iso = take(at, 10);
                let is_iso = iso.len() == 10
                    && iso.char_indices().all(|(i, c)| match i {
                        4 | 7 => c == '-',
                        _ => c.is_ascii_digit(),
                    });
                found.push(if is_iso { iso } else { digits });
            } else if digits.len() <= 2 {
                let mut rest: String = chars[at + digits.len()..].iter().take(16).collect();
                for suffix in ["st", "nd", "rd", "th"] {
                    if let Some(stripped) = rest.strip_prefix(suffix) {
                        rest = stripped.to_string();
                        break;
                    }
                }
                let rest = rest.trim_start_matches(" of").trim_start();
                if let Some(month) = MONTHS.iter().find(|m| rest.starts_with(**m)) {
                    found.push(format!("{} {month}", digits.trim_start_matches('0')));
                }
            }
        } else if before.is_none_or(|c| !c.is_alphabetic()) {
            for month in MONTHS {
                if take(at, month.len()) == month && chars.get(at + month.len()) == Some(&' ') {
                    let day: String = chars[at + month.len() + 1..]
                        .iter()
                        .take_while(|c| c.is_ascii_digit())
                        .collect();
                    if !day.is_empty() {
                        found.push(format!("{month} {day}"));
                    }
                }
            }
        }
    }
    found
}
