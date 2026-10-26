//! Architecture rules (`AGENTS.md` → "Architecture rules").
//!
//! Everything here is a pure function over plain data so the rules are unit
//! tested with fixtures; `main.rs` only gathers the data from `cargo
//! metadata`, the server and the file system.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

/// Where a workspace crate sits in the architecture, derived from its path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// `crates/kernel`
    Kernel,
    /// `crates/server`
    Server,
    /// `crates/modules/<name>`
    Module(String),
    /// `crates/xtask`
    Xtask,
    /// Anywhere else: not allowed until the rules know about it.
    Unknown,
}

impl Kind {
    /// Classify a crate by its directory relative to the workspace root
    /// (`/`-separated).
    pub fn from_path(rel_dir: &str) -> Self {
        let parts: Vec<&str> = rel_dir.trim_matches('/').split('/').collect();
        match parts.as_slice() {
            ["crates", "kernel"] => Self::Kernel,
            ["crates", "server"] => Self::Server,
            ["crates", "modules", name] => Self::Module((*name).to_owned()),
            ["crates", "xtask"] => Self::Xtask,
            _ => Self::Unknown,
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kernel => f.write_str("kernel"),
            Self::Server => f.write_str("server"),
            Self::Module(name) => write!(f, "module `{name}`"),
            Self::Xtask => f.write_str("xtask"),
            Self::Unknown => f.write_str("unclassified crate"),
        }
    }
}

/// A workspace crate and the workspace crates it depends on (any kind).
#[derive(Debug, Clone)]
pub struct Crate {
    /// Package name.
    pub name: String,
    /// Directory relative to the workspace root.
    pub path: String,
    /// Package version.
    pub version: String,
    /// Names of the *workspace* crates it depends on.
    pub deps: Vec<String>,
}

/// One `[[allow]]` entry of `architecture.toml`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Allow {
    /// Depending module (directory name).
    pub from: String,
    /// Module it may depend on.
    pub to: String,
    /// Why events are not enough.
    pub reason: String,
}

/// `architecture.toml`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchitectureFile {
    /// Allowed module → module dependencies.
    #[serde(default)]
    pub allow: Vec<Allow>,
}

/// Names of the modules (directory names under `crates/modules`).
pub fn module_names(crates: &[Crate]) -> BTreeSet<String> {
    crates
        .iter()
        .filter_map(|c| match Kind::from_path(&c.path) {
            Kind::Module(name) => Some(name),
            _ => None,
        })
        .collect()
}

/// Dependency-direction rules between workspace crates.
pub fn check_dependencies(crates: &[Crate], allow: &[Allow]) -> Vec<String> {
    let kinds: BTreeMap<&str, Kind> = crates
        .iter()
        .map(|c| (c.name.as_str(), Kind::from_path(&c.path)))
        .collect();
    let mut violations = Vec::new();
    for krate in crates {
        let from = Kind::from_path(&krate.path);
        if from == Kind::Unknown {
            violations.push(format!(
                "`{}` lives at `{}`, which is not a known crate location \
                 (crates/kernel, crates/server, crates/modules/<name>, crates/xtask)",
                krate.name, krate.path
            ));
            continue;
        }
        for dep in &krate.deps {
            let Some(to) = kinds.get(dep.as_str()) else {
                continue;
            };
            if let Some(why) = edge_violation(&from, to, allow) {
                violations.push(format!(
                    "`{}` ({from}) must not depend on `{dep}` ({to}): {why}",
                    krate.name
                ));
            }
        }
    }
    violations
}

fn edge_violation(from: &Kind, to: &Kind, allow: &[Allow]) -> Option<&'static str> {
    if *to == Kind::Xtask {
        return Some("nothing depends on xtask");
    }
    if *to == Kind::Server && *from != Kind::Xtask {
        return Some("only xtask may depend on the server");
    }
    match (from, to) {
        (Kind::Kernel, _) => Some("the kernel depends on no workspace crate"),
        (Kind::Module(_), Kind::Kernel) | (Kind::Server | Kind::Xtask, _) => None,
        (Kind::Module(a), Kind::Module(b)) => {
            if allow.iter().any(|e| &e.from == a && &e.to == b) {
                None
            } else {
                Some(
                    "modules don't import modules; publish a domain event, or add an \
                     [[allow]] entry to architecture.toml (reviewed like an ADR)",
                )
            }
        }
        (Kind::Module(_), _) => Some("modules may depend only on the kernel"),
        (Kind::Unknown, _) => Some("unclassified crate"),
    }
}

