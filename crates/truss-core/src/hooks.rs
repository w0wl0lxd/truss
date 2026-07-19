use crate::error::{Error, Result};
use crate::sync::SyncContext;
use crate::template::Engine;
use indexmap::IndexMap;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HookPhase {
    Pre,
    Post,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HookCondition {
    pub prompt: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Hook {
    pub phase: HookPhase,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: IndexMap<String, String>,
    #[serde(default)]
    pub commands: Option<Vec<String>>,
    #[serde(default)]
    pub when: Option<HookCondition>,
}

impl Hook {
    fn command_allowed(&self, command_name: &str) -> bool {
        match &self.commands {
            Some(allowed) => allowed.iter().any(|c| c == command_name),
            None => true,
        }
    }

    fn condition_matches(&self, ctx: &SyncContext) -> bool {
        match &self.when {
            Some(condition) => {
                let value = resolve_context_value(ctx, &condition.prompt);
                condition.values.iter().any(|v| v == value)
            }
            None => true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct HookManifest {
    #[serde(default)]
    pub hooks: Vec<Hook>,
}

impl HookManifest {
    pub fn from_toml(text: &str) -> Result<Self> {
        toml_edit::de::from_str(text).map_err(|e| Error::Argument(format!("invalid hooks: {e}")))
    }

    /// Return the hooks matching `phase` and `command_name` whose conditions are
    /// satisfied. The caller is responsible for dry-run reporting or execution.
    pub fn matching_hooks(
        &self,
        phase: HookPhase,
        command_name: &str,
        ctx: &SyncContext,
    ) -> Result<Vec<&Hook>> {
        let mut out = Vec::new();
        for hook in &self.hooks {
            if hook.phase != phase {
                continue;
            }
            if !hook.command_allowed(command_name) {
                continue;
            }
            if !hook.condition_matches(ctx) {
                continue;
            }
            out.push(hook);
        }
        Ok(out)
    }
}

/// Render hook arguments and environment values against the sync context, then
/// either execute the hooks or return a list of "would run" descriptions.
pub fn run_hooks(
    manifest: &HookManifest,
    phase: HookPhase,
    command_name: &str,
    ctx: &SyncContext,
    cwd: &Path,
    dry_run: bool,
) -> Result<Vec<String>> {
    let engine = Engine::new();
    let ctx_value = ctx.render_context()?;
    let mut descriptions = Vec::new();

    for hook in manifest.matching_hooks(phase, command_name, ctx)? {
        let command = render(&engine, &ctx_value, &hook.command)?;
        let args: Vec<String> = hook
            .args
            .iter()
            .map(|a| render(&engine, &ctx_value, a))
            .collect::<Result<_>>()?;
        let env: IndexMap<String, String> = hook
            .env
            .iter()
            .map(|(k, v)| {
                let key = render(&engine, &ctx_value, k)?;
                let value = render(&engine, &ctx_value, v)?;
                Ok((key, value))
            })
            .collect::<Result<_>>()?;

        validate_hook_command(&command)?;
        for arg in &args {
            if arg.contains("..") {
                return Err(Error::Argument(format!(
                    "hook argument cannot contain '..': {arg}"
                )));
            }
        }

        if dry_run {
            descriptions.push(
                format!("hook {command} {}", args.join(" "))
                    .trim()
                    .to_string(),
            );
            continue;
        }

        // Resolve relative command paths against the target directory so a hook
        // like `scripts/setup.sh` works regardless of where truss is invoked.
        let program = if command.contains('/') || command.contains('\\') {
            cwd.join(&command)
        } else {
            PathBuf::from(&command)
        };
        let mut cmd = Command::new(program);
        cmd.args(&args).envs(env).current_dir(cwd);
        let status = cmd.status().map_err(Error::Io)?;
        if !status.success() {
            return Err(Error::Argument(format!(
                "hook failed: {command} {}",
                args.join(" ")
            )));
        }
    }

    Ok(descriptions)
}

fn render(engine: &Engine, ctx: &serde_json::Value, source: &str) -> Result<String> {
    // Fast path for values without template syntax.
    if !source.contains("{{") && !source.contains("{%") && !source.contains("{#") {
        return Ok(source.to_string());
    }
    engine.render_str(source, ctx)
}

fn resolve_context_value<'a>(ctx: &'a SyncContext, key: &str) -> &'a str {
    match key {
        "project_name" => &ctx.project_name,
        "author" => &ctx.author,
        "license" => &ctx.license,
        "repository" => &ctx.repository,
        "edition" => &ctx.edition,
        // Optional prompt answers that were skipped default to an empty string
        // so the condition simply evaluates to false instead of failing.
        _ => match ctx.extra.get(key) {
            Some(v) => v.as_str(),
            None => "",
        },
    }
}

fn validate_hook_command(value: &str) -> Result<()> {
    // Reject absolute paths and path traversal in the command value.
    if value.starts_with('/') || value.starts_with('\\') {
        return Err(Error::Argument(format!(
            "hook command must be relative: {value}"
        )));
    }
    if value.contains("..") {
        return Err(Error::Argument(format!(
            "hook command cannot contain '..': {value}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn ctx() -> SyncContext {
        SyncContext::new()
            .with_project_name("demo")
            .with_author("test")
            .with_license("MIT")
            .with_repository("")
            .with_edition("2024")
    }

    #[test]
    fn hook_runs_command_and_fails_on_bad_exit() {
        let manifest = HookManifest::from_toml(
            r#"
[[hooks]]
phase = "post"
command = "false"
"#,
        )
        .unwrap();
        let dir = tempdir().unwrap();
        let result = run_hooks(&manifest, HookPhase::Post, "new", &ctx(), dir.path(), false);
        assert!(result.is_err());
    }

    #[test]
    fn hook_command_restriction_filters_by_command_name() {
        let manifest = HookManifest::from_toml(
            r#"
[[hooks]]
phase = "pre"
command = "echo"
args = ["sync-only"]
commands = ["sync"]
"#,
        )
        .unwrap();
        let dir = tempdir().unwrap();
        let list = run_hooks(&manifest, HookPhase::Pre, "new", &ctx(), dir.path(), false).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn hook_condition_checks_context_value() {
        let manifest = HookManifest::from_toml(
            r#"
[[hooks]]
phase = "pre"
command = "echo"
args = ["pro"]
when = { prompt = "edition", values = ["2024"] }
"#,
        )
        .unwrap();
        let dir = tempdir().unwrap();
        let list = run_hooks(&manifest, HookPhase::Pre, "new", &ctx(), dir.path(), true).unwrap();
        assert_eq!(list.len(), 1);
    }
}
