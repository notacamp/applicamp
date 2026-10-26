//! The master profile: what Applicamp knows about a person's work.
//!
//! Field names follow JSON Resume where it has them (`basics`, `work`,
//! `education`, `skills`, `languages`, `certificates`, `projects`;
//! `startDate`, `studyType`, …), so tailored CVs (ADR 0007) start from it.
//! `basics.location` is one written place ("Lisbon, Portugal"), not JSON
//! Resume's address object: both the LinkedIn export and CVs give it so.
//!
//! The same types are the API's (`utoipa`), the JSON schema the `complete`
//! model answers in (`schemars`), and what is stored (`profile.profiles.data`).
//! Unknown fields are refused everywhere.

use std::sync::LazyLock;

use applicamp_kernel::{Error, Result};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Longest short value (a name, a title, a place), in characters.
pub const MAX_SHORT: usize = 300;
/// Longest long value (a summary, a description), in characters.
pub const MAX_LONG: usize = 10_000;
/// Most entries in one list.
pub const MAX_ITEMS: usize = 200;

static DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{4}(-(0[1-9]|1[0-2])(-(0[1-9]|[12]\d|3[01]))?)?$")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});

/// A whole profile.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Who they are.
    #[serde(default)]
    #[schema(required = true)]
    pub basics: Basics,
    /// Positions, newest first.
    #[serde(default)]
    #[schema(required = true)]
    pub work: Vec<Work>,
    /// Schools and degrees.
    #[serde(default)]
    #[schema(required = true)]
    pub education: Vec<Education>,
    /// Skills.
    #[serde(default)]
    #[schema(required = true)]
    pub skills: Vec<Skill>,
    /// Languages they speak.
    #[serde(default)]
    #[schema(required = true)]
    pub languages: Vec<Language>,
    /// Certifications.
    #[serde(default)]
    #[schema(required = true)]
    pub certificates: Vec<Certificate>,
    /// Projects.
    #[serde(default)]
    #[schema(required = true)]
    pub projects: Vec<Project>,
}

/// Who they are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Basics {
    /// Their name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Their headline ("Senior software engineer").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The address for applications.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Where they are, as written ("Lisbon, Portugal").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// A few sentences about them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Their own site.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Profiles elsewhere (GitHub, a portfolio).
    #[serde(default)]
    #[schema(required = true)]
    pub profiles: Vec<Link>,
}

/// A profile elsewhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Link {
    /// Where ("GitHub").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    /// The address.
    pub url: String,
}

/// A position.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Work {
    /// The company.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    /// Where.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// `YYYY`, `YYYY-MM` or `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    /// As `startDate`; none while it is their current position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
    /// What they did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Achievements, one sentence each.
    #[serde(default)]
    #[schema(required = true)]
    pub highlights: Vec<String>,
}

/// A school or degree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Education {
    /// The school.
    pub institution: String,
    /// The field ("Computer science").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    /// The degree ("Master's").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub study_type: Option<String>,
    /// `YYYY`, `YYYY-MM` or `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    /// As `startDate`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
}

/// A skill.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Skill {
    /// The skill ("PostgreSQL").
    pub name: String,
    /// How well, in their words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
}

/// A language they speak.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Language {
    /// The language ("Portuguese").
    pub language: String,
    /// How well ("Native or bilingual").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fluency: Option<String>,
}

/// A certification.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Certificate {
    /// Its name.
    pub name: String,
    /// Who issued it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    /// When, `YYYY`, `YYYY-MM` or `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Where it can be checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// A project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Project {
    /// Its name.
    pub name: String,
    /// What it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `YYYY`, `YYYY-MM` or `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    /// As `startDate`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
    /// Where to see it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// What a value is, for its limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A name, a title, a place, a URL.
    Short,
    /// A summary or a description.
    Long,
    /// A date.
    Date,
}