/// Every allowlist entry names two existing, distinct modules and a reason.
pub fn check_allowlist(allow: &[Allow], modules: &BTreeSet<String>) -> Vec<String> {
    let mut violations = Vec::new();
    for entry in allow {
        for name in [&entry.from, &entry.to] {
            if !modules.contains(name) {
                violations.push(format!(
                    "architecture.toml: [[allow]] {} -> {} names unknown module `{name}`",
                    entry.from, entry.to
                ));
            }
        }
        if entry.from == entry.to {
            violations.push(format!(
                "architecture.toml: [[allow]] {0} -> {0} is a self-reference",
                entry.from
            ));
        }
        if entry.reason.trim().is_empty() {
            violations.push(format!(
                "architecture.toml: [[allow]] {} -> {} needs a non-empty reason",
                entry.from, entry.to
            ));
        }
    }
    violations
}

/// The schemas owned by infrastructure rather than a module: the kernel's
/// (outbox, job queue) and the server's backend-for-frontend (sessions).
pub const INFRASTRUCTURE_SCHEMAS: [&str; 2] = ["kernel", "bff"];

/// Operation owners that are not modules: the server's own `/healthz` and
/// the BFF.
pub const SERVER_OWNERS: [&str; 2] = ["system", "bff"];

fn is_infrastructure(schema: &str) -> bool {
    INFRASTRUCTURE_SCHEMAS.contains(&schema)
}

static MIGRATION_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\d{14})_([a-z0-9_]+?)__([a-z0-9_]+)\.sql$")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});

/// Migration files are `<14-digit timestamp>_<owner>__<description>.sql`,
/// where the owner is a module or an infrastructure schema, with unique
/// timestamps. No module may take an infrastructure schema's name, or a
/// server operation owner's.
pub fn check_migrations(file_names: &[String], modules: &BTreeSet<String>) -> Vec<String> {
    let mut violations = Vec::new();
    let mut versions: BTreeMap<&str, &str> = BTreeMap::new();
    let reserved: BTreeSet<&str> = INFRASTRUCTURE_SCHEMAS
        .into_iter()
        .chain(SERVER_OWNERS)
        .collect();
    for reserved in reserved {
        if modules.contains(reserved) {
            violations.push(format!(
                "crates/modules/{reserved}: `{reserved}` is reserved for the kernel or the server"
            ));
        }
    }
    for name in file_names {
        let Some(caps) = MIGRATION_NAME.captures(name) else {
            violations.push(format!(
                "migrations/{name}: expected `<YYYYMMDDHHMMSS>_<module>__<description>.sql` \
                 (lower-case, digits and underscores)"
            ));
            continue;
        };
        let (version, owner) = (
            caps.get(1).map_or("", |m| m.as_str()),
            caps.get(2).map_or("", |m| m.as_str()),
        );
        if !is_infrastructure(owner) && !modules.contains(owner) {
            let known: Vec<_> = modules.iter().map(String::as_str).collect();
            violations.push(format!(
                "migrations/{name}: `{owner}` is not a module, `kernel` or `bff` \
                 (known modules: {})",
                known.join(", ")
            ));
        }
        if let Some(other) = versions.insert(version, name) {
            violations.push(format!(
                "migrations/{name}: timestamp {version} is already used by {other}"
            ));
        }
    }
    violations
}

static SQL_NOISE: LazyLock<Regex> = LazyLock::new(|| {
    // Comments and string literals are not schema references.
    Regex::new(r"(?s)--[^\n]*|/\*.*?\*/|'(?:[^']|'')*'")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});

static SCHEMA_STATEMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(?:create|alter|drop)\s+schema\s+(?:if\s+(?:not\s+)?exists\s+)?([a-z_][a-z0-9_]*)",
    )
    .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});

static QUALIFIED_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b([a-z_][a-z0-9_]*)\s*\.\s*[a-z_]")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});

