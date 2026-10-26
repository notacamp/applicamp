//! `cargo xtask`: repository automation.
//!
//! - `cargo xtask arch` enforces the architecture rules in `AGENTS.md`.
//! - `cargo xtask openapi [--check]` regenerates (or verifies) `openapi/openapi.json`.

#![forbid(unsafe_code)]

mod arch;
mod openapi;

use std::{
    collections::BTreeSet,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::ExitCode,
};

use anyhow::{Context as _, Result};
use cargo_metadata::MetadataCommand;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "cargo xtask", about = "Applicamp repository automation.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check crate dependency directions, migration names and the schemas
    /// they touch, versions, the composed modules and their background
    /// work, that every OpenAPI operation declares its security and owner,
    /// and that no route is registered outside the OpenAPI document.
    Arch,
    /// Write openapi/openapi.json from the server's routes.
    Openapi {
        /// Fail if the committed document is stale instead of writing it.
        #[arg(long)]
        check: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let root = workspace_root();
    let result = match cli.command {
        Command::Arch => run_arch(&root),
        Command::Openapi { check } => openapi::run(&root, check),
    };
    match result {
        Ok(message) => {
            let _ = writeln!(std::io::stdout(), "{message}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            let _ = writeln!(std::io::stderr(), "error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn workspace_root() -> PathBuf {
    // crates/xtask → the workspace root.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    dir.parent()
        .and_then(Path::parent)
        .unwrap_or(dir)
        .to_path_buf()
}

fn run_arch(root: &Path) -> Result<String> {
    let crates = load_crates(root)?;
    let modules = arch::module_names(&crates);
    let arch_file: arch::ArchitectureFile = toml::from_str(
        &fs::read_to_string(root.join("architecture.toml")).context("reading architecture.toml")?,
    )
    .context("parsing architecture.toml")?;

    let migrations = file_names(&root.join("migrations"))?;
    let migration_sql = migrations
        .iter()
        .filter(|name| name.ends_with(".sql"))
        .map(|name| {
            let path = root.join("migrations").join(name);
            fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))
                .map(|sql| (name.clone(), sql))
        })
        .collect::<Result<Vec<_>>>()?;
    let version = fs::read_to_string(root.join("VERSION")).context("reading VERSION")?;
    let composed = applicamp_server::modules();
    let composed: Vec<&str> = composed.iter().map(|module| module.name).collect();
    let spec = serde_json::to_value(applicamp_server::openapi())
        .context("serializing the OpenAPI document")?;
    let sources = rust_sources(root)?;

    let mut violations = Vec::new();
    violations.extend(arch::check_allowlist(&arch_file.allow, &modules));
    violations.extend(arch::check_dependencies(&crates, &arch_file.allow));
    violations.extend(arch::check_migrations(&migrations, &modules));
    violations.extend(arch::check_migration_schemas(&migration_sql, &modules));
    violations.extend(arch::check_versions(&version, &crates));
    violations.extend(arch::check_composed_modules(&composed, &modules));
    if let Err(problems) = applicamp_server::registry() {
        violations.extend(
            problems
                .into_iter()
                .map(|p| format!("background work: {p}")),
        );
    }
    violations.extend(arch::check_operations(
        &spec,
        applicamp_server::bff::SESSION_SCHEME,
        &modules,
    ));
    violations.extend(arch::check_undocumented_routes(&sources));

    if violations.is_empty() {
        let modules: Vec<_> = modules.iter().map(String::as_str).collect();
        let operations = spec["paths"].as_object().map_or(0, |paths| {
            paths
                .values()
                .map(|item| {
                    arch::OPERATION_METHODS
                        .iter()
                        .filter(|method| item.get(**method).is_some())
                        .count()
                })
                .sum()
        });
        return Ok(format!(
            "architecture OK: {} crates, {} module(s) [{}], {} migration(s), {operations} \
             operation(s), each declaring its security",
            crates.len(),
            modules.len(),
            modules.join(", "),
            migrations.len(),
        ));
    }
    let list: Vec<String> = violations.iter().map(|v| format!("  - {v}")).collect();
    anyhow::bail!(
        "{} architecture violation(s) (see AGENTS.md → Architecture rules):\n{}",
        violations.len(),
        list.join("\n")
    )
}

fn load_crates(root: &Path) -> Result<Vec<arch::Crate>> {
    let metadata = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .no_deps()
        .exec()
        .context("running cargo metadata")?;
    let members: BTreeSet<&str> = metadata
        .workspace_packages()
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    metadata
        .workspace_packages()
        .iter()
        .map(|package| {
            let dir = package
                .manifest_path
                .parent()
                .context("manifest has a parent directory")?;
            let rel = dir
                .strip_prefix(&metadata.workspace_root)
                .with_context(|| format!("{dir} is outside the workspace"))?;
            Ok(arch::Crate {
                name: package.name.to_string(),
                path: rel.as_str().replace('\\', "/"),
                version: package.version.to_string(),
                deps: package
                    .dependencies
                    .iter()
                    .map(|d| d.name.clone())
                    .filter(|name| members.contains(name.as_str()))
                    .collect(),
            })
        })
        .collect()
}

fn file_names(dir: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    Ok(names)
}

/// Every `.rs` file under `crates/*/src` and `crates/modules/*/src`, as
/// `(path relative to the root, source)`.
fn rust_sources(root: &Path) -> Result<Vec<(String, String)>> {
    let mut files = Vec::new();
    let mut pending = vec![root.join("crates")];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
            let path = entry?.path();
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            if path.is_dir() {
                if name.as_deref() != Some("target") && name.as_deref() != Some("xtask") {
                    pending.push(path);
                }
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let source = fs::read_to_string(&path)
                    .with_context(|| format!("reading {}", path.display()))?;
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push((rel, source));
            }
        }
    }
    files.sort();
    Ok(files)
}
