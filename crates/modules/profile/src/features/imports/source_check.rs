//! The source check: a model-structured draft keeps only values that appear
//! in the text it was made from. Anything else is dropped and counted,
//! never shown as if the person had written it.
//!
//! Text is compared without case and without any whitespace (a PDF's text
//! comes with spacing of its own), with typographic quotes and dashes made
//! plain. A date passes when the source writes it in one of its common
//! forms: `2020-03`, `03/2020`, `3.2020`, `Mar 2020`, `março de 2020`,
//! `mars 2020`… in English, Portuguese, Spanish or French.

use crate::schema::{Kind, Profile, Values};

/// Month names (full, then abbreviations) per month, in every language the
/// app speaks. Lower case.
const MONTHS: [&[&str]; 12] = [
    &[
        "january", "janeiro", "enero", "janvier", "jan", "ene", "janv",
    ],
    &[
        "february",
        "fevereiro",
        "febrero",
        "février",
        "fevrier",
        "feb",
        "fev",
        "fév",
        "févr",
        "fevr",
    ],
    &["march", "março", "marco", "marzo", "mars", "mar"],
    &["april", "abril", "avril", "apr", "abr", "avr"],
    &["may", "maio", "mayo", "mai"],
    &["june", "junho", "junio", "juin", "jun"],
    &["july", "julho", "julio", "juillet", "jul", "juil"],
    &["august", "agosto", "août", "aout", "aug", "ago"],
    &[
        "september",
        "setembro",
        "septiembre",
        "setiembre",
        "septembre",
        "sep",
        "sept",
        "set",
    ],
    &["october", "outubro", "octubre", "octobre", "oct", "out"],
    &["november", "novembro", "noviembre", "novembre", "nov"],
    &[
        "december",
        "dezembro",
        "diciembre",
        "décembre",
        "decembre",
        "dec",
        "dez",
        "dic",
        "déc",
    ],
];

/// `text` as the check compares it: lower case, no whitespace, plain quotes
/// and dashes.
pub(crate) fn squash(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '\u{200b}' | '\u{00ad}' | '\u{feff}'))
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{201b}' | '\u{2032}' => '\'',
            '\u{201c}' | '\u{201d}' | '\u{201f}' | '\u{2033}' => '"',
            '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
            other => other,
        })
        .flat_map(char::to_lowercase)
        .collect()
}

/// The ways a source may write the date `value` (`YYYY`, `YYYY-MM` or
/// `YYYY-MM-DD`), squashed.
fn written_forms(value: &str) -> Vec<String> {
    let mut parts = value.split('-');
    let (Some(year), month) = (parts.next(), parts.next()) else {
        return Vec::new();
    };
    let Some(month) = month else {
        return vec![year.to_owned()];
    };
    let mut forms = vec![
        format!("{year}-{month}"),
        format!("{year}/{month}"),
        format!("{year}.{month}"),
        format!("{month}/{year}"),
        format!("{month}-{year}"),
        format!("{month}.{year}"),
    ];
    let Ok(number) = month.parse::<usize>() else {
        return forms;
    };
    let short = number.to_string();
    if short != month {
        forms.extend([
            format!("{short}/{year}"),
            format!("{short}-{year}"),
            format!("{short}.{year}"),
        ]);
    }
    if let Some(names) = number.checked_sub(1).and_then(|i| MONTHS.get(i)) {
        for name in *names {
            forms.extend([
                format!("{name}{year}"),
                format!("{name}.{year}"),
                format!("{name},{year}"),
                format!("{name}de{year}"),
                format!("{name}.de{year}"),
            ]);
        }
    }
    forms
}

/// What the check compares against: the source, squashed.
pub(crate) struct Source {
    squashed: String,
    /// Values dropped so far.
    pub(crate) dropped: u32,
}

impl Source {
    pub(crate) fn new(text: &str) -> Self {
        Self {
            squashed: squash(text),
            dropped: 0,
        }
    }

