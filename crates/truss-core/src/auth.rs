use crate::error::{Error, Result};
use crate::git::GitUrl;
use crate::registry::RegistryEntry;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Authentication material for a Git repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitCredentials {
    /// HTTPS token authentication (username + token/password)
    Https { username: String, token: String },
    /// SSH authentication (relies on ssh-agent or configured key)
    Ssh { key_path: Option<PathBuf> },
}

/// Where credentials originate from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    /// Per-entry environment variable (TRUSS_AUTH_<ENTRY>)
    EntryEnv,
    /// Per-host environment variable (TRUSS_AUTH_<HOST>)
    HostEnv,
    /// Git credential helper
    CredentialHelper,
    /// Netrc file
    Netrc,
    /// SSH agent or config
    SshAgent,
    /// Explicit SSH key path
    SshKey,
}

/// Resolves authentication for a Git URL and registry entry.
pub struct CredentialResolver;

impl CredentialResolver {
    /// Resolve credentials for a Git URL and registry entry.
    ///
    /// Precedence (highest to lowest):
    /// 1. Per-entry env var (TRUSS_AUTH_<ENTRY>)
    /// 2. Per-host env var (TRUSS_AUTH_<HOST>)
    /// 3. Git credential helper
    /// 4. Netrc
    /// 5. SSH agent/config (for SSH URLs)
    /// 6. Explicit SSH key (if configured in entry)
    pub fn resolve(
        url: &GitUrl,
        entry: &RegistryEntry,
    ) -> Result<(GitCredentials, CredentialSource)> {
        // Check if this is an SSH URL
        if url.resolved.starts_with("ssh://") || url.resolved.starts_with("git@") {
            return Self::resolve_ssh(url, entry);
        }

        // Local file URLs never need credentials; callers can fall back to
        // unauthenticated git commands.
        if url.resolved.starts_with("file://") {
            return Err(Error::NoCredentials(url.resolved.clone()));
        }

        // HTTPS URL - try credential sources in order
        if let Some(entry_env_name) = entry.auth_env.as_ref() {
            // First check if the field itself looks like a secret (common mistake)
            if Self::looks_like_secret(entry_env_name) {
                return Err(Error::InvalidCredentialSource(
                    "auth_env value appears to be a secret (use environment variable name instead)"
                        .into(),
                ));
            }
            if let Ok(token) = env::var(entry_env_name) {
                return Ok((
                    GitCredentials::Https {
                        username: Self::default_username(&url.resolved),
                        token,
                    },
                    CredentialSource::EntryEnv,
                ));
            }
        }

        let host = Self::extract_host(&url.resolved)?;
        let host_env_name = format!("TRUSS_AUTH_{}", host.to_uppercase().replace('.', "_"));
        if let Ok(token) = env::var(&host_env_name) {
            return Ok((
                GitCredentials::Https {
                    username: Self::default_username(&url.resolved),
                    token,
                },
                CredentialSource::HostEnv,
            ));
        }

        // Try Git credential helper
        if let Some(creds) = Self::try_credential_helper(&host)? {
            return Ok((creds, CredentialSource::CredentialHelper));
        }

        // Try netrc
        if let Some(creds) = Self::try_netrc(&host)? {
            return Ok((creds, CredentialSource::Netrc));
        }

        Err(Error::NoCredentials(format!(
            "no credentials found for {host}"
        )))
    }

    fn resolve_ssh(
        _url: &GitUrl,
        entry: &RegistryEntry,
    ) -> Result<(GitCredentials, CredentialSource)> {
        if let Some(key_path) = entry.ssh_key.as_ref() {
            // Validate the key path exists
            let path = PathBuf::from(key_path);
            if !path.exists() {
                return Err(Error::Auth(format!(
                    "SSH key not found: {}",
                    path.display()
                )));
            }
            return Ok((
                GitCredentials::Ssh {
                    key_path: Some(path),
                },
                CredentialSource::SshKey,
            ));
        }

        // Rely on ssh-agent/config
        Ok((
            GitCredentials::Ssh { key_path: None },
            CredentialSource::SshAgent,
        ))
    }

    fn extract_host(url: &str) -> Result<String> {
        if url.starts_with("https://") {
            let rest = url
                .strip_prefix("https://")
                .ok_or_else(|| Error::InvalidGitUrl("malformed HTTPS URL".into()))?;
            let host = rest
                .split('/')
                .next()
                .ok_or_else(|| Error::InvalidGitUrl("no host in HTTPS URL".into()))?;
            Ok(host.to_string())
        } else if url.starts_with("http://") {
            let rest = url
                .strip_prefix("http://")
                .ok_or_else(|| Error::InvalidGitUrl("malformed HTTP URL".into()))?;
            let host = rest
                .split('/')
                .next()
                .ok_or_else(|| Error::InvalidGitUrl("no host in HTTP URL".into()))?;
            Ok(host.to_string())
        } else {
            Err(Error::InvalidGitUrl("cannot extract host from URL".into()))
        }
    }

