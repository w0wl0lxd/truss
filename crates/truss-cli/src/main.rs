use clap::{Args, Parser, Subcommand, ValueEnum};
use color_eyre::Result;
use color_eyre::eyre::bail;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;
use truss_core::{GitCache, Kind, PlanAction, ProtectList, Registry, RegistryEntry, SyncOptions};

#[derive(Parser)]
#[command(
    name = "truss",
    version,
    about = "Rust project scaffolder with template sync and local registries",
    subcommand_required = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new project from a template
    New(NewArgs),
    /// Sync a project to a template
    Sync(SyncArgs),
    /// Check for drift against a template
    Check(CheckArgs),
    /// List embedded and registry templates
    Templates,
    /// Manage the local template registry
    Registry(RegistryCmd),
    /// Manage workspace members
    Member(MemberCmd),
}

#[derive(Args)]
struct RegistryCmd {
    #[command(subcommand)]
    command: RegistryCommands,
}

#[derive(Subcommand)]
enum RegistryCommands {
    /// List registry + embedded templates
    List,
    /// Add a local template source
    Add(RegistryAddArgs),
    /// Remove a user registry entry
    Remove(RegistryRemoveArgs),
}

#[derive(Args)]
struct RegistryAddArgs {
    name: String,
    #[arg(long)]
    source: PathBuf,
    #[arg(long, value_enum, default_value_t = CliKind::Dir)]
    kind: CliKind,
    #[arg(long)]
    force: bool,
    /// Relative destination paths (required for --kind file)
    #[arg(long = "target")]
    targets: Vec<String>,
    /// Git ref (branch, tag, or commit) to checkout for --kind git
    #[arg(long)]
    pointer: Option<String>,
    /// Subfolder inside the Git repository to use as the template root for --kind git
    #[arg(long)]
    subfolder: Option<String>,
    /// Environment variable name containing an HTTPS token for --kind git
    #[arg(long)]
    auth_env: Option<String>,
    /// Path to SSH private key for --kind git
    #[arg(long)]
    ssh_key: Option<String>,
}

#[derive(Args)]
struct RegistryRemoveArgs {
    name: String,
}

#[derive(Clone, ValueEnum)]
enum CliKind {
    Dir,
    File,
    Git,
    Json,
}

impl From<CliKind> for Kind {
    fn from(value: CliKind) -> Self {
        match value {
            CliKind::Dir => Self::Dir,
            CliKind::File => Self::File,
            CliKind::Git => Self::Git,
            CliKind::Json => Self::Json,
        }
    }
}

#[derive(Args)]
struct MemberCmd {
    #[command(subcommand)]
    command: MemberCommands,
}

#[derive(Subcommand)]
enum MemberCommands {
    /// Add a crate to the workspace
    Add(MemberAddArgs),
    /// List workspace members
    List(MemberListArgs),
    /// Remove a workspace member
    Remove(MemberRemoveArgs),
}

#[derive(Args)]
struct MemberAddArgs {
    name: String,
    #[arg(long, value_enum)]
    kind: CliMemberKind,
    #[arg(long)]
    member_path: Option<String>,
    /// Workspace root (defaults to current directory)
    #[arg(short, long)]
    path: Option<PathBuf>,
}

#[derive(Args)]
struct MemberListArgs {
    /// Workspace root (defaults to current directory)
    #[arg(short, long)]
    path: Option<PathBuf>,
}

#[derive(Args)]
struct MemberRemoveArgs {
    name: String,
    /// Workspace root (defaults to current directory)
    #[arg(short, long)]
    path: Option<PathBuf>,
    #[arg(long)]
    delete: bool,
}

#[derive(Clone, ValueEnum)]
enum CliMemberKind {
    Lib,
    Bin,
}

impl From<CliMemberKind> for truss_core::MemberKind {
    fn from(value: CliMemberKind) -> Self {
        match value {
            CliMemberKind::Lib => Self::Lib,
            CliMemberKind::Bin => Self::Bin,
        }
    }
}

#[derive(Args)]
struct NewArgs {
    name: Option<String>,
    #[arg(short, long, default_value = "default")]
    template: String,
    #[arg(short, long)]
    path: Option<PathBuf>,
    #[arg(long)]
    author: Option<String>,
    #[arg(long)]
    license: Option<String>,
    #[arg(long)]
    edition: Option<String>,
}

