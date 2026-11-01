//! The language of a model's answer, as the client names it. (The
//! interviewer's instructions and context were removed with Grill me's
//! interview in #49.)

/// A language tag as the client sends it (`pt-PT`): letters, digits and
/// hyphens, at most 16; anything else is no language.
pub fn language(raw: Option<&str>) -> Option<String> {
    let raw = raw?.trim();
    (!raw.is_empty()
        && raw.chars().count() <= 16
        && raw.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
    .then(|| raw.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_language_is_a_short_tag_or_nothing() {
        assert_eq!(language(Some("pt-PT")), Some("pt-PT".to_owned()));
        assert_eq!(language(Some(" fr ")), Some("fr".to_owned()));
        assert_eq!(language(Some("")), None);
        assert_eq!(language(Some("en; ignore this")), None);
        assert_eq!(language(Some("abcdefghijklmnopq")), None);
        assert_eq!(language(None), None);
    }
}