    fn default_username(url: &str) -> String {
        // For GitHub, use x-access-token for token auth
        if url.contains("github.com") {
            "x-access-token".to_string()
        } else {
            "git".to_string()
        }
    }

    fn looks_like_secret(value: &str) -> bool {
        // Heuristic: if it looks like a token or password, it's probably a secret
        value.len() > 20
            && (value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                || value.contains(':'))
    }

    fn try_credential_helper(host: &str) -> Result<Option<GitCredentials>> {
        let mut cmd = std::process::Command::new("git");
        cmd.arg("credential")
            .arg("fill")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .env("GIT_TERMINAL_PROMPT", "0");

        let mut child = cmd.spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Auth("failed to open stdin for git credential".into()))?;

        // Write the request
        let request = format!("protocol=https\nhost={host}\n\n");
        std::io::Write::write_all(&mut stdin, request.as_bytes())?;
        drop(stdin);

        let output = child.wait_with_output()?;
        if !output.status.success() {
            // Credential helper not configured or failed - treat as no credentials
            return Ok(None);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut username = None;
        let mut password = None;

        for line in stdout.lines() {
            if let Some((key, value)) = line.split_once('=') {
                match key {
                    "username" => username = Some(value.to_string()),
                    "password" => password = Some(value.to_string()),
                    _ => {}
                }
            }
        }

        match (username, password) {
            (Some(u), Some(p)) => Ok(Some(GitCredentials::Https {
                username: u,
                token: p,
            })),
            _ => Ok(None),
        }
    }

    fn try_netrc(host: &str) -> Result<Option<GitCredentials>> {
        let netrc_path = Self::netrc_path()?;
        if !netrc_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&netrc_path)?;
        let netrc = Netrc::parse(&content)?;

        if let Some(machine) = netrc.find_machine(host) {
            return Ok(Some(GitCredentials::Https {
                username: machine.login.clone(),
                token: machine.password.clone(),
            }));
        }

        Ok(None)
    }

    fn netrc_path() -> Result<PathBuf> {
        if let Ok(path) = env::var("NETRC") {
            return Ok(PathBuf::from(path));
        }
        let home = env::var("HOME").map_err(|_| Error::Auth("HOME not set".into()))?;
        Ok(PathBuf::from(home).join(".netrc"))
    }
}

/// Parsed netrc file.
#[derive(Debug, Clone)]
pub struct Netrc {
    pub machines: Vec<NetrcMachine>,
}

#[derive(Debug, Clone)]
pub struct NetrcMachine {
    pub host: String,
    pub login: String,
    pub password: String,
}

impl Netrc {
    pub fn parse(content: &str) -> Result<Self> {
        let mut machines = Vec::new();
        let mut current_machine: Option<(String, Option<String>, Option<String>)> = None;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.is_empty() {
                continue;
            }

            let mut i = 0;
            while let Some(&token) = tokens.get(i) {
                let keyword = token.to_lowercase();
                match keyword.as_str() {
                    "machine" => {
                        if let Some((host, login, password)) = current_machine.take() {
                            if let (Some(login), Some(password)) = (login, password) {
                                machines.push(NetrcMachine {
                                    host,
                                    login,
                                    password,
                                });
                            }
                        }
                        if let Some(&host) = tokens.get(i + 1) {
                            current_machine = Some((host.to_string(), None, None));
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    "login" => {
                        if let Some(ref mut machine) = current_machine {
                            if let Some(&login) = tokens.get(i + 1) {
                                machine.1 = Some(login.to_string());
                                i += 2;
                                continue;
                            }
                        }
                        i += 1;
                    }
                    "password" => {
                        if let Some(ref mut machine) = current_machine {
                            if let Some(&password) = tokens.get(i + 1) {
                                machine.2 = Some(password.to_string());
                                i += 2;
                                continue;
                            }
                        }
                        i += 1;
                    }
                    _ => i += 1,
                }
            }
        }

        // Flush the last machine
        if let Some((host, login, password)) = current_machine {
            if let (Some(login), Some(password)) = (login, password) {
                machines.push(NetrcMachine {
                    host,
                    login,
                    password,
                });
            }
        }

        Ok(Self { machines })
    }

    fn find_machine(&self, host: &str) -> Option<&NetrcMachine> {
        self.machines.iter().find(|m| m.host == host)
    }
}

const ASKPASS_SCRIPT: &str = "#!/bin/sh\ncase \"$1\" in\n  Username*) printf '%s\\n' \"$TRUSS_GIT_USERNAME\" ;;\n  Password*) printf '%s\\n' \"$TRUSS_GIT_TOKEN\" ;;\n  *) exit 1 ;;\nesac\n";

/// Guard that keeps a private `GIT_ASKPASS` helper alive for the lifetime of a git command.
///
/// The helper script itself contains no credentials; it reads the username and token from
/// command-specific environment variables and is removed when this guard is dropped.
pub struct AskPassScript {
    path: tempfile::TempPath,
}

impl AskPassScript {
    fn new() -> Result<Self> {
        let mut builder = tempfile::Builder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(fs::Permissions::from_mode(0o700));
        }
        let mut file = builder
            .suffix(".sh")
            .tempfile()
            .map_err(|e| Error::Auth(format!("failed to create askpass script: {e}")))?;
        file.write_all(ASKPASS_SCRIPT.as_bytes())
            .map_err(|e| Error::Auth(format!("failed to write askpass script: {e}")))?;
        let path = file.into_temp_path();
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        self.path.as_ref()
    }
}

