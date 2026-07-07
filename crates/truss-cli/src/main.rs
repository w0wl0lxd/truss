use clap::{Args, Parser, Subcommand};
use color_eyre::Result;
use std::path::PathBuf;

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
    #[arg(short, long)]
    template: Option<PathBuf>,
    #[arg(short, long)]
    path: Option<PathBuf>,
}

#[derive(Args)]
struct SyncArgs {
    #[arg(short, long)]
    path: Option<PathBuf>,
    #[arg(short, long)]
    entry: Option<String>,
}

#[derive(Args)]
struct CheckArgs {
    #[arg(short, long)]
    path: Option<PathBuf>,
    #[arg(short, long)]
    entry: Option<String>,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::New(args) => handle_new(args),
        Commands::Sync(args) => handle_sync(args),
        Commands::Check(args) => handle_check(args),
    }
}

fn handle_new(args: NewArgs) -> Result<()> {
    let path = resolve_path(args.path);
    let template = args.template.as_deref();
    truss_core::new_workspace(&path, template)?;
    Ok(())
}

fn handle_sync(args: SyncArgs) -> Result<()> {
    let path = resolve_path(args.path);
    let entry = args.entry.as_deref();
    truss_core::sync_workspace(&path, entry)?;
    Ok(())
}

fn handle_check(args: CheckArgs) -> Result<()> {
    let path = resolve_path(args.path);
    let entry = args.entry.as_deref();
    truss_core::check_workspace(&path, entry)?;
    Ok(())
}

fn resolve_path(path: Option<PathBuf>) -> PathBuf {
    match path {
        Some(p) => p,
        None => PathBuf::from("."),
    }
}
