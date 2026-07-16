use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("template error: {0}")]
    Template(#[from] minijinja::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML error: {0}")]
    Toml(#[from] toml_edit::TomlError),

    #[error("embedded template is not valid UTF-8")]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error("project directory not available for this platform")]
    ProjectDir,

    #[error("template {0:?} not found")]
    TemplateNotFound(String),

    #[error("empty registry")]
    EmptyRegistry,

    #[error("validation failed: {0}")]
    Validation(String),

    #[error("invalid argument: {0}")]
    Argument(String),

    #[error("unsupported template kind for {0}")]
    UnsupportedKind(String),

    #[error("git is not installed or not on PATH")]
    GitNotInstalled,

    #[error("git command failed: {0}")]
    Git(String),

    #[error("git ref {0:?} not found")]
    MissingRef(String),

    #[error("invalid git URL: {0}")]
    InvalidGitUrl(String),

    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("no credentials found for {0}")]
    NoCredentials(String),

    #[error("invalid credential source: {0}")]
    InvalidCredentialSource(String),

    #[error("netrc parse error: {0}")]
    Netrc(String),

    #[error("update conflict: {0}")]
    UpdateConflict(String),

    #[error("marketplace error: {0}")]
    Marketplace(String),

    #[error("network error: {0}")]
    Network(String),
}

pub type Result<T> = std::result::Result<T, Error>;
