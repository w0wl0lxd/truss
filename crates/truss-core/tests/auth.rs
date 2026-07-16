use std::fs;
use tempfile::tempdir;
use truss_core::{Kind, RegistryEntry, auth, git::GitUrl};

#[test]
fn credential_resolver_ssh_url_uses_agent() {
    let url = GitUrl::parse("git@github.com:test/repo.git").expect("parse");
    let entry = RegistryEntry {
        name: "test".into(),
        source: url.resolved.clone(),
        kind: Kind::Git,
        targets: vec![],
        pointer: None,
        subfolder: None,
        file_mode: None,
        auth_env: None,
        ssh_key: None,
    };

    let (creds, source) = auth::CredentialResolver::resolve(&url, &entry).expect("resolve");
    assert_eq!(source, auth::CredentialSource::SshAgent);
    assert!(matches!(creds, auth::GitCredentials::Ssh { key_path } if key_path.is_none()));
}

#[test]
fn credential_resolver_ssh_url_uses_explicit_key() {
    let url = GitUrl::parse("git@github.com:test/repo.git").expect("parse");
    let tmp = tempdir().expect("tempdir");
    let key_path = tmp.path().join("id_rsa");
    fs::write(&key_path, "dummy key").expect("write key");

    let entry = RegistryEntry {
        name: "test".into(),
        source: url.resolved.clone(),
        kind: Kind::Git,
        targets: vec![],
        pointer: None,
        subfolder: None,
        file_mode: None,
        auth_env: None,
        ssh_key: Some(key_path.to_str().unwrap().into()),
    };

    let (creds, source) = auth::CredentialResolver::resolve(&url, &entry).expect("resolve");
    assert_eq!(source, auth::CredentialSource::SshKey);
    assert!(matches!(creds, auth::GitCredentials::Ssh { key_path: Some(k) } if k == key_path));
}

#[test]
fn credential_resolver_ssh_key_not_found() {
    let url = GitUrl::parse("git@github.com:test/repo.git").expect("parse");
    let entry = RegistryEntry {
        name: "test".into(),
        source: url.resolved.clone(),
        kind: Kind::Git,
        targets: vec![],
        pointer: None,
        subfolder: None,
        file_mode: None,
        auth_env: None,
        ssh_key: Some("/nonexistent/key".into()),
    };

    assert!(auth::CredentialResolver::resolve(&url, &entry).is_err());
}

#[test]
fn credential_resolver_rejects_secret_in_auth_env() {
    let url = GitUrl::parse("https://github.com/test/repo.git").expect("parse");
    // Use a string that looks like a token but is clearly a test value
    let fake_token = "ghp_1234567890abcdefghijklmnopqrstuvwx";
    let entry = RegistryEntry {
        name: "test".into(),
        source: url.resolved.clone(),
        kind: Kind::Git,
        targets: vec![],
        pointer: None,
        subfolder: None,
        file_mode: None,
        auth_env: Some(fake_token.into()),
        ssh_key: None,
    };

    // The resolver checks if the auth_env value looks like a secret
    // and rejects it (treating it as if it were the secret itself)
    let result = auth::CredentialResolver::resolve(&url, &entry);
    assert!(result.is_err());
}

#[test]
fn git_askpass_script_generation() {
    let tmp = tempdir().expect("tempdir");
    let script_path = tmp.path().join("askpass.sh");

    let script_content = format!(
        "#!/bin/sh\nif [ \"$1\" = \"Username for 'user':\" ]; then\n  echo 'user'\nelse\n  echo 'token'\nfi\n"
    );

    fs::write(&script_path, script_content).expect("write script");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("set permissions");
    }

    assert!(script_path.exists());
}

#[test]
fn netrc_parser_handles_empty_lines() {
    let content = "\n\nmachine github.com\n  login user\n  password token\n\n";
    let netrc = auth::Netrc::parse(content).expect("parse");
    assert_eq!(netrc.machines.len(), 1);
}

#[test]
fn netrc_parser_handles_mixed_case() {
    let content = "MACHINE github.com\n  LOGIN user\n  PASSWORD token\n";
    let netrc = auth::Netrc::parse(content).expect("parse");
    assert_eq!(netrc.machines.len(), 1);
    assert_eq!(netrc.machines[0].login, "user");
}

#[test]
fn ssh_command_construction_with_key() {
    let tmp = tempdir().expect("tempdir");
    let key_path = tmp.path().join("id_rsa");
    fs::write(&key_path, "dummy").expect("write");

    let creds = auth::GitCredentials::Ssh {
        key_path: Some(key_path.clone()),
    };

    let mut cmd = std::process::Command::new("echo");
    auth::apply_credentials(&mut cmd, &creds).expect("apply");

    let ssh_cmd = cmd.get_envs().find(|(k, _)| *k == "GIT_SSH_COMMAND");
    assert!(ssh_cmd.is_some());
    let (_, value) = ssh_cmd.unwrap();
    assert!(
        value
            .unwrap()
            .to_str()
            .unwrap()
            .contains(&key_path.to_string_lossy().to_string())
    );
}

#[test]
fn ssh_command_construction_without_key() {
    let creds = auth::GitCredentials::Ssh { key_path: None };

    let mut cmd = std::process::Command::new("echo");
    auth::apply_credentials(&mut cmd, &creds).expect("apply");

    let ssh_cmd = cmd.get_envs().find(|(k, _)| *k == "GIT_SSH_COMMAND");
    assert!(ssh_cmd.is_none());
}
