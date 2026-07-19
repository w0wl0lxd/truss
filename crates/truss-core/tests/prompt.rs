use indexmap::IndexMap;
use tempfile::TempDir;
use truss_core::{Engine, PromptManifest, SyncContext, Template};

#[test]
fn prompt_manifest_parses_toml() {
    let toml = r#"
[prompts]
description = { label = "Project description", kind = "text" }
include_cli = { label = "Include CLI", kind = "bool", default = "false" }
framework = { label = "Framework", kind = "choice", choices = ["axum", "actix"], default = "axum" }
"#;
    let manifest = PromptManifest::from_toml(toml).unwrap();
    assert_eq!(manifest.prompts.len(), 3);
    assert_eq!(manifest.prompts[0].name, "description");
    assert_eq!(manifest.prompts[1].kind, truss_core::PromptKind::Bool);
    assert_eq!(manifest.prompts[2].kind, truss_core::PromptKind::Choice);
}

#[test]
fn regex_validation_rejects_invalid_value() {
    let toml = r#"
[prompts]
version = { label = "Version", kind = "text", regex = "^\\d+\\.\\d+\\.\\d+$", required = true }
"#;
    let manifest = PromptManifest::from_toml(toml).unwrap();
    let mut answers = IndexMap::new();
    answers.insert("version".into(), "1.0".into());
    assert!(manifest.validate(&answers).is_err());

    answers.insert("version".into(), "1.0.0".into());
    manifest.validate(&answers).unwrap();
}

#[test]
fn conditional_prompts_only_validate_when_visible() {
    let toml = r#"
[prompts]
include_cli = { label = "Include CLI", kind = "bool", default = "false" }
framework = { label = "Framework", kind = "choice", choices = ["axum", "actix"], condition = { prompt = "include_cli", values = ["true"] }, required = true }
"#;
    let manifest = PromptManifest::from_toml(toml).unwrap();
    let mut answers = IndexMap::new();
    answers.insert("include_cli".into(), "false".into());
    // framework is hidden, so its required constraint does not apply
    manifest.validate(&answers).unwrap();

    answers.insert("include_cli".into(), "true".into());
    assert!(manifest.validate(&answers).is_err());

    answers.insert("framework".into(), "axum".into());
    manifest.validate(&answers).unwrap();
}

#[test]
fn save_and_load_answers_roundtrip() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("prompts.toml");
    let mut answers = IndexMap::new();
    answers.insert("description".into(), "Hello world".into());
    answers.insert("include_cli".into(), "true".into());

    truss_core::save_answers(&path, &answers).unwrap();
    let loaded = truss_core::load_answers(&path).unwrap();
    assert_eq!(loaded, answers);
}

#[test]
fn template_ignores_truss_toml_in_source_files() {
    let dir = TempDir::new().unwrap();
    let td = dir.path();
    std::fs::write(
        td.join("truss.toml"),
        r#"
[prompts]
description = { label = "Project description", kind = "text" }
"#,
    )
    .unwrap();
    std::fs::write(td.join("Cargo.toml"), "[package]\n").unwrap();

    let template = Template::from_directory(td).unwrap();
    assert!(template.files.iter().all(|f| f.path != "truss.toml"));
    assert!(template.prompt_manifest.is_some());
}

#[test]
fn template_renders_prompt_variables_in_paths_and_content() {
    let dir = TempDir::new().unwrap();
    let td = dir.path();
    std::fs::write(
        td.join("truss.toml"),
        r#"
[prompts]
description = { label = "Project description", kind = "text", default = "A project" }
"#,
    )
    .unwrap();
    std::fs::create_dir(td.join("src")).unwrap();
    std::fs::write(
        td.join("Cargo.toml"),
        "name = \"{{ project_name }}\"\ndescription = \"{{ description }}\"\n",
    )
    .unwrap();
    std::fs::write(td.join("src/{{ project_name }}.rs"), "fn main() {}\n").unwrap();

    let template = Template::from_directory(td).unwrap();
    let ctx = SyncContext::new()
        .with_project_name("myapp")
        .with_author("me")
        .with_license("MIT")
        .with_repository("")
        .with_edition("2024")
        .with_extra("description", "Hello world");

    let rendered = template.render(&ctx, &Engine::new()).unwrap();
    let cargo = rendered.iter().find(|f| f.path == "Cargo.toml").unwrap();
    assert!(cargo.content.contains("name = \"myapp\""));
    assert!(cargo.content.contains("description = \"Hello world\""));

    let main = rendered.iter().find(|f| f.path == "src/myapp.rs").unwrap();
    assert_eq!(main.path, "src/myapp.rs");
}