#[derive(Args)]
struct SyncArgs {
    #[arg(short, long)]
    path: Option<PathBuf>,
    #[arg(short, long)]
    template: Option<String>,
    #[arg(long)]
    author: Option<String>,
    #[arg(long)]
    license: Option<String>,
    #[arg(long)]
    edition: Option<String>,
    /// Preview planned writes without modifying the project
    #[arg(long)]
    dry_run: bool,
    /// Relative paths that must not be overwritten (repeatable)
    #[arg(long = "protect")]
    protect: Vec<String>,
}

#[derive(Args)]
struct CheckArgs {
    #[arg(short, long)]
    path: Option<PathBuf>,
    #[arg(short, long)]
    template: Option<String>,
    #[arg(long)]
    author: Option<String>,
    #[arg(long)]
    license: Option<String>,
    #[arg(long)]
    edition: Option<String>,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::New(args) => handle_new(args),
        Commands::Sync(args) => handle_sync(args),
        Commands::Check(args) => handle_check(args),
        Commands::Templates => handle_templates(),
        Commands::Registry(cmd) => match cmd.command {
            RegistryCommands::List => handle_templates(),
            RegistryCommands::Add(args) => handle_registry_add(args),
            RegistryCommands::Remove(args) => handle_registry_remove(args),
        },
        Commands::Member(cmd) => match cmd.command {
            MemberCommands::Add(args) => handle_member_add(args),
            MemberCommands::List(args) => handle_member_list(args),
            MemberCommands::Remove(args) => handle_member_remove(args),
        },
    }
}

fn handle_new(args: NewArgs) -> Result<()> {
    let name = match args.name {
        Some(n) => n,
        None => {
            if is_interactive() {
                prompt("Project name:", "")?
            } else {
                bail!("project name is required")
            }
        }
    };

    if name.is_empty() {
        bail!("project name cannot be empty");
    }

    let path = match args.path {
        Some(p) => p,
        None => PathBuf::from(&name),
    };
    let project_name = prompt("Project name:", &name)?;
    let author = match args.author {
        Some(author) => author,
        None => prompt("Author:", &default_author())?,
    };
    let license = match args.license {
        Some(license) => license,
        None => prompt("License:", &default_license())?,
    };
    let edition = match args.edition {
        Some(edition) => edition,
        None => prompt("Edition:", &default_edition())?,
    };
    let repository = prompt("Repository:", "")?;

    let ctx = truss_core::SyncContext::new()
        .with_project_name(project_name)
        .with_author(author)
        .with_license(license)
        .with_repository(repository)
        .with_edition(edition);

    std::fs::create_dir_all(&path)?;
    truss_core::new_workspace(&path, &args.template, &ctx)?;
    println!("created workspace at {}", path.display());
    Ok(())
}

fn handle_sync(args: SyncArgs) -> Result<()> {
    let path = resolve_path(args.path)?;
    let template = select_template(args.template)?;
    let ctx = build_context(&path, args.author, args.license, args.edition)?;
    let protect = ProtectList::load(&path, &args.protect)?;
    let options = SyncOptions {
        protect,
        dry_run: args.dry_run,
    };
    let plan = truss_core::sync_workspace_with(&path, &template, &ctx, &options)?;
    if args.dry_run {
        for item in &plan {
            let label = match item.action {
                PlanAction::WouldWrite => "write",
                PlanAction::Unchanged => "unchanged",
                PlanAction::SkipProtected => "skip-protected",
            };
            println!("{label}\t{}", item.path);
        }
        println!(
            "dry-run: {} write(s) planned for template {template} at {}",
            plan.iter()
                .filter(|p| p.action == PlanAction::WouldWrite)
                .count(),
            path.display()
        );
    } else {
        let skipped = plan
            .iter()
            .filter(|p| p.action == PlanAction::SkipProtected)
            .count();
        println!(
            "synced template {template} into {} (protected skips: {skipped})",
            path.display()
        );
    }
    Ok(())
}

fn handle_check(args: CheckArgs) -> Result<()> {
    let path = resolve_path(args.path)?;
    let template = select_template(args.template)?;
    let ctx = build_context(&path, args.author, args.license, args.edition)?;
    let drift = truss_core::check_workspace(&path, &template, &ctx)?;

    if drift.is_empty() {
        println!("no drift");
    } else {
        for d in &drift {
            println!(
                "drift: {} (expected {} bytes, actual {} bytes)",
                d.file,
                d.expected.len(),
                d.actual.len()
            );
        }
        bail!("drift detected in {} file(s)", drift.len());
    }

    Ok(())
}