/// Each migration touches only its owner's schema: it creates, alters or
/// drops no other schema, never qualifies a name with another module's (or
/// the infrastructure's) schema (so no cross-schema join or foreign key),
/// and never changes `search_path`. `files` are `(file name, SQL)`; files
/// with a malformed name are reported by [`check_migrations`].
pub fn check_migration_schemas(
    files: &[(String, String)],
    modules: &BTreeSet<String>,
) -> Vec<String> {
    let mut violations = Vec::new();
    for (name, raw) in files {
        let Some(owner) = MIGRATION_NAME
            .captures(name)
            .and_then(|caps| caps.get(2))
            .map(|m| m.as_str())
        else {
            continue;
        };
        let sql = SQL_NOISE
            .replace_all(raw, " ")
            .to_lowercase()
            .replace('"', "");
        let mut touched = BTreeSet::new();
        for caps in SCHEMA_STATEMENT.captures_iter(&sql) {
            let schema = caps.get(1).map_or("", |m| m.as_str());
            if schema != owner {
                touched.insert(schema.to_owned());
            }
        }
        for caps in QUALIFIED_NAME.captures_iter(&sql) {
            let schema = caps.get(1).map_or("", |m| m.as_str());
            let known = is_infrastructure(schema) || modules.contains(schema);
            if known && schema != owner {
                touched.insert(schema.to_owned());
            }
        }
        for schema in touched {
            violations.push(format!(
                "migrations/{name}: touches schema `{schema}`; a migration of `{owner}` may \
                 touch only the `{owner}` schema (reference other modules by id)"
            ));
        }
        if raw.to_lowercase().contains("search_path") {
            violations.push(format!(
                "migrations/{name}: must not change `search_path`; qualify names with `{owner}.`"
            ));
        }
    }
    violations
}

/// The server composes exactly the modules under `crates/modules`, each
/// named after its directory.
pub fn check_composed_modules(composed: &[&str], modules: &BTreeSet<String>) -> Vec<String> {
    let composed: BTreeSet<String> = composed.iter().map(|&name| name.to_owned()).collect();
    let mut violations = Vec::new();
    for missing in modules.difference(&composed) {
        violations.push(format!(
            "crates/modules/{missing} is not composed: add it to `applicamp_server::modules()`"
        ));
    }
    for unknown in composed.difference(modules) {
        violations.push(format!(
            "`applicamp_server::modules()` has a module named `{unknown}`, which is not a \
             directory under crates/modules (a module's name is its directory and schema)"
        ));
    }
    violations
}

/// Every workspace crate carries the version in the `VERSION` file.
pub fn check_versions(version_file: &str, crates: &[Crate]) -> Vec<String> {
    let expected = version_file.trim();
    crates
        .iter()
        .filter(|c| c.version != expected)
        .map(|c| {
            format!(
                "`{}` is version {} but VERSION says {expected}; keep [workspace.package] \
                 version in Cargo.toml equal to VERSION",
                c.name, c.version
            )
        })
        .collect()
}

/// HTTP methods an OpenAPI path item may hold operations under.
pub const OPERATION_METHODS: [&str; 8] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

static OPERATION_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([a-z][a-z0-9_]*)\.[a-z][a-z0-9_]*$")
        .unwrap_or_else(|e| unreachable!("static regex compiles: {e}"))
});

/// Every operation of the generated OpenAPI document states its security
/// explicitly, as public (`security(())`, i.e. `[{}]`) or the session
/// cookie scheme `session` (no scopes); nothing is left to a document-wide
/// default. Its `operationId` is a unique `<owner>.<verb>` whose owner is a
/// module or the server (`system`, `bff`).
pub fn check_operations(
    spec: &serde_json::Value,
    session_scheme: &str,
    modules: &BTreeSet<String>,
) -> Vec<String> {
    let mut violations = Vec::new();
    let mut ids = BTreeSet::new();
    let Some(paths) = spec.get("paths").and_then(serde_json::Value::as_object) else {
        return vec!["the OpenAPI document has no paths".to_owned()];
    };
    for (path, item) in paths {
        for method in OPERATION_METHODS {
            let Some(operation) = item.get(method) else {
                continue;
            };
            let name = format!("{} {path}", method.to_uppercase());
            match operation
                .get("security")
                .and_then(serde_json::Value::as_array)
            {
                None => violations.push(format!(
                    "OpenAPI operation `{name}` declares no `security`: mark it public with \
                     `security(())` or require the session with \
                     `security((\"{session_scheme}\" = []))`"
                )),
                Some(requirements) => {
                    for requirement in requirements {
                        if !valid_requirement(requirement, session_scheme) {
                            violations.push(format!(
                                "OpenAPI operation `{name}`: security requirement {requirement} \
                                 is neither public (`{{}}`) nor `{{\"{session_scheme}\": []}}`"
                            ));
                        }
                    }
                }
            }
            let id = operation
                .get("operationId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            match OPERATION_ID.captures(id).and_then(|caps| caps.get(1)) {
                None => violations.push(format!(
                    "OpenAPI operation `{name}` needs an `operation_id` `<owner>.<verb>`, got \
                     `{id}`"
                )),
                Some(owner) => {
                    let owner = owner.as_str();
                    if !modules.contains(owner) && !SERVER_OWNERS.contains(&owner) {
                        violations.push(format!(
                            "OpenAPI operation `{name}`: `{id}` names owner `{owner}`, which is \
                             not a module, `system` or `bff`"
                        ));
                    }
                }
            }
            if !id.is_empty() && !ids.insert(id.to_owned()) {
                violations.push(format!("operationId `{id}` is used twice"));
            }
        }
    }
    violations
}

