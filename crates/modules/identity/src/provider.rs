//! The providers people sign in with.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A sign-in provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    /// Sign in with Campbooks (OIDC); also grants Campbooks API access later.
    Campbooks,
    /// Sign in with LinkedIn (OIDC: name, email, photo).
    Linkedin,
}

impl Provider {
    /// Every provider, in the order the sign-in page shows them.
    pub const ALL: [Self; 2] = [Self::Campbooks, Self::Linkedin];

    /// The wire and database name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Campbooks => "campbooks",
            Self::Linkedin => "linkedin",
        }
    }

    /// The provider named `name`.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == name)
    }
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for provider in Provider::ALL {
            assert_eq!(Provider::parse(provider.as_str()), Some(provider));
            assert_eq!(
                serde_json::to_value(provider).unwrap(),
                serde_json::json!(provider.as_str())
            );
        }
        assert_eq!(Provider::parse("google"), None);
    }
}