/// Whether `value` (trimmed, non-empty) fits `kind`.
pub(crate) fn fits(kind: Kind, value: &str) -> bool {
    match kind {
        Kind::Short => value.chars().count() <= MAX_SHORT,
        Kind::Long => value.chars().count() <= MAX_LONG,
        Kind::Date => DATE.is_match(value),
    }
}

/// `value` trimmed; `None` when that leaves nothing.
pub(crate) fn tidy(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

/// A visitor over every value of a profile, which may keep, change or drop
/// each one and every entry. Drafts (the source check, the limits) and
/// saving (tidying, validating) are written once over it.
pub(crate) trait Values {
    /// An optional value of `kind`: set it to `None` to drop it.
    fn value(&mut self, kind: Kind, value: &mut Option<String>);
    /// An entry's key value (a company, a school, a skill): `false` drops
    /// the whole entry.
    fn key(&mut self, kind: Kind, value: &mut String) -> bool;
    /// An entry with no key value left at all (a position with neither a
    /// company nor a title).
    fn empty_entry(&mut self) {}
}

impl Profile {
    /// Visit every value; entries `visitor` drops are removed.
    pub(crate) fn visit(&mut self, visitor: &mut impl Values) {
        let basics = &mut self.basics;
        visitor.value(Kind::Short, &mut basics.name);
        visitor.value(Kind::Short, &mut basics.label);
        visitor.value(Kind::Short, &mut basics.email);
        visitor.value(Kind::Short, &mut basics.location);
        visitor.value(Kind::Long, &mut basics.summary);
        visitor.value(Kind::Short, &mut basics.url);
        basics.profiles.retain_mut(|link| {
            if !visitor.key(Kind::Short, &mut link.url) {
                return false;
            }
            visitor.value(Kind::Short, &mut link.network);
            true
        });
        self.work.retain_mut(|work| {
            // A position is its company and title: either unverified drops it.
            for key in [&mut work.name, &mut work.position] {
                if let Some(value) = key.as_mut()
                    && !visitor.key(Kind::Short, value)
                {
                    return false;
                }
            }
            if work.name.is_none() && work.position.is_none() {
                visitor.empty_entry();
                return false;
            }
            visitor.value(Kind::Short, &mut work.location);
            visitor.value(Kind::Date, &mut work.start_date);
            visitor.value(Kind::Date, &mut work.end_date);
            visitor.value(Kind::Long, &mut work.summary);
            work.highlights.retain_mut(|highlight| {
                let mut value = Some(std::mem::take(highlight));
                visitor.value(Kind::Long, &mut value);
                match value {
                    Some(kept) => {
                        *highlight = kept;
                        true
                    }
                    None => false,
                }
            });
            true
        });
        self.education.retain_mut(|school| {
            if !visitor.key(Kind::Short, &mut school.institution) {
                return false;
            }
            visitor.value(Kind::Short, &mut school.area);
            visitor.value(Kind::Short, &mut school.study_type);
            visitor.value(Kind::Date, &mut school.start_date);
            visitor.value(Kind::Date, &mut school.end_date);
            true
        });
        self.skills.retain_mut(|skill| {
            if !visitor.key(Kind::Short, &mut skill.name) {
                return false;
            }
            visitor.value(Kind::Short, &mut skill.level);
            true
        });
        self.languages.retain_mut(|language| {
            if !visitor.key(Kind::Short, &mut language.language) {
                return false;
            }
            visitor.value(Kind::Short, &mut language.fluency);
            true
        });
        self.certificates.retain_mut(|certificate| {
            if !visitor.key(Kind::Short, &mut certificate.name) {
                return false;
            }
            visitor.value(Kind::Short, &mut certificate.issuer);
            visitor.value(Kind::Date, &mut certificate.date);
            visitor.value(Kind::Short, &mut certificate.url);
            true
        });
        self.projects.retain_mut(|project| {
            if !visitor.key(Kind::Short, &mut project.name) {
                return false;
            }
            visitor.value(Kind::Long, &mut project.description);
            visitor.value(Kind::Date, &mut project.start_date);
            visitor.value(Kind::Date, &mut project.end_date);
            visitor.value(Kind::Short, &mut project.url);
            true
        });
    }

    /// Nothing in it at all.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The lists, each cut to [`MAX_ITEMS`]: how many entries that dropped.
    pub(crate) fn cap_lists(&mut self) -> u32 {
        fn cap<T>(list: &mut Vec<T>) -> usize {
            let over = list.len().saturating_sub(MAX_ITEMS);
            list.truncate(MAX_ITEMS);
            over
        }
        let over = cap(&mut self.basics.profiles)
            + cap(&mut self.work)
            + cap(&mut self.education)
            + cap(&mut self.skills)
            + cap(&mut self.languages)
            + cap(&mut self.certificates)
            + cap(&mut self.projects)
            + self
                .work
                .iter_mut()
                .map(|work| cap(&mut work.highlights))
                .sum::<usize>();
        u32::try_from(over).unwrap_or(u32::MAX)
    }

    /// Every value trimmed and empty ones removed; entries with no key value
    /// left are removed too.
    #[must_use]
    pub fn tidied(mut self) -> Self {
        // A position keeps a title without a company, or the other way round.
        for work in &mut self.work {
            work.name = tidy(work.name.take());
            work.position = tidy(work.position.take());
        }
        struct Tidy;
        impl Values for Tidy {
            fn value(&mut self, _: Kind, value: &mut Option<String>) {
                *value = tidy(value.take());
            }
            fn key(&mut self, _: Kind, value: &mut String) -> bool {
                *value = value.trim().to_owned();
                !value.is_empty()
            }
        }
        self.visit(&mut Tidy);
        self
    }

    /// A draft fit to save: tidied, every value within its limits (dates in
    /// form), the lists capped. Values that don't fit are dropped and
    /// counted.
    pub(crate) fn sanitized(self) -> (Self, u32) {
        struct Limits(u32);
        impl Values for Limits {
            fn value(&mut self, kind: Kind, value: &mut Option<String>) {
                if value.as_deref().is_some_and(|v| !fits(kind, v)) {
                    *value = None;
                    self.0 += 1;
                }
            }
            fn key(&mut self, kind: Kind, value: &mut String) -> bool {
                let fits = fits(kind, value);
                if !fits {
                    self.0 += 1;
                }
                fits
            }
        }
        let mut profile = self.tidied();
        let mut limits = Limits(0);
        profile.visit(&mut limits);
        let capped = profile.cap_lists();
        (profile, limits.0.saturating_add(capped))
    }

    /// Check a profile someone saved by hand: every value within its limits
    /// and every date in form. The first problem, in English, as a 400
    /// `invalid_profile`.
    pub fn validate(&self) -> Result<()> {
        struct Check(Option<String>);
        impl Check {
            fn check(&mut self, kind: Kind, value: &str) {
                if self.0.is_none() && !fits(kind, value) {
                    self.0 = Some(match kind {
                        Kind::Short => format!("A value is longer than {MAX_SHORT} characters."),
                        Kind::Long => format!("A text is longer than {MAX_LONG} characters."),
                        Kind::Date => {
                            format!("`{value}` is not a date like 2020, 2020-03 or 2020-03-15.")
                        }
                    });
                }
            }
        }
        impl Values for Check {
            fn value(&mut self, kind: Kind, value: &mut Option<String>) {
                if let Some(value) = value.as_deref() {
                    self.check(kind, value);
                }
            }
            fn key(&mut self, kind: Kind, value: &mut String) -> bool {
                self.check(kind, value);
                true
            }
            fn empty_entry(&mut self) {
                if self.0.is_none() {
                    self.0 = Some("A position needs a company or a title.".to_owned());
                }
            }
        }
        let lists = [
            self.basics.profiles.len(),
            self.work.len(),
            self.education.len(),
            self.skills.len(),
            self.languages.len(),
            self.certificates.len(),
            self.projects.len(),
        ];
        if lists.into_iter().any(|len| len > MAX_ITEMS)
            || self.work.iter().any(|w| w.highlights.len() > MAX_ITEMS)
        {
            return Err(invalid(format!(
                "A list has more than {MAX_ITEMS} entries."
            )));
        }
        let mut check = Check(None);
        self.clone().visit(&mut check);
        check.0.map_or(Ok(()), |why| Err(invalid(why)))
    }

    /// `draft` merged into this profile: everything here stays as it is;
    /// empty fields of `basics` are filled from the draft, and positions,
    /// schools, skills, languages, certifications, projects and links the
    /// profile does not have yet are added after its own.
    #[must_use]
    pub fn merged_with(mut self, draft: Self) -> Self {
        let basics = &mut self.basics;
        let fill = |mine: &mut Option<String>, theirs: Option<String>| {
            if mine.is_none() {
                *mine = theirs;
            }
        };
        fill(&mut basics.name, draft.basics.name);
        fill(&mut basics.label, draft.basics.label);
        fill(&mut basics.email, draft.basics.email);
        fill(&mut basics.location, draft.basics.location);
        fill(&mut basics.summary, draft.basics.summary);
        fill(&mut basics.url, draft.basics.url);
        add_new(&mut basics.profiles, draft.basics.profiles, |a, b| {
            same(&a.url) == same(&b.url)
        });
        // A position is the same one when the company matches and so does
        // its start or its title: editing either by hand never brings the
        // imported one back beside it.
        add_new(&mut self.work, draft.work, |a, b| {
            same_opt(a.name.as_deref()) == same_opt(b.name.as_deref())
                && (same_opt(a.start_date.as_deref()) == same_opt(b.start_date.as_deref())
                    || same_opt(a.position.as_deref()) == same_opt(b.position.as_deref()))
        });
        add_new(&mut self.education, draft.education, |a, b| {
            same(&a.institution) == same(&b.institution)
                && same_opt(a.study_type.as_deref()) == same_opt(b.study_type.as_deref())
        });
        add_new(&mut self.skills, draft.skills, |a, b| {
            same(&a.name) == same(&b.name)
        });
        add_new(&mut self.languages, draft.languages, |a, b| {
            same(&a.language) == same(&b.language)
        });
        add_new(&mut self.certificates, draft.certificates, |a, b| {
            same(&a.name) == same(&b.name)
        });
        add_new(&mut self.projects, draft.projects, |a, b| {
            same(&a.name) == same(&b.name)
        });
        self.cap_lists();
        self
    }
}

/// A 400 `invalid_profile` saying `why`.
fn invalid(why: String) -> Error {
    Error::BadRequest("invalid_profile", why)
}

/// `value` as merging compares it: case and spacing do not matter.
fn same(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn same_opt(value: Option<&str>) -> String {
    value.map(same).unwrap_or_default()
}

/// The entries of `theirs` that are not `same_as` any of `mine`, added to it.
fn add_new<T>(mine: &mut Vec<T>, theirs: Vec<T>, same_as: impl Fn(&T, &T) -> bool) {
    for entry in theirs {
        if !mine.iter().any(|known| same_as(known, &entry)) {
            mine.push(entry);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work(company: &str, title: &str, start: &str) -> Work {
        Work {
            name: Some(company.to_owned()),
            position: Some(title.to_owned()),
            start_date: Some(start.to_owned()),
            ..Work::default()
        }
    }

    fn skill(name: &str) -> Skill {
        Skill {
            name: name.to_owned(),
            level: None,
        }
    }

    #[test]
    fn merging_keeps_what_is_there_and_adds_what_is_new() {
        let mine = Profile {
            basics: Basics {
                label: Some("Edited by hand".into()),
                ..Basics::default()
            },
            work: vec![work("Example Org", "Engineer (edited)", "2020-01")],
            skills: vec![skill("Rust")],
            ..Profile::default()
        };
        let draft = Profile {
            basics: Basics {
                name: Some("Alex Demo".into()),
                label: Some("Imported headline".into()),
                ..Basics::default()
            },
            work: vec![
                work("Example Org", "Engineer (edited)", "2020-01"),
                work("Sample Labs", "Intern", "2018-06"),
            ],
            skills: vec![skill("rust"), skill("SQL")],
            ..Profile::default()
        };
        let merged = mine.merged_with(draft);
        assert_eq!(merged.basics.label.as_deref(), Some("Edited by hand"));
        assert_eq!(merged.basics.name.as_deref(), Some("Alex Demo"));
        assert_eq!(merged.work.len(), 2);
        assert_eq!(merged.work[1].name.as_deref(), Some("Sample Labs"));
        let skills: Vec<_> = merged.skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(skills, ["Rust", "SQL"]);
    }

    #[test]
    fn tidying_trims_and_drops_empty_values_and_entries() {
        let profile = Profile {
            basics: Basics {
                name: Some("  Alex Demo ".into()),
                label: Some("   ".into()),
                ..Basics::default()
            },
            work: vec![Work {
                name: Some(" ".into()),
                highlights: vec![" ".into(), "Shipped it".into()],
                ..Work::default()
            }],
            skills: vec![skill(" "), skill("Go ")],
            ..Profile::default()
        }
        .tidied();
        assert_eq!(profile.basics.name.as_deref(), Some("Alex Demo"));
        assert_eq!(profile.basics.label, None);
        assert!(profile.work.is_empty(), "no company and no title");
        assert_eq!(profile.skills, [skill("Go")]);
    }

    #[test]
    fn validation_names_the_first_problem() {
        let ok = Profile {
            work: vec![work("Example Org", "Engineer", "2020-03")],
            ..Profile::default()
        };
        assert!(ok.validate().is_ok());
        for (bad, says) in [
            (
                Profile {
                    work: vec![work("Example Org", "Engineer", "March 2020")],
                    ..Profile::default()
                },
                "not a date",
            ),
            (
                Profile {
                    skills: vec![skill(&"x".repeat(MAX_SHORT + 1))],
                    ..Profile::default()
                },
                "longer than",
            ),
            (
                Profile {
                    skills: vec![skill("x"); MAX_ITEMS + 1],
                    ..Profile::default()
                },
                "more than",
            ),
            (
                Profile {
                    work: vec![Work::default()],
                    ..Profile::default()
                },
                "company or a title",
            ),
        ] {
            let err = bad.validate().unwrap_err();
            assert_eq!(err.code(), "invalid_profile");
            assert!(err.to_problem().detail.contains(says), "{err:?}");
        }
    }

    #[test]
    fn sanitizing_drops_and_counts_what_does_not_fit() {
        let (profile, dropped) = Profile {
            work: vec![
                Work {
                    end_date: Some("until now".into()),
                    ..work("Example Org", "Engineer", "2020")
                },
                work(&"x".repeat(MAX_SHORT + 1), "Engineer", "2019"),
            ],
            skills: vec![skill("x"); MAX_ITEMS + 2],
            ..Profile::default()
        }
        .sanitized();
        assert_eq!(profile.work.len(), 1);
        assert_eq!(profile.work[0].end_date, None);
        assert_eq!(profile.skills.len(), MAX_ITEMS);
        assert_eq!(dropped, 1 + 1 + 2);
    }

    #[test]
    fn dates_are_years_months_or_days() {
        for good in ["2020", "2020-01", "2020-12-31"] {
            assert!(fits(Kind::Date, good), "{good}");
        }
        for bad in ["20", "2020-13", "2020-1", "Jan 2020", "2020-02-32"] {
            assert!(!fits(Kind::Date, bad), "{bad}");
        }
    }

    #[test]
    fn unknown_fields_are_refused() {
        let err = serde_json::from_value::<Profile>(serde_json::json!({
            "basics": { "name": "Alex", "birthday": "1990-01-01" }
        }))
        .unwrap_err();
        assert!(err.to_string().contains("birthday"), "{err}");
        let empty: Profile = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(empty.is_empty());
    }
}