fn valid_requirement(requirement: &serde_json::Value, session_scheme: &str) -> bool {
    let Some(requirement) = requirement.as_object() else {
        return false;
    };
    requirement.iter().all(|(scheme, scopes)| {
        scheme == session_scheme && scopes.as_array().is_some_and(Vec::is_empty)
    })
}

/// Route registrations that bypass `utoipa-axum` and would be missing from
/// the OpenAPI document.
const UNDOCUMENTED_ROUTES: [&str; 3] = [".route(", ".route_service(", ".nest_service("];

/// No source file registers a route outside `utoipa-axum`'s `routes!`
/// (`OpenApiRouter::routes`), so every route the server answers is in the
/// document. `files` are `(path, source)`; test code (a `tests.rs` file, or
/// anything after a `#[cfg(test)]` line) is skipped.
pub fn check_undocumented_routes(files: &[(String, String)]) -> Vec<String> {
    let mut violations = Vec::new();
    for (path, source) in files {
        if path.ends_with("/tests.rs") || path.contains("/tests/") {
            continue;
        }
        for (number, line) in source.lines().enumerate() {
            if line.trim_start().starts_with("#[cfg(test)]") {
                break;
            }
            if line.trim_start().starts_with("//") {
                continue;
            }
            if let Some(call) = UNDOCUMENTED_ROUTES
                .iter()
                .find(|call| line.contains(**call))
            {
                violations.push(format!(
                    "{path}:{}: `{call}` adds a route the OpenAPI document does not describe; \
                     declare it with `#[utoipa::path]` and mount it with `routes!`",
                    number + 1
                ));
            }
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    fn krate(name: &str, path: &str, deps: &[&str]) -> Crate {
        Crate {
            name: name.to_owned(),
            path: path.to_owned(),
            version: "0.0.0".to_owned(),
            deps: deps.iter().map(|&d| d.to_owned()).collect(),
        }
    }

    fn modules(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|&n| n.to_owned()).collect()
    }

    #[test]
    fn dependency_directions() {
        let crates = [
            krate("applicamp-kernel", "crates/kernel", &[]),
            krate(
                "applicamp-identity",
                "crates/modules/identity",
                &["applicamp-kernel"],
            ),
            krate(
                "applicamp-profile",
                "crates/modules/profile",
                &["applicamp-kernel", "applicamp-identity"],
            ),
            krate(
                "applicamp-server",
                "crates/server",
                &["applicamp-kernel", "applicamp-identity"],
            ),
            krate("xtask", "crates/xtask", &["applicamp-server"]),
        ];
        let violations = check_dependencies(&crates, &[]);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("modules don't import modules"));

        let allow = [Allow {
            from: "profile".into(),
            to: "identity".into(),
            reason: "test".into(),
        }];
        assert!(check_dependencies(&crates, &allow).is_empty());

        let bad = [
            krate("applicamp-kernel", "crates/kernel", &["applicamp-identity"]),
            krate(
                "applicamp-identity",
                "crates/modules/identity",
                &["applicamp-server"],
            ),
            krate("applicamp-server", "crates/server", &[]),
            krate("stray", "tools/stray", &[]),
        ];
        let violations = check_dependencies(&bad, &[]);
        assert_eq!(violations.len(), 3, "{violations:?}");
    }

    #[test]
    fn allowlist_entries_name_real_modules() {
        let allow = [Allow {
            from: "profile".into(),
            to: "profile".into(),
            reason: " ".into(),
        }];
        let violations = check_allowlist(&allow, &modules(&["identity"]));
        assert_eq!(violations.len(), 4, "{violations:?}");
    }

    #[test]
    fn migration_names() {
        let names = [
            "20261026000000_kernel__events_and_jobs.sql".to_owned(),
            "20261026000100_bff__sessions.sql".to_owned(),
            "20261026000200_identity__init.sql".to_owned(),
        ];
        assert!(check_migrations(&names, &modules(&["identity"])).is_empty());

        let bad = [
            "20261026000000_identity__init.sql".to_owned(),
            "20261026000000_identity__again.sql".to_owned(),
            "20261026_identity__short.sql".to_owned(),
            "20261026000300_profile__init.sql".to_owned(),
        ];
        let violations = check_migrations(&bad, &modules(&["identity", "bff"]));
        assert_eq!(violations.len(), 4, "{violations:?}");
    }

    #[test]
    fn migrations_touch_only_their_schema() {
        let modules = modules(&["identity", "profile"]);
        let ok = [(
            "20261026000200_identity__init.sql".to_owned(),
            "CREATE SCHEMA identity;\n\
             -- see bff.sessions\n\
             CREATE TABLE identity.ways_in (account_id uuid REFERENCES identity.accounts (id));"
                .to_owned(),
        )];
        assert!(check_migration_schemas(&ok, &modules).is_empty());

        let bad = [(
            "20261026000300_profile__init.sql".to_owned(),
            "CREATE TABLE profile.goals (account_id uuid REFERENCES identity.accounts (id));\n\
             SET search_path = profile;"
                .to_owned(),
        )];
        let violations = check_migration_schemas(&bad, &modules);
        assert_eq!(violations.len(), 2, "{violations:?}");
        assert!(violations[0].contains("`identity`"));
    }

    #[test]
    fn composed_modules_match_the_directories() {
        assert!(check_composed_modules(&["identity"], &modules(&["identity"])).is_empty());
        let violations = check_composed_modules(&["ident"], &modules(&["identity"]));
        assert_eq!(violations.len(), 2, "{violations:?}");
    }

    #[test]
    fn operations_declare_security_and_an_owner() {
        let spec = serde_json::json!({ "paths": {
            "/healthz": { "get": { "operationId": "system.health", "security": [{}] } },
            "/api/v1/me": { "get": { "operationId": "identity.me", "security": [{ "session": [] }] } },
        }});
        assert!(check_operations(&spec, "session", &modules(&["identity"])).is_empty());

        let bad = serde_json::json!({ "paths": {
            "/a": { "get": { "operationId": "identity.a" } },
            "/b": { "post": { "operationId": "identity.a", "security": [{ "bearer": ["x"] }] } },
            "/c": { "get": { "operationId": "elsewhere.c", "security": [{ "session": ["x"] }] } },
            "/d": { "get": { "security": [{}] } },
        }});
        let violations = check_operations(&bad, "session", &modules(&["identity"]));
        assert_eq!(violations.len(), 6, "{violations:?}");
    }

    #[test]
    fn routes_outside_utoipa_are_found() {
        let files = [
            (
                "crates/server/src/lib.rs".to_owned(),
                "let app = router.routes(routes!(health));\n\
                 // a comment about .route( is fine\n\
                 let bad = Router::new().route(\"/x\", get(x));\n\
                 #[cfg(test)]\n\
                 mod tests { fn t() { Router::new().route(\"/y\", get(y)); } }"
                    .to_owned(),
            ),
            (
                "crates/server/src/bff/tests.rs".to_owned(),
                "Router::new().route(\"/z\", get(z));".to_owned(),
            ),
        ];
        let violations = check_undocumented_routes(&files);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].starts_with("crates/server/src/lib.rs:3:"));
    }

    #[test]
    fn versions_match() {
        let crates = [krate("a", "crates/kernel", &[])];
        assert!(check_versions("0.0.0\n", &crates).is_empty());
        assert_eq!(check_versions("0.1.0", &crates).len(), 1);
    }
}
