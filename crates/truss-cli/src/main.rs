use clap::{Args, Parser, Subcommand, ValueEnum};
use color_eyre::Result;
use color_eyre::eyre::bail;
use indexmap::IndexMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;
use truss_core::{
    GitCache, Kind, PlanAction, Prompt, PromptKind, PromptManifest, ProtectList, Registry,
    RegistryEntry, SyncOptions,
};

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
    /// Provide a prompt answer as KEY=VALUE (repeatable)
    #[arg(long = "define", value_name = "KEY=VALUE")]
    define: Vec<String>,
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
    /// Provide a prompt answer as KEY=VALUE (repeatable)
    #[arg(long = "define", value_name = "KEY=VALUE")]
    define: Vec<String>,
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
    /// Provide a prompt answer as KEY=VALUE (repeatable)
    #[arg(long = "define", value_name = "KEY=VALUE")]
    define: Vec<String>,
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
                prompt_text("Project name:", "")?
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
    let project_name = prompt_text("Project name:", &name)?;
    let author = match args.author {
        Some(author) => author,
        None => prompt_text("Author:", &default_author())?,
    };
    let license = match args.license {
        Some(license) => license,
        None => prompt_text("License:", &default_license())?,
    };
    let edition = match args.edition {
        Some(edition) => edition,
        None => prompt_text("Edition:", &default_edition())?,
    };
    let repository = prompt_text("Repository:", "")?;

    let mut ctx = truss_core::SyncContext::new()
        .with_project_name(project_name)
        .with_author(author)
        .with_license(license)
        .with_repository(repository)
        .with_edition(edition);

    let template = truss_core::resolve_template(&args.template)?;
    if let Some(manifest) = &template.prompt_manifest {
        let defaults = IndexMap::new();
        let cli = parse_define_args(&args.define)?;
        let extra = collect_prompt_answers(manifest, &defaults, &cli, is_interactive())?;
        for (k, v) in extra {
            ctx = ctx.with_extra(k, v);
        }
    }

    std::fs::create_dir_all(&path)?;
    truss_core::new_workspace(&path, &args.template, &ctx)?;
    println!("created workspace at {}", path.display());
    Ok(())
}

fn handle_sync(args: SyncArgs) -> Result<()> {
    let path = resolve_path(args.path)?;
    let template_name = select_template(args.template)?;
    let mut ctx = build_context(&path, args.author, args.license, args.edition)?;
    let template = truss_core::resolve_template(&template_name)?;
    if let Some(manifest) = &template.prompt_manifest {
        let persisted = truss_core::load_answers(&path.join(".truss/prompts.toml"))?;
        let cli = parse_define_args(&args.define)?;
        let extra = collect_prompt_answers(manifest, &persisted, &cli, is_interactive())?;
        for (k, v) in extra {
            ctx = ctx.with_extra(k, v);
        }
    }
    let protect = ProtectList::load(&path, &args.protect)?;
    let options = SyncOptions {
        protect,
        dry_run: args.dry_run,
    };
    let plan = truss_core::sync_workspace_with(&path, &template_name, &ctx, &options)?;
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
            "dry-run: {} write(s) planned for template {template_name} at {}",
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
            "synced template {template_name} into {} (protected skips: {skipped})",
            path.display()
        );
    }
    Ok(())
}

