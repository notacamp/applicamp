//! What the profile module needs from other modules, as ports the server
//! wires (modules never import each other): reading a past employer's own
//! website for "Tell me about you" (#43).
//!
//! The server answers with the pipeline's company facts, so a past employer
//! and a job's company are one mechanism and one cache. The wizard sees only
//! this port; when the agent harness has a `profile_research` run kind, the
//! same trait is implemented over it and the wizard does not change.

use std::sync::Arc;

use applicamp_kernel::{AppState, BoxFuture, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// How well a researched fact is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactBasis {
    /// Its quote is verbatim on a page that was fetched.
    Sourced,
    /// The model's general knowledge, or a guess: never a fact until the
    /// person says so.
    Assumed,
}

/// One thing found out about an employer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedFact {
    /// What is claimed.
    pub claim: String,
    /// Whether a page says it.
    pub status: FactBasis,
    /// The page's own words (sourced facts only).
    pub quote: Option<String>,
    /// The page it was read on (sourced facts only).
    pub source_url: Option<String>,
    /// Why it is a fact, or why it is only assumed.
    pub reason: String,
}

impl ProposedFact {
    /// The fact as a port reported it, held to the rule: it is `Sourced`
    /// only with a quote and the page it came from; anything else is
    /// `Assumed`, with the missing source as its reason.
    #[must_use]
    pub fn checked(mut self) -> Self {
        let quoted = self.quote.as_deref().is_some_and(|q| !q.trim().is_empty())
            && self
                .source_url
                .as_deref()
                .is_some_and(|u| !u.trim().is_empty());
        if self.status == FactBasis::Sourced && !quoted {
            self.status = FactBasis::Assumed;
            self.quote = None;
            self.source_url = None;
            self.reason = "No page was quoted for it.".to_owned();
        }
        self
    }
}

/// What researching one employer came to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Research {
    /// The employer's site, when one is known.
    pub site: Option<String>,
    /// This call read the site (one model call); false for facts that were
    /// already fresh or when there was no site to read.
    pub read: bool,
    /// What was found.
    pub facts: Vec<ProposedFact>,
}

/// Researching a past employer.
pub trait EmployerResearch: Send + Sync + 'static {
    /// The site of the company named `company`, when one is known (typed
    /// before, or known from a job). No site, no fetch.
    fn known_site<'a>(
        &'a self,
        db: &'a PgPool,
        account: Uuid,
        company: &'a str,
    ) -> BoxFuture<'a, Result<Option<String>>>;

    /// Record `url` as `company`'s own site, typed by the person; the site as
    /// stored. 400 for an address that may not be read.
    fn set_site<'a>(
        &'a self,
        state: &'a AppState,
        account: Uuid,
        company: &'a str,
        url: &'a str,
    ) -> BoxFuture<'a, Result<String>>;

    /// What `company`'s own pages say: read now (**one model call**) unless
    /// the facts are fresh (30 days). Without a known site, nothing is
    /// fetched and nothing is found.
    fn run<'a>(
        &'a self,
        state: &'a AppState,
        account: Uuid,
        company: &'a str,
        now: DateTime<Utc>,
    ) -> BoxFuture<'a, Result<Research>>;
}

/// The port, if the instance has one: an extension of the router.
#[derive(Clone, Default)]
pub struct Researcher(pub Option<Arc<dyn EmployerResearch>>);

impl std::fmt::Debug for Researcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Researcher")
            .field(&self.0.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(status: FactBasis, quote: Option<&str>, url: Option<&str>) -> ProposedFact {
        ProposedFact {
            claim: "Sells to small shops".to_owned(),
            status,
            quote: quote.map(str::to_owned),
            source_url: url.map(str::to_owned),
            reason: "Read on its site".to_owned(),
        }
    }

    #[test]
    fn sourced_needs_a_quote_and_the_page() {
        let ok = fact(
            FactBasis::Sourced,
            Some("small shops"),
            Some("https://x.example/about"),
        )
        .checked();
        assert_eq!(ok.status, FactBasis::Sourced);
        for bad in [
            fact(FactBasis::Sourced, None, Some("https://x.example/")),
            fact(FactBasis::Sourced, Some("  "), Some("https://x.example/")),
            fact(FactBasis::Sourced, Some("small shops"), None),
        ] {
            let checked = bad.checked();
            assert_eq!(checked.status, FactBasis::Assumed);
            assert!(checked.quote.is_none() && checked.source_url.is_none());
        }
        assert_eq!(
            fact(FactBasis::Assumed, None, None).checked().status,
            FactBasis::Assumed
        );
    }
}