    /// Whether the source says `value`, a value of `kind`.
    pub(crate) fn says(&self, kind: Kind, value: &str) -> bool {
        let value = squash(value);
        if value.is_empty() {
            return true;
        }
        if self.squashed.contains(&value) {
            return true;
        }
        kind == Kind::Date
            && written_forms(&value)
                .iter()
                .any(|form| self.squashed.contains(form.as_str()))
    }
}

impl Values for Source {
    fn value(&mut self, kind: Kind, value: &mut Option<String>) {
        if value.as_deref().is_some_and(|v| !self.says(kind, v)) {
            *value = None;
            self.dropped += 1;
        }
    }

    fn key(&mut self, kind: Kind, value: &mut String) -> bool {
        let says = self.says(kind, value);
        if !says {
            self.dropped += 1;
        }
        says
    }
}

/// `draft` with only what `source` says: what was dropped, counted.
pub(crate) fn keep_sourced(mut draft: Profile, source: &str) -> (Profile, u32) {
    let mut check = Source::new(source);
    draft.visit(&mut check);
    (draft, check.dropped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Basics, Skill, Work};

    const CV: &str = "ALEX DEMO\n\
        Senior   Software Engineer — Lisbon\n\
        Experience\n\
        Example Org · Staff Engineer · Mar 2021 – present\n\
        Built the billing platform.\n\
        Sample Labs, Engineer, 03/2018 to Fevereiro de 2021\n\
        Skills: Rust, PostgreSQL";

    #[test]
    fn values_in_the_text_are_kept() {
        let source = Source::new(CV);
        for value in [
            "Alex Demo",
            "senior software engineer",
            "Example Org",
            "Built the billing platform.",
            "Sample Labs",
        ] {
            assert!(source.says(Kind::Short, value), "{value}");
        }
        for date in ["2021-03", "2018-03", "2021-02", "2021", "2018-03-01"] {
            assert!(source.says(Kind::Date, date), "{date}");
        }
    }

    #[test]
    fn values_not_in_the_text_are_not() {
        let source = Source::new(CV);
        assert!(!source.says(Kind::Short, "Invented Corp"));
        assert!(!source.says(Kind::Short, "Kubernetes"));
        assert!(!source.says(Kind::Date, "2019-07"));
        assert!(!source.says(Kind::Date, "2015"));
    }

    #[test]
    fn an_invented_employer_drops_its_position_and_is_counted() {
        let draft = Profile {
            basics: Basics {
                name: Some("Alex Demo".into()),
                label: Some("Chief Executive Officer".into()),
                ..Basics::default()
            },
            work: vec![
                Work {
                    name: Some("Example Org".into()),
                    position: Some("Staff Engineer".into()),
                    start_date: Some("2021-03".into()),
                    summary: Some("Built the billing platform.".into()),
                    ..Work::default()
                },
                Work {
                    name: Some("Invented Corp".into()),
                    position: Some("Staff Engineer".into()),
                    start_date: Some("2016-01".into()),
                    ..Work::default()
                },
            ],
            skills: vec![
                Skill {
                    name: "Rust".into(),
                    level: None,
                },
                Skill {
                    name: "Kubernetes".into(),
                    level: None,
                },
            ],
            ..Profile::default()
        };
        let (kept, dropped) = keep_sourced(draft, CV);
        assert_eq!(kept.basics.label, None, "not their headline");
        assert_eq!(kept.work.len(), 1);
        assert_eq!(kept.work[0].name.as_deref(), Some("Example Org"));
        assert_eq!(kept.work[0].start_date.as_deref(), Some("2021-03"));
        assert_eq!(kept.skills.len(), 1);
        // The headline, the invented position, Kubernetes.
        assert_eq!(dropped, 3);
    }

    #[test]
    fn squashing_ignores_case_spacing_and_typography() {
        assert_eq!(squash(" Rock’n  Roll\u{2013}Co \n"), "rock'nroll-co");
    }
}
