# Feature Specification: Template Marketplace

**Feature Branch**: `014-template-marketplace`

**Created**: 2026-07-17

**Status**: Draft

**Input**: User description: "Add a template marketplace to `truss` so users can discover, install, and update community template packs without manually entering git URLs or local paths."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Search and browse marketplace templates (Priority: P1)

A developer wants to find a pack for a specific use case. They run a marketplace search command with a keyword and see a list of templates with name, description, author, tags, and the source URL. They can install one directly from the results.

**Why this priority**: Discoverability is the core value of a marketplace. Without search/browse, the marketplace is just a list of URLs.

**Independent Test**: Run `truss marketplace search axum` and verify results are returned with enough metadata to choose a template.

**Acceptance Scenarios**:

1. **Given** a marketplace index containing a template tagged `web`, **when** the user searches for `web`, **then** the template appears in the results.
2. **Given** an empty or offline marketplace, **when** the user searches, **then** the command fails with a clear message and does not hang.
3. **Given** a search result, **when** the user chooses to install it, **then** the template is added to the local registry with the correct source URL.

---

### User Story 2 - Install a template from the marketplace (Priority: P1)

A developer finds a template in the marketplace and installs it to their local registry with a single command. The installed entry behaves like any other registry entry and can be used with `truss new` and `truss sync`.

**Why this priority**: Installation bridges discovery and scaffolding and must be as simple as using a local pack.

**Independent Test**: Run `truss marketplace install myorg/web-service` and verify `truss new app --template web-service` scaffolds from the installed source.

**Acceptance Scenarios**:

1. **Given** a marketplace template `web-service` pointing at a valid git URL, **when** the user installs it, **then** the local registry contains an entry named `web-service` with kind `git` and the resolved source URL.
2. **Given** a template already installed under the same name, **when** the user installs again without force, **then** the command fails clearly; with `--force`, the entry is replaced.
3. **Given** a marketplace template with optional subfolder or ref, **when** the user installs it, **then** those options are preserved in the registry entry.

---

### User Story 3 - Update installed marketplace templates (Priority: P2)

A developer wants to refresh installed templates to the latest marketplace versions. They run an update command and `truss` updates the local registry entries and clears or refreshes any cache.

**Why this priority**: Keeps installed templates current without manual re-installation. Not required for the first install.

**Independent Test**: Install a template, update the marketplace entry, run `truss marketplace update web-service`, and verify the registry entry reflects the new ref or source.

**Acceptance Scenarios**:

1. **Given** an installed template with a new version in the marketplace, **when** the user updates, **then** the registry entry is updated to the latest version or ref.
2. **Given** a locally modified registry entry for a marketplace template, **when** the user updates with `--force`, **then** local changes are replaced by the marketplace metadata.
3. **Given** no network connectivity, **when** the user updates, **then** the command fails with a typed network error and local entries remain unchanged.

---

### User Story 4 - List installed and available marketplace templates (Priority: P2)

A developer wants to see which templates are already installed and which are available. They run a marketplace list command with filters for installed, available, or by tag.

**Why this priority**: Helps users manage installed templates and discover new ones. Not required for basic install.

**Independent Test**: Run `truss marketplace list --installed` and verify only installed entries appear.

**Acceptance Scenarios**:

1. **Given** two installed and three available marketplace templates, **when** the user lists all, **then** all five are shown with installed status.
2. **Given** a filter `--tag rust`, **when** the user lists, **then** only templates tagged `rust` appear.

---

### User Story 5 - Publish or register a template in the marketplace (Priority: P3)

A pack author wants to make their template discoverable through the marketplace. They submit or register metadata (name, description, source URL, tags) so other users can find and install it.

**Why this priority**: Completes the marketplace lifecycle but depends on marketplace hosting choices and is not required for users to consume templates.

**Independent Test**: Run `truss marketplace publish /path/to/pack` and verify the marketplace index includes the submitted metadata after approval or validation.

**Acceptance Scenarios**:

1. **Given** a valid pack with a manifest or source URL, **when** the user publishes, **then** the marketplace receives the metadata and the template becomes searchable.
2. **Given** a pack with missing required metadata, **when** the user publishes, **then** the command fails and lists the missing fields.

### Edge Cases

- What happens when the marketplace index is unreachable or returns invalid data?
- What happens when a marketplace template name collides with a local registry entry?
- What happens when a marketplace entry references a private or inaccessible repository?
- What happens when a template is removed from the marketplace after it was installed locally?
- What happens when the same template has multiple versions in the marketplace?
- What happens when a user installs a template without a network connection?
- What happens when marketplace metadata includes a malformed source URL?

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST provide a marketplace command that can search available templates by keyword and tag.
- **FR-002**: System MUST display marketplace search results with name, description, author, tags, and source reference.
- **FR-003**: System MUST allow a user to install a marketplace template into the local registry.
- **FR-004**: System MUST allow a user to update installed marketplace templates from the marketplace index.
- **FR-005**: System MUST allow a user to list all marketplace templates with optional filters for installed status and tags.
- **FR-006**: System MUST resolve marketplace template sources using the existing registry `git` or `dir` entry types after installation.
- **FR-007**: System MUST support [NEEDS CLARIFICATION: the marketplace index format — is it a remote JSON index, a curated Git repository, or a local file?].
- **FR-008**: System MUST validate marketplace metadata (name, source URL, tags) before installation or publication.
- **FR-009**: System MUST support a `publish` or `register` subcommand that submits pack metadata to the marketplace.
- **FR-010**: System MUST fail closed with a typed error for network failures, invalid marketplace responses, and unreachable sources.
- **FR-011**: System MUST protect existing local registry entries from accidental overwrite by marketplace operations unless force is requested.
- **FR-012**: System MUST support deterministic ordering of search and list results.

### Key Entities

- **MarketplaceIndex**: A catalog of available templates. [NEEDS CLARIFICATION: hosted location and update mechanism]. Attributes: version, list of entries, last updated timestamp.
- **MarketplaceEntry**: A template listing in the marketplace. Attributes: name, display name, description, author, tags, source URL, kind, ref, subfolder, version.
- **InstalledMarketplaceEntry**: A marketplace entry that has been added to the local registry. Attributes: registry entry name, marketplace source, installed version or ref, last updated timestamp.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A developer can find and install a suitable template from the marketplace in under 2 minutes.
- **SC-002**: Installed marketplace templates work with `truss new` and `truss sync` without requiring the user to manually copy URLs.
- **SC-003**: Marketplace search returns relevant results for a keyword query with at least 80% top-5 relevance for common terms.
- **SC-004**: Update refreshes installed entries to the latest marketplace version without corrupting the local registry.
- **SC-005**: Invalid marketplace data or unreachable sources produce a clear error before any local changes.

## Assumptions

- The marketplace index is read-only for users and published by pack authors or a curator.
- Marketplace entries reference source URLs that `truss` already supports (git, local directory, etc.).
- Tags are plain strings without spaces and are case-insensitive for search.
- The marketplace command is layered on top of the existing registry system.
- Network connectivity is required for search, install, update, and publish; offline behavior is fail-closed.

## Out of Scope

- In-app payments or licensing for marketplace templates.
- User ratings, reviews, or download statistics in the first release.
- Automated security scanning or sandboxing of third-party templates.
- Peer-to-peer marketplace distribution without a central index.