fn handle_templates() -> Result<()> {
    let rows = truss_core::list_templates()?;
    println!("{:<20} {:<10} SOURCE", "NAME", "KIND");
    for (name, kind, source) in rows {
        println!("{name:<20} {kind:<10} {source}");
    }
    Ok(())
}

fn handle_registry_add(args: RegistryAddArgs) -> Result<()> {
    let kind = Kind::from(args.kind);
    let source = match kind {
        Kind::Git => args.source.to_string_lossy().to_string(),
        _ => args
            .source
            .canonicalize()
            .map_err(|e| color_eyre::eyre::eyre!("source path: {e}"))?
            .display()
            .to_string(),
    };
    let entry = RegistryEntry {
        name: args.name,
        source,
        kind,
        targets: args.targets,
        pointer: args.pointer,
        subfolder: args.subfolder,
        file_mode: None,
        auth_env: args.auth_env,
        ssh_key: args.ssh_key,
    };
    let mut registry = Registry::load_user()?;
    registry.add(entry, args.force)?;
    registry.save()?;
    println!("registered {}", Registry::user_path()?.display());
    Ok(())
}

fn handle_registry_remove(args: RegistryRemoveArgs) -> Result<()> {
    let mut registry = Registry::load_user()?;
    registry.remove(&args.name)?;
    if let Ok(cache) = GitCache::for_entry(&args.name) {
        let _ = cache.remove();
    }
    registry.save()?;
    println!("removed {}", args.name);
    Ok(())
}

fn handle_member_add(args: MemberAddArgs) -> Result<()> {
    let path = resolve_path(args.path)?;
    let ctx = build_context(&path, None, None, None)?;
    let kind = args.kind.into();
    truss_core::add_workspace_member(&path, &args.name, kind, args.member_path.as_deref(), &ctx)?;
    println!("added member {} to {}", args.name, path.display());
    Ok(())
}

fn handle_member_list(args: MemberListArgs) -> Result<()> {
    let path = resolve_path(args.path)?;
    let members = truss_core::list_workspace_members(&path)?;
    for member in members {
        println!("{member}");
    }
    Ok(())
}

fn handle_member_remove(args: MemberRemoveArgs) -> Result<()> {
    let path = resolve_path(args.path)?;
    truss_core::remove_workspace_member(&path, &args.name, args.delete)?;
    println!("removed member {} from {}", args.name, path.display());
    Ok(())
}

fn is_interactive() -> bool {
    std::io::stdin().is_terminal()
}

fn default_author() -> String {
    std::env::var("USER").unwrap_or_else(|_| "author".to_string())
}

fn default_license() -> String {
    String::new()
}

fn default_edition() -> String {
    option_env!("CARGO_PKG_EDITION")
        .unwrap_or_else(|| "2024")
        .to_string()
}

fn prompt(message: &str, default: &str) -> Result<String> {
    if is_interactive() {
        Ok(inquire::Text::new(message).with_default(default).prompt()?)
    } else {
        Ok(default.to_string())
    }
}

fn select_template(template: Option<String>) -> Result<String> {
    if let Some(name) = template {
        return Ok(name);
    }
    if !is_interactive() {
        return Ok("default".to_string());
    }

    let rows = truss_core::list_templates()?;
    let choices: Vec<String> = rows.into_iter().map(|(n, _, _)| n).collect();
    let choice = inquire::Select::new("Choose template or registry entry:", choices).prompt()?;
    Ok(choice)
}

fn build_context(
    path: &Path,
    author: Option<String>,
    license: Option<String>,
    edition: Option<String>,
) -> Result<truss_core::SyncContext> {
    let mut context = truss_core::SyncContext::from_workspace(path)?;
    if context.project_name.is_empty() {
        let fallback = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().to_string());
        context = context.with_project_name(fallback);
    }

    if let Some(author) = author {
        context = context.with_author(author);
    }
    if context.author.is_empty() {
        context = context.with_author(default_author());
    }
    if let Some(license) = license {
        context = context.with_license(license);
    }
    if let Some(edition) = edition {
        context = context.with_edition(edition);
    }

    Ok(context)
}

fn resolve_path(path: Option<PathBuf>) -> Result<PathBuf> {
    match path {
        Some(p) => Ok(p),
        None => Ok(std::env::current_dir()?),
    }
}
