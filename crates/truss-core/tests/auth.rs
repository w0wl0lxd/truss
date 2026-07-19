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
fn askpass_script_outputs_credentials() {
    let creds = auth::GitCredentials::Https {
        username: "w0wl0lxd".into(),
        token: "super-secret".into(),
    };
    let mut cmd = std::process::Command::new("echo");
    let askpass = auth::apply_credentials(&mut cmd, &creds)
        .expect("apply")
        .expect("askpass script");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::metadata(askpass.path())
            .expect("metadata")
            .permissions();
        assert_eq!(perms.mode() & 0o777, 0o700);
    }

    let user_cmd = std::process::Command::new(askpass.path())
        .arg("Username for 'https://github.com':")
        .env("TRUSS_GIT_USERNAME", "w0wl0lxd")
        .env("TRUSS_GIT_TOKEN", "super-secret")
        .output()
        .expect("run askpass");
    assert_eq!(String::from_utf8_lossy(&user_cmd.stdout).trim(), "w0wl0lxd");

    let pass_cmd = std::process::Command::new(askpass.path())
        .arg("Password for 'https://w0wl0lxd@github.com':")
        .env("TRUSS_GIT_USERNAME", "w0wl0lxd")
        .env("TRUSS_GIT_TOKEN", "super-secret")
        .output()
        .expect("run askpass");
    assert_eq!(
        String::from_utf8_lossy(&pass_cmd.stdout).trim(),
        "super-secret"
    );
}

#[test]
fn netrc_parser_handles_empty_lines() {
    let content = "\n\nmachine github.com\n  login user\n  password token\n\n";
    let netrc = auth::Netrc::parse(content).expect("parse");
    assert_eq!(netrc.machines.len(), 1);
}

#[test]
fn netrc_parser_handles_one_line_entry() {
    let content = "machine github.com login user password token\n";
    let netrc = auth::Netrc::parse(content).expect("parse");
    assert_eq!(netrc.machines.len(), 1);
    assert_eq!(netrc.machines[0].host, "github.com");
    assert_eq!(netrc.machines[0].login, "user");
    assert_eq!(netrc.machines[0].password, "token");
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
