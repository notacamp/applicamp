//! The run kinds this instance executes: every module's, gathered by the
//! server, and `runs`' own ([`crate::probe`]).

use std::collections::BTreeMap;

use applicamp_kernel::agent::RunKind;

/// The kinds, by name. Cheap to share behind an `Arc`.
#[derive(Debug, Clone, Default)]
pub struct Kinds {
    kinds: BTreeMap<&'static str, RunKind>,
}

impl Kinds {
    /// `kinds` and `runs`' own; a later kind of the same name replaces an
    /// earlier one (the server checks names are unique,
    /// [`applicamp_kernel::agent::check_kinds`]).
    pub fn new(kinds: impl IntoIterator<Item = RunKind>) -> Self {
        let kinds = crate::own_kinds()
            .into_iter()
            .chain(kinds)
            .map(|kind| (kind.name, kind))
            .collect();
        Self { kinds }
    }

    /// The kind named `name`.
    pub fn get(&self, name: &str) -> Option<&RunKind> {
        self.kinds.get(name)
    }

    /// Every kind, by name.
    pub fn iter(&self) -> impl Iterator<Item = &RunKind> {
        self.kinds.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probe_is_always_there() {
        let kinds = Kinds::new([]);
        let probe = kinds.get(crate::probe::KIND).unwrap();
        assert!(probe.manual);
        assert_eq!(kinds.iter().count(), 1);
        assert!(kinds.get("runs.nothing").is_none());
    }
}
