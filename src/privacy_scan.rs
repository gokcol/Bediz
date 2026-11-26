//! On-device privacy suggestions. Only the categories and image bounds reach the editor.
use eframe::egui::Rect;
use regex::Regex;
use std::{ops::Range, sync::LazyLock};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    Face,
    Email,
    Phone,
    Card,
    PrivateField,
}

impl Kind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Face => "Face",
            Self::Email => "Email address",
            Self::Phone => "Phone number",
            Self::Card => "Payment card",
            Self::PrivateField => "Private field",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Finding {
    pub(crate) kind: Kind,
    // Top-left origin, normalized to the image that was scanned.
    pub(crate) bounds: Rect,
}

static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b[\p{L}0-9_.%+\-]+@[\p{L}0-9\-]+(?:\.[\p{L}0-9\-]+)*\.[\p{L}]{2,}\b").unwrap()
});
static NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\+\s*)?(?:\([0-9]{2,4}\)|[0-9])(?:[ .\t()\-]*[0-9]){7,24}").unwrap()
});
static PHONE_LABEL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:phone|mobile|tel|telefon|gsm)\.?\s*:?\s*$").unwrap());
static FIELD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:password|passcode|api[ _-]?key|client[ _-]?secret|access[ _-]?token|secret[ _-]?key|social security(?: number)?|ssn|tax id|passport(?: number)?|account (?:number|no\.?|id)|iban|full name|date of birth|home address|şifre|parola|t\.?c\.? kimlik(?: numarası)?|vergi numarası|hesap numarası)\s*[:=]\s*(?P<value>\S.*)",
    )
    .unwrap()
});

pub(crate) fn sensitive_ranges(line: &str) -> Vec<(Kind, Range<usize>)> {
    let mut ranges = Vec::new();
    for capture in FIELD.captures_iter(line) {
        if let Some(value) = capture.name("value") {
            ranges.push((
                Kind::PrivateField,
                value.start()..value.start() + value.as_str().trim_end().len(),
            ));
        }
    }
    for email in EMAIL.find_iter(line) {
        if !ranges
            .iter()
            .any(|(_, range)| range.contains(&email.start()))
        {
            ranges.push((Kind::Email, email.range()));
        }
    }
    for number in NUMBER.find_iter(line) {
        let value = number.as_str();
        let at = number.range();
        let embedded = line[..at.start]
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_alphanumeric() || ch == '_')
            || line[at.end..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
        if embedded
            || ranges
                .iter()
                .any(|(_, range)| at.start < range.end && range.start < at.end)
        {
            continue;
        }
        let digits: Vec<_> = value
            .bytes()
            .filter(u8::is_ascii_digit)
            .map(|b| b - b'0')
            .collect();
        if digits.iter().all(|digit| *digit == digits[0]) {
            continue;
        }
        if (13..=19).contains(&digits.len()) && luhn_valid(&digits) {
            ranges.push((Kind::Card, at));
        } else if (10..=15).contains(&digits.len())
            && value.parse::<std::net::Ipv4Addr>().is_err()
            && (value.starts_with('+')
                || value.contains('(')
                || PHONE_LABEL.is_match(&line[..at.start])
                || value
                    .bytes()
                    .filter(|byte| matches!(byte, b' ' | b'-' | b'.'))
                    .count()
                    >= 2)
        {
            ranges.push((Kind::Phone, at));
        }
    }
    ranges.sort_unstable_by_key(|(_, range)| range.start);
    ranges
}

fn luhn_valid(digits: &[u8]) -> bool {
    digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, digit)| {
            let double = 2 * digit;
            u32::from(if i % 2 == 0 {
                *digit
            } else if double > 9 {
                double - 9
            } else {
                double
            })
        })
        .sum::<u32>()
        .is_multiple_of(10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_only_values_in_screen_text_including_unicode_prefixes() {
        let line = "İletişim: someone@example.com  |  Tel +90 (555) 010 23 45  |  Kart 4111 1111 1111 1111";
        let results: Vec<_> = sensitive_ranges(line)
            .into_iter()
            .map(|(kind, span)| (kind, &line[span]))
            .collect();
        assert_eq!(
            results,
            vec![
                (Kind::Email, "someone@example.com"),
                (Kind::Phone, "+90 (555) 010 23 45"),
                (Kind::Card, "4111 1111 1111 1111")
            ]
        );
        let line = "Hesap numarası: ÖRNEK-123 ";
        let found = sensitive_ranges(line);
        assert_eq!(found.len(), 1);
        assert_eq!(
            (found[0].0, &line[found[0].1.clone()]),
            (Kind::PrivateField, "ÖRNEK-123")
        );
        assert!(sensitive_ranges("Date 2026-11-26, ticket XY1234567890123456.").is_empty());
        assert!(sensitive_ranges("Address 192.168.100.200").is_empty());
        assert_eq!(sensitive_ranges("Tel: 5550102345")[0].0, Kind::Phone);
    }
}
