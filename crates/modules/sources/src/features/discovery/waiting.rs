//! Why a candidate still waits for the owner, in one plain sentence (#108),
//! kept in its report so the admin page can show it. The sentences never say
//! "RSS", "robots" or "API": they speak of a list of jobs and of the site's
//! rules.

use chrono::{DateTime, Utc};

use super::{ALIVE_DAYS, MIN_POSTINGS};

/// What a check knows about a candidate it did not approve.
#[derive(Debug, Default)]
pub struct Facts<'a> {
    /// Its list of jobs needs a key that is not set.
    pub needs_key: bool,
    /// The site's rules do not let apps read its list of jobs.
    pub rules_forbid_list: bool,
    /// A list of jobs was found, and how many jobs could be read from it.
    pub found: bool,
    pub sample: usize,
    /// How many of them look like postings.
    pub postings: usize,
    /// The newest posting date in the sample.
    pub newest: Option<DateTime<Utc>>,
    /// The check used up its requests before it found the list.
    pub out_of_requests: bool,
    pub requests: usize,
    /// The name of the company every job applies at, when it is one.
    pub one_company: Option<&'a str>,
    /// The terms' verdict, and whether it is of the terms as they are now.
    pub verdict: Option<&'a str>,
    pub terms_fresh: bool,
    pub budget_reached: bool,
    /// The owner changed its settings.
    pub edited: bool,
    /// A config was found that the owner has to use.
    pub proposal_waits: bool,
    pub now: Option<DateTime<Utc>>,
}

/// The sentence for the first reason that applies.
pub fn because(f: &Facts<'_>) -> String {
    if f.needs_key {
        return "Its list of jobs needs a key from the site: set one, then check again.".to_owned();
    }
    if f.rules_forbid_list {
        return "The site's rules do not let apps read its list of jobs.".to_owned();
    }
    if !f.found {
        return if f.out_of_requests {
            format!(
                "The check used its {} requests before it found where the jobs are.",
                f.requests
            )
        } else {
            "No list of jobs was found on the site.".to_owned()
        };
    }
    if f.sample == 0 {
        return "A list was found, but no job could be read from it.".to_owned();
    }
    if let Some(company) = f.one_company {
        return format!(
            "Every job on it applies on one company's hiring page ({company}), so it is one company's page, not a job site; that page is added as a company page."
        );
    }
    if f.sample < MIN_POSTINGS {
        return format!(
            "Only {} job{} found; at least {MIN_POSTINGS} are needed to tell a job site from a blog.",
            f.sample,
            if f.sample == 1 { " was" } else { "s were" }
        );
    }
    if f.postings < MIN_POSTINGS {
        return format!(
            "Only {} of {} items look like jobs (a title with a company, an apply link or a page of its own); at least {MIN_POSTINGS} are needed.",
            f.postings, f.sample
        );
    }
    let since = f.now.map(|now| now - chrono::TimeDelta::days(ALIVE_DAYS));
    if since.is_some_and(|since| f.newest.is_none_or(|at| at < since)) {
        return match f.newest {
            Some(at) => format!(
                "Its newest job is from {}, more than {ALIVE_DAYS} days ago.",
                at.format("%Y-%m-%d")
            ),
            None => format!(
                "None of its jobs says when it was posted, and the site shows no change in the last {ALIVE_DAYS} days."
            ),
        };
    }
    match f.verdict {
        Some("forbid") => return "Its terms do not allow apps to read it.".to_owned(),
        Some("ask_first") => return "Its terms ask for permission first.".to_owned(),
        Some("personal_only") => {
            return "Its terms allow personal use only, not an app.".to_owned();
        }
        _ => {}
    }
    if f.budget_reached {
        return "The daily AI limit was reached before its terms were read.".to_owned();
    }
    if !f.terms_fresh {
        return "Its terms have not been read yet.".to_owned();
    }
    if f.edited {
        return "You changed its settings, so it waits for you.".to_owned();
    }
    if f.proposal_waits {
        return "The app found how to read it; use that config to approve it.".to_owned();
    }
    "It waits for your approval.".to_owned()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 5, 8, 0, 0).unwrap()
    }

    fn clean() -> Facts<'static> {
        Facts {
            found: true,
            sample: 8,
            postings: 8,
            newest: Some(now() - chrono::TimeDelta::days(2)),
            verdict: Some("silent"),
            terms_fresh: true,
            now: Some(now()),
            ..Facts::default()
        }
    }

    #[test]
    fn each_reason_is_one_plain_sentence() {
        let says = |facts: Facts<'_>| because(&facts);
        assert!(
            says(Facts {
                found: false,
                ..clean()
            })
            .starts_with("No list of jobs")
        );
        assert!(
            says(Facts {
                found: false,
                out_of_requests: true,
                requests: 25,
                ..clean()
            })
            .contains("25 requests")
        );
        assert!(
            says(Facts {
                sample: 0,
                ..clean()
            })
            .contains("no job could be read")
        );
        assert!(
            says(Facts {
                sample: 3,
                postings: 3,
                ..clean()
            })
            .starts_with("Only 3 jobs were found")
        );
        assert!(
            says(Facts {
                sample: 1,
                postings: 1,
                ..clean()
            })
            .starts_with("Only 1 job was found")
        );
        assert!(
            says(Facts {
                postings: 2,
                ..clean()
            })
            .starts_with("Only 2 of 8 items")
        );
        assert!(
            says(Facts {
                newest: Some(now() - chrono::TimeDelta::days(90)),
                ..clean()
            })
            .contains("2026-08-07")
        );
        assert!(
            says(Facts {
                newest: None,
                ..clean()
            })
            .starts_with("None of its jobs says when")
        );
        assert!(
            says(Facts {
                one_company: Some("Acme"),
                ..clean()
            })
            .contains("one company's hiring page (Acme)")
        );
        assert!(
            says(Facts {
                verdict: Some("forbid"),
                ..clean()
            })
            .starts_with("Its terms do not allow")
        );
        assert!(
            says(Facts {
                verdict: Some("ask_first"),
                ..clean()
            })
            .contains("permission")
        );
        assert!(
            says(Facts {
                budget_reached: true,
                verdict: None,
                terms_fresh: false,
                ..clean()
            })
            .contains("AI limit")
        );
        assert!(
            says(Facts {
                verdict: None,
                terms_fresh: false,
                ..clean()
            })
            .contains("not been read")
        );
        assert!(
            says(Facts {
                edited: true,
                ..clean()
            })
            .starts_with("You changed")
        );
        assert!(
            says(Facts {
                proposal_waits: true,
                ..clean()
            })
            .contains("use that config")
        );
        assert!(
            says(Facts {
                needs_key: true,
                ..clean()
            })
            .contains("key")
        );
        assert!(
            says(Facts {
                rules_forbid_list: true,
                ..clean()
            })
            .contains("site's rules")
        );
        assert_eq!(says(clean()), "It waits for your approval.");
    }

    #[test]
    fn no_sentence_uses_jargon_for_the_ui() {
        let facts = [
            Facts {
                found: false,
                ..clean()
            },
            Facts {
                rules_forbid_list: true,
                ..clean()
            },
            Facts {
                needs_key: true,
                ..clean()
            },
            Facts {
                sample: 0,
                ..clean()
            },
            Facts {
                postings: 1,
                ..clean()
            },
            Facts {
                newest: None,
                ..clean()
            },
        ];
        for f in &facts {
            let sentence = because(f);
            for word in ["RSS", "robots", "API", "Atom"] {
                assert!(!sentence.contains(word), "{sentence}");
            }
        }
    }
}
