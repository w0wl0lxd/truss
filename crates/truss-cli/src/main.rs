use clap::{Args, Parser, Subcommand};
use color_eyre::eyre::bail;
use color_eyre::Result;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;

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
    New(NewArgs),
    Sync(SyncArgs),
    Check(CheckArgs),
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
    truss_core::sync_workspace(&path, &template, &ctx)?;
    println!("synced template {template} into {}", path.display());
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
            println!("drift: {} (expected {} bytes, actual {} bytes)", d.file, d.expected.len(), d.actual.len());
        }
        bail!("drift detected in {} file(s)", drift.len());
    }

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

    let registry = truss_core::Registry::load()?;
    let mut choices = vec!["default".to_string()];
    choices.extend(registry.entries().keys().cloned());

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
