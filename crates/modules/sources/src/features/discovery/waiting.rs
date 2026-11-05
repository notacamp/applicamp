//! Why a candidate still waits for the owner (#108), kept in its report as a
//! code and, for some, a detail, so the admin page words it in the person's
//! language (`WaitingCode`).

use chrono::{DateTime, Utc};

use super::{ALIVE_DAYS, MIN_POSTINGS};
use crate::job_boards::{Waiting, WaitingCode};

/// What a check knows about a candidate it did not approve.
#[derive(Debug, Default)]
pub struct Facts<'a> {
    /// Its list of jobs needs a key that is not set.
    pub needs_key: bool,
    /// The site's rules do not let apps read its list of jobs.
    pub rules_forbid_list: bool,
    /// A list of jobs was found.
    pub found: bool,
    /// How many jobs could be read from it.
    pub sample: usize,
    /// How many of them look like postings.
    pub postings: usize,
    /// The newest posting date in the sample.
    pub newest: Option<DateTime<Utc>>,
    /// The check used up its requests before it found the list.
    pub out_of_requests: bool,
    pub requests: usize,
    /// The hiring system every job applies on, when it is one company's page.
    pub one_system: Option<&'a str>,
    /// The terms' verdict, the sentence that decided it, and whether it is of
    /// the terms as they are now.
    pub verdict: Option<&'a str>,
    pub quote: Option<&'a str>,
    pub terms_fresh: bool,
    pub budget_reached: bool,
    /// The owner changed its settings.
    pub edited: bool,
    /// A config was found that the owner has to use.
    pub proposal_waits: bool,
    pub now: Option<DateTime<Utc>>,
}

/// The first reason that applies.
pub fn because(f: &Facts<'_>) -> Waiting {
    use WaitingCode as C;
    if f.needs_key {
        return Waiting::new(C::NeedsKey);
    }
    if f.rules_forbid_list {
        return Waiting::new(C::ListNotAllowed);
    }
    if !f.found {
        return if f.out_of_requests {
            Waiting::with(C::RequestLimit, f.requests.to_string())
        } else {
            Waiting::new(C::NoJobList)
        };
    }
    if f.sample == 0 {
        return Waiting::new(C::EmptyList);
    }
    if let Some(system) = f.one_system {
        return Waiting::with(C::JobsOnHiringSystem, system);
    }
    if f.sample < MIN_POSTINGS || f.postings < MIN_POSTINGS {
        return Waiting::new(C::NotAJobBoard);
    }
    let since = f.now.map(|now| now - chrono::TimeDelta::days(ALIVE_DAYS));
    if since.is_some_and(|since| f.newest.is_none_or(|at| at < since)) {
        return match f.newest {
            Some(at) => Waiting::with(C::NothingRecent, at.format("%Y-%m-%d").to_string()),
            None => Waiting::new(C::Undated),
        };
    }
    let quoted = |code| match f.quote {
        Some(quote) => Waiting::with(code, quote),
        None => Waiting::new(code),
    };
    match f.verdict {
        Some("forbid") => return quoted(C::TermsForbid),
        Some("ask_first") => return quoted(C::TermsAskFirst),
        Some("personal_only") => return quoted(C::TermsPersonalOnly),
        _ => {}
    }
    if f.budget_reached || !f.terms_fresh {
        return Waiting::new(C::TermsUnread);
    }
    if f.edited {
        return Waiting::new(C::OwnerEdited);
    }
    if f.proposal_waits {
        return Waiting::new(C::ConfigToUse);
    }
    Waiting::new(C::NeedsApproval)
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

    fn code(facts: Facts<'_>) -> WaitingCode {
        because(&facts).code
    }

    #[test]
    fn each_reason_is_a_code() {
        use WaitingCode as C;
        assert_eq!(
            code(Facts {
                found: false,
                ..clean()
            }),
            C::NoJobList
        );
        let limit = because(&Facts {
            found: false,
            out_of_requests: true,
            requests: 25,
            ..clean()
        });
        assert_eq!(
            (limit.code, limit.detail.as_deref()),
            (C::RequestLimit, Some("25"))
        );
        assert_eq!(
            code(Facts {
                sample: 0,
                ..clean()
            }),
            C::EmptyList
        );
        assert_eq!(
            code(Facts {
                sample: 3,
                postings: 3,
                ..clean()
            }),
            C::NotAJobBoard
        );
        assert_eq!(
            code(Facts {
                postings: 2,
                ..clean()
            }),
            C::NotAJobBoard
        );
        let old = because(&Facts {
            newest: Some(now() - chrono::TimeDelta::days(90)),
            ..clean()
        });
        assert_eq!(
            (old.code, old.detail.as_deref()),
            (C::NothingRecent, Some("2026-08-07"))
        );
        assert_eq!(
            code(Facts {
                newest: None,
                ..clean()
            }),
            C::Undated
        );
        let one = because(&Facts {
            one_system: Some("Lever"),
            ..clean()
        });
        assert_eq!(
            (one.code, one.detail.as_deref()),
            (C::JobsOnHiringSystem, Some("Lever"))
        );
        let forbid = because(&Facts {
            verdict: Some("forbid"),
            quote: Some("No apps."),
            ..clean()
        });
        assert_eq!(
            (forbid.code, forbid.detail.as_deref()),
            (C::TermsForbid, Some("No apps."))
        );
        assert_eq!(
            code(Facts {
                verdict: Some("ask_first"),
                ..clean()
            }),
            C::TermsAskFirst
        );
        assert_eq!(
            code(Facts {
                verdict: Some("personal_only"),
                ..clean()
            }),
            C::TermsPersonalOnly
        );
        assert_eq!(
            code(Facts {
                budget_reached: true,
                verdict: None,
                terms_fresh: false,
                ..clean()
            }),
            C::TermsUnread
        );
        assert_eq!(
            code(Facts {
                verdict: None,
                terms_fresh: false,
                ..clean()
            }),
            C::TermsUnread
        );
        assert_eq!(
            code(Facts {
                edited: true,
                ..clean()
            }),
            C::OwnerEdited
        );
        assert_eq!(
            code(Facts {
                proposal_waits: true,
                ..clean()
            }),
            C::ConfigToUse
        );
        assert_eq!(
            code(Facts {
                needs_key: true,
                ..clean()
            }),
            C::NeedsKey
        );
        assert_eq!(
            code(Facts {
                rules_forbid_list: true,
                ..clean()
            }),
            C::ListNotAllowed
        );
        assert_eq!(code(clean()), C::NeedsApproval);
    }

    #[test]
    fn codes_are_snake_case_in_json() {
        let json =
            serde_json::to_value(Waiting::with(WaitingCode::JobsOnHiringSystem, "Lever")).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "code": "jobs_on_hiring_system", "detail": "Lever" })
        );
        let json = serde_json::to_value(Waiting::new(WaitingCode::NotAJobBoard)).unwrap();
        assert_eq!(json, serde_json::json!({ "code": "not_a_job_board" }));
    }
}
