use crate::error::{Error, Result};
use crate::pathsafe::validate_relative_path;
use crate::sync::SyncContext;
use crate::template::Engine;
use indexmap::IndexMap;
use serde::Deserialize;
use std::path::Path;
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

    fn condition_matches(&self, ctx: &SyncContext) -> Result<bool> {
        match &self.when {
            Some(condition) => {
                let value = resolve_context_value(ctx, &condition.prompt)?;
                Ok(condition.values.iter().any(|v| v == value))
            }
            None => Ok(true),
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
            if !hook.condition_matches(ctx)? {
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

        validate_hook_path(&command)?;
        for arg in &args {
            validate_hook_path(arg)?;
        }

        if dry_run {
            descriptions.push(
                format!("hook {command} {}", args.join(" "))
                    .trim()
                    .to_string(),
            );
            continue;
        }

        let mut cmd = Command::new(&command);
        cmd.args(&args).envs(env).current_dir(cwd);
        let output = cmd.output().map_err(Error::Io)?;
        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Argument(format!(
                "hook failed: {command} {}\nstdout: {stdout}\nstderr: {stderr}",
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

fn resolve_context_value<'a>(ctx: &'a SyncContext, key: &str) -> Result<&'a str> {
    match key {
        "project_name" => Ok(&ctx.project_name),
        "author" => Ok(&ctx.author),
        "license" => Ok(&ctx.license),
        "repository" => Ok(&ctx.repository),
        "edition" => Ok(&ctx.edition),
        _ => ctx.extra.get(key).map(String::as_str).ok_or_else(|| {
            Error::Argument(format!("context value not found for condition: {key}"))
        }),
    }
}

fn validate_hook_path(value: &str) -> Result<()> {
    // Reject absolute paths and path traversal in command or argument values.
    if value.starts_with('/') || value.starts_with('\\') {
        return Err(Error::Argument(format!(
            "hook command or argument must be relative: {value}"
        )));
    }
    if value.contains("..") {
        return Err(Error::Argument(format!(
            "hook command or argument cannot contain '..': {value}"
        )));
    }
    if value.contains('/') || value.contains('\\') {
        // Allow relative paths, but ensure they stay under the workspace.
        validate_relative_path(value)?;
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