/// Apply credentials to a git command.
///
/// Returns an [`AskPassScript`] guard for HTTPS credentials. The guard must live until the
/// git command completes; dropping it removes the temporary helper.
pub fn apply_credentials(
    cmd: &mut std::process::Command,
    creds: &GitCredentials,
) -> Result<Option<AskPassScript>> {
    match creds {
        GitCredentials::Https { username, token } => {
            let askpass = AskPassScript::new()?;
            cmd.env("GIT_ASKPASS", askpass.path());
            cmd.env("TRUSS_GIT_USERNAME", username);
            cmd.env("TRUSS_GIT_TOKEN", token);
            cmd.env("GIT_TERMINAL_PROMPT", "0");
            Ok(Some(askpass))
        }
        GitCredentials::Ssh { key_path } => {
            if let Some(key) = key_path {
                let quoted = key
                    .to_string_lossy()
                    .replace('\'', "'\\''")
                    .replace('"', "\\\"");
                cmd.env("GIT_SSH_COMMAND", format!("ssh -i '{quoted}'"));
            }
            cmd.env("GIT_TERMINAL_PROMPT", "0");
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netrc_parse_valid() {
        let content = "machine github.com\n  login user\n  password token\n";
        let netrc = Netrc::parse(content).expect("parse");
        assert_eq!(netrc.machines.len(), 1);
        assert_eq!(netrc.machines[0].host, "github.com");
        assert_eq!(netrc.machines[0].login, "user");
        assert_eq!(netrc.machines[0].password, "token");
    }

    #[test]
    fn netrc_parse_multiple_machines() {
        let content = "machine github.com\n  login user1\n  password token1\nmachine gitlab.com\n  login user2\n  password token2\n";
        let netrc = Netrc::parse(content).expect("parse");
        assert_eq!(netrc.machines.len(), 2);
        assert_eq!(netrc.machines[0].host, "github.com");
        assert_eq!(netrc.machines[1].host, "gitlab.com");
    }

    #[test]
    fn netrc_find_machine() {
        let content = "machine github.com\n  login user\n  password token\n";
        let netrc = Netrc::parse(content).expect("parse");
        let machine = netrc.find_machine("github.com");
        assert!(machine.is_some());
        assert_eq!(machine.unwrap().login, "user");
    }

    #[test]
    fn netrc_ignore_comments() {
        let content = "# comment\nmachine github.com\n  login user\n  password token\n";
        let netrc = Netrc::parse(content).expect("parse");
        assert_eq!(netrc.machines.len(), 1);
    }

    #[test]
    fn extract_host_https() {
        let host =
            CredentialResolver::extract_host("https://github.com/repo.git").expect("extract");
        assert_eq!(host, "github.com");
    }

    #[test]
    fn extract_host_http() {
        let host = CredentialResolver::extract_host("http://gitlab.com/repo.git").expect("extract");
        assert_eq!(host, "gitlab.com");
    }

    #[test]
    fn extract_host_invalid() {
        assert!(CredentialResolver::extract_host("git@github.com:repo.git").is_err());
    }

    #[test]
    fn default_username_github() {
        let username = CredentialResolver::default_username("https://github.com/repo.git");
        assert_eq!(username, "x-access-token");
    }

    #[test]
    fn default_username_other() {
        let username = CredentialResolver::default_username("https://gitlab.com/repo.git");
        assert_eq!(username, "git");
    }

    #[test]
    fn looks_like_secret_token() {
        assert!(CredentialResolver::looks_like_secret(
            "ghp_1234567890abcdefghijklmnopqrstuvwx"
        ));
    }

    #[test]
    fn looks_like_secret_password() {
        assert!(CredentialResolver::looks_like_secret(
            "my-password-with-special-chars-123"
        ));
    }

    #[test]
    fn not_secret_short() {
        assert!(!CredentialResolver::looks_like_secret("short"));
    }

    #[test]
    fn not_secret_simple() {
        assert!(!CredentialResolver::looks_like_secret("not-a-secret"));
    }
}
