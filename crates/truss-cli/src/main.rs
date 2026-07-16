use clap::{Args, Parser, Subcommand, ValueEnum};
use color_eyre::Result;
use color_eyre::eyre::bail;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;
use truss_core::{Kind, PlanAction, ProtectList, Registry, RegistryEntry, SyncOptions};

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
}

#[derive(Args)]
struct RegistryRemoveArgs {
    name: String,
}

#[derive(Clone, ValueEnum)]
enum CliKind {
    Dir,
    File,
    Json,
}

impl From<CliKind> for Kind {
    fn from(value: CliKind) -> Self {
        match value {
            CliKind::Dir => Self::Dir,
            CliKind::File => Self::File,
            CliKind::Json => Self::Json,
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
}

#[derive(Args)]
struct SyncArgs {
    #[arg(short, long)]
    path: Option<PathBuf>,
    #[arg(short, long)]
    template: Option<String>,
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
    let author = prompt("Author:", "owner")?;
    let license = prompt("License:", "MIT")?;
    let repository = prompt("Repository:", "")?;

    let ctx = truss_core::SyncContext::new()
        .with_project_name(project_name)
        .with_author(author)
        .with_license(license)
        .with_repository(repository);

    std::fs::create_dir_all(&path)?;
    truss_core::new_workspace(&path, &args.template, &ctx)?;
    println!("created workspace at {}", path.display());
    Ok(())
}

fn handle_sync(args: SyncArgs) -> Result<()> {
    let path = resolve_path(args.path)?;
    let template = select_template(args.template)?;
    let ctx = build_context(&path);
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
    let ctx = build_context(&path);
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
    let source = args
        .source
        .canonicalize()
        .map_err(|e| color_eyre::eyre::eyre!("source path: {e}"))?;
    let kind = Kind::from(args.kind);
    let entry = RegistryEntry {
        name: args.name,
        source: source.display().to_string(),
        kind,
        targets: args.targets,
        pointer: None,
        file_mode: None,
        dir_mode: None,
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
    registry.save()?;
    println!("removed {}", args.name);
    Ok(())
}

fn is_interactive() -> bool {
    std::io::stdin().is_terminal()
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

fn build_context(path: &Path) -> truss_core::SyncContext {
    let project_name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().to_string());
    truss_core::SyncContext::new().with_project_name(project_name)
}

fn resolve_path(path: Option<PathBuf>) -> Result<PathBuf> {
    match path {
        Some(p) => Ok(p),
        None => Ok(std::env::current_dir()?),
    }
}
