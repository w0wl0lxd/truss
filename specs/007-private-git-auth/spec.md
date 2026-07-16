# Feature Specification: Private Git Repository Authentication

**Feature Branch**: `007-private-git-auth`

**Created**: 2026-07-16

**Status**: Draft

**Input**: `truss` should be able to fetch template packs from private Git repositories by supporting common authentication methods (HTTPS tokens, SSH keys/agent, and credential helpers) without storing secrets in the registry file.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Clone a private HTTPS template with a token (Priority: P1)

A team stores its template pack in a private GitHub repository. A developer registers the repository with `truss registry add` and provides a personal access token via an environment variable. `truss new` clones the private repository and scaffolds the project.

**Why this priority**: Private HTTPS repos are the most common case for organizations that do not expose template packs publicly.

**Independent Test**: Register a `git` entry pointing at a private HTTPS repository, set an environment variable with a valid token, run `truss new`, and verify the project is scaffolded.

**Acceptance Scenarios**:

1. **Given** a `git` registry entry for a private HTTPS repository and a valid token in the expected environment variable, **when** the user runs `truss new`, **then** the repository is cloned and the project is scaffolded.
2. **Given** a private repository and a missing or invalid token, **when** the user runs `truss new`, **then** the command fails with a clear authentication error and does not write files.
3. **Given** a successful registration, **when** the registry file is inspected, **then** no token or secret appears in `registry.json`.

---

### User Story 2 - Use SSH keys or agent for private templates (Priority: P1)

A team uses SSH URLs for internal repositories and has `ssh-agent` configured. `truss` uses the agent or configured SSH keys to clone the private template without asking for a password.

**Why this priority**: SSH is the other dominant authentication transport for private repositories and should work out of the box on correctly configured systems.

**Independent Test**: Register a `git` entry with an SSH URL, ensure `ssh-agent` has the appropriate key, run `truss new`, and verify the clone succeeds.

**Acceptance Scenarios**:

1. **Given** a `git` entry with an SSH URL and a running `ssh-agent` that contains a valid key, **when** the user runs `truss new`, **then** the private repository is cloned successfully.
2. **Given** an SSH URL and no agent or key configured, **when** the user runs `truss new`, **then** the command fails with an actionable authentication error.
3. **Given** the user's `~/.ssh/config` defines a key or host alias for the target host, **when** `truss` resolves the URL, **then** the SSH configuration is honored.

---

### User Story 3 - Integrate with Git credential helpers and netrc (Priority: P2)

A developer already has Git credential helpers or a `~/.netrc` file configured for other tools. `truss` uses those existing credential sources so the user does not need to set a new environment variable.

**Why this priority**: Reusing existing Git/user credentials reduces friction and avoids duplicating secrets. It is a quality-of-life improvement over explicit token management.

**Independent Test**: Configure a Git credential helper or `~/.netrc` entry for a private HTTPS host, run `truss new` without setting a `truss`-specific token, and verify the clone succeeds.

**Acceptance Scenarios**:

1. **Given** a configured Git credential helper that returns a valid token for the repository host, **when** the user runs `truss new`, **then** `truss` uses the helper's token and the clone succeeds.
2. **Given** a `~/.netrc` entry with valid credentials for the repository host, **when** the user runs `truss new`, **then** the credentials are used.
3. **Given** a credential helper that returns no entry for the host, **when** the user runs `truss new`, **then** the command fails with a clear "no credentials" error.

### Edge Cases

- A token is supplied for an SSH URL: the token is ignored and the SSH transport is used.
- An SSH key is supplied for an HTTPS URL: the HTTPS URL is used with token/credential-helper logic, or fails if no HTTPS credentials are available.
- A repository URL redirects to a different host: credentials are sent only to the original host unless the redirect is to the same host.
- Multiple `truss` registry entries point to the same host but require different credentials. Per-entry credentials override per-host credentials.
- A token expires or is revoked between `truss new` and `truss sync`. The next operation fails with an authentication error and the cache is not updated.
- A credential helper writes a password prompt to `stderr`. `truss` does not forward interactive prompts and fails closed in non-interactive mode.
- A credential helper returns malformed output. The command fails with a clear error and does not leak the output.
- The registry file is copied to another machine. It contains no secrets, so authentication does not work until the new machine provides its own credentials.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST support private `git` registry entries by authenticating to the remote repository before cloning or fetching.
- **FR-002**: The system MUST support HTTPS authentication via a token or password supplied through an environment variable or CLI flag.
- **FR-003**: The system MUST support SSH authentication through the user's SSH agent (`SSH_AUTH_SOCK`) and standard OpenSSH key/configuration files.
- **FR-004**: The system MUST support Git credential helpers that follow the `git credential` protocol.
- **FR-005**: The system MUST support `netrc`-style host credentials from the user's home directory or platform-equivalent location.
- **FR-006**: The system MUST NOT persist tokens, passwords, or SSH keys in `registry.json`, cache directories, or any other project/config file.
- **FR-007**: The system MUST fail closed with a typed, actionable error when authentication fails, without falling back to an unauthenticated public attempt.
- **FR-008**: The system MUST NOT print secrets in error messages, logs, or `--dry-run` output.
- **FR-009**: The system MUST scope per-entry credentials higher than per-host credentials, with global environment variables as a fallback.

### Key Entities

- **GitCredentials**: The authentication material for a repository, including kind (token, password, ssh-agent, ssh-key) and source.
- **CredentialSource**: Where credentials come from (CLI argument, environment variable, Git credential helper, netrc file, SSH agent, SSH config).
- **CredentialResolver**: A component that selects the right credentials for a given repository URL and registry entry.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A developer can register and clone a private HTTPS template using a single environment variable in under 2 minutes.
- **SC-002**: Secret material never appears in `registry.json`, cache directories, or error output in 100% of test cases.
- **SC-003**: SSH-agent-based private repo access works out of the box on systems with a configured agent and valid key.
- **SC-004**: Authentication failures surface as clear, actionable errors that do not expose the secret.
- **SC-005**: `truss sync` against a private git template fetches updates using the same credentials without requiring re-entry.

## Assumptions

- Users manage secrets outside of `truss` via environment variables, credential helpers, SSH agents, or OS keychains.
- HTTPS token auth uses a username and token or password; for hosts like GitHub the username may be `x-access-token` or the user's name.
- SSH auth relies on the OpenSSH ecosystem (`ssh-agent`, `~/.ssh/config`, `known_hosts`).
- Public repositories continue to work without any authentication configuration.
- The project constitution applies: typed errors, no unsafe code, no secrets in the repository tree, and deterministic ordering.
- Secret authentication is scoped both per Git host and per registry entry, with per-entry credentials overriding per-host credentials and environment variables used as a final fallback.
- `truss` does not interactively prompt for passwords or tokens. Credentials must be supplied non-interactively via environment variables, CLI flags, Git credential helpers, netrc, or SSH agent/config.
- `truss` relies on the user's global Git configuration and the standard `git credential` protocol. Per-registry-entry credential helper names are out of scope for the first release.

## Out of Scope

- Storing encrypted credentials inside `truss` itself.
- Generating or rotating SSH keys or tokens.
- Authentication for non-Git remote sources (HTTP zip, custom registries).
- Interactive password prompts in non-TTY environments.
- Certificate-based or Kerberos authentication in the first release.