fn handle_check(args: CheckArgs) -> Result<()> {
    let path = resolve_path(args.path)?;
    let template_name = select_template(args.template)?;
    let mut ctx = build_context(&path, args.author, args.license, args.edition)?;
    let template = truss_core::resolve_template(&template_name)?;
    if let Some(manifest) = &template.prompt_manifest {
        let persisted = truss_core::load_answers(&path.join(".truss/prompts.toml"))?;
        let cli = parse_define_args(&args.define)?;
        let extra = collect_prompt_answers(manifest, &persisted, &cli, is_interactive())?;
        for (k, v) in extra {
            ctx = ctx.with_extra(k, v);
        }
    }
    let drift = truss_core::check_workspace(&path, &template_name, &ctx)?;

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

fn prompt_text(message: &str, default: &str) -> Result<String> {
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

fn parse_define_args(args: &[String]) -> Result<IndexMap<String, String>> {
    let mut out = IndexMap::new();
    for arg in args {
        let (k, v) = parse_key_value(arg)?;
        if k.is_empty() {
            bail!("--define key cannot be empty in {arg:?}");
        }
        if !k
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            bail!("--define key {k:?} must be ASCII alphanumeric, '-' or '_'");
        }
        if is_reserved_prompt_name(&k) {
            bail!("--define key {k:?} is reserved for built-in context variables");
        }
        out.insert(k, v);
    }
    Ok(out)
}

fn parse_key_value(s: &str) -> Result<(String, String)> {
    let mut parts = s.splitn(2, '=');
    let k = parts
        .next()
        .ok_or_else(|| color_eyre::eyre::eyre!("missing key in {s:?}"))?;
    let v = parts
        .next()
        .ok_or_else(|| color_eyre::eyre::eyre!("missing value in {s:?} (expected KEY=VALUE)"))?;
    Ok((k.to_string(), v.to_string()))
}

fn is_reserved_prompt_name(name: &str) -> bool {
    const RESERVED: &[&str] = &[
        "project_name",
        "author",
        "license",
        "edition",
        "repository",
        "extra",
    ];
    RESERVED.contains(&name)
}

fn env_var_for_prompt(name: &str) -> String {
    let normalized = name.to_uppercase().replace('-', "_");
    format!("TRUSS_PROMPT_{normalized}")
}

fn collect_prompt_answers(
    manifest: &PromptManifest,
    persisted: &IndexMap<String, String>,
    cli: &IndexMap<String, String>,
    interactive: bool,
) -> Result<IndexMap<String, String>> {
    let mut answers = IndexMap::new();
    let mut missing = Vec::new();

    for prompt in &manifest.prompts {
        if !prompt.is_visible(&answers) {
            continue;
        }

        let value = if let Some(v) = cli.get(&prompt.name) {
            v.clone()
        } else if let Ok(v) = std::env::var(env_var_for_prompt(&prompt.name)) {
            v
        } else if let Some(v) = persisted.get(&prompt.name) {
            v.clone()
        } else if let Some(v) = &prompt.default {
            v.clone()
        } else if interactive {
            prompt_for(prompt)?
        } else {
            String::new()
        };

        if value.is_empty() {
            if prompt.required {
                missing.push(prompt.name.clone());
            } else {
                answers.insert(prompt.name.clone(), String::new());
            }
        } else {
            answers.insert(prompt.name.clone(), value);
        }
    }

    if !missing.is_empty() {
        bail!("missing required prompt values: {}", missing.join(", "));
    }

    manifest.validate(&answers)?;
    Ok(answers)
}

fn prompt_for(prompt: &Prompt) -> Result<String> {
    match prompt.kind {
        PromptKind::Text => {
            let default = match prompt.default.as_deref() {
                Some(v) => v,
                None => "",
            };
            Ok(inquire::Text::new(&prompt.label)
                .with_default(default)
                .prompt()?)
        }
        PromptKind::Choice => {
            let choices = prompt.choices.clone();
            let default = match &prompt.default {
                Some(v) => v.clone(),
                None => choices.first().cloned().unwrap_or_else(String::new),
            };
            let index = match choices.iter().position(|c| c == &default) {
                Some(i) => i,
                None => 0,
            };
            Ok(inquire::Select::new(&prompt.label, choices)
                .with_starting_cursor(index)
                .prompt()?)
        }
        PromptKind::Bool => {
            let default = prompt.default.as_deref() == Some("true");
            let value = inquire::Confirm::new(&prompt.label)
                .with_default(default)
                .prompt()?;
            Ok(if value { "true".into() } else { "false".into() })
        }
    }
}
