use std::path::PathBuf;

/// Custom command functionality error definitions
use thiserror::Error;

/// Errors related to custom commands
#[derive(Error, Debug)]
pub enum CustomCommandError {
    /// Command not found
    #[error("Custom command '{0}' not found")]
    CommandNotFound(String),

    /// File read error
    #[error("Failed to read command file '{path}': {source}")]
    FileReadError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Directory access error
    #[error("Failed to access directory '{path}': {source}")]
    DirectoryError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Markdown parsing error
    #[error("Failed to parse markdown file '{path}': {message}")]
    MarkdownParseError { path: PathBuf, message: String },

    /// Frontmatter parsing error
    #[error("Failed to parse frontmatter in '{path}': {source}")]
    FrontmatterParseError {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },

    /// Command execution error
    #[error("Failed to execute custom command '{command}': {message}")]
    ExecutionError { command: String, message: String },

    /// Argument processing error
    #[error("Invalid arguments for command '{command}': {message}")]
    ArgumentError { command: String, message: String },

    /// File reference error
    #[error("Failed to resolve file reference '{file}': {source}")]
    FileReferenceError {
        file: String,
        #[source]
        source: std::io::Error,
    },

    /// Bash command execution error
    #[error("Failed to execute bash command: {message}")]
    BashExecutionError { message: String },

    /// Security error
    #[error("Security violation in command '{command}': {message}")]
    SecurityError { command: String, message: String },

    /// Dependency error
    #[error("Dependency error for command '{command}': missing '{dependency}'")]
    DependencyError { command: String, dependency: String },

    /// Configuration error
    #[error("Configuration error: {message}")]
    ConfigError { message: String },

    /// Timeout error
    #[error("Command '{command}' timed out after {timeout_ms}ms")]
    TimeoutError { command: String, timeout_ms: u64 },

    /// General I/O error
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// JSON parsing error
    #[error("JSON parse error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// YAML parsing error
    #[error("YAML parse error: {0}")]
    YamlError(#[from] serde_yaml::Error),

    /// Other errors
    #[error("Unknown error: {0}")]
    Other(String),
}

impl CustomCommandError {
    /// Create file read error
    pub fn file_read_error(path: PathBuf, source: std::io::Error) -> Self {
        Self::FileReadError { path, source }
    }

    /// Create directory error
    pub fn directory_error(path: PathBuf, source: std::io::Error) -> Self {
        Self::DirectoryError { path, source }
    }

    /// Create markdown parsing error
    pub fn markdown_parse_error(path: PathBuf, message: impl Into<String>) -> Self {
        Self::MarkdownParseError {
            path,
            message: message.into(),
        }
    }

    /// Create frontmatter parsing error
    pub fn frontmatter_parse_error(path: PathBuf, source: serde_yaml::Error) -> Self {
        Self::FrontmatterParseError { path, source }
    }

    /// Create command execution error
    pub fn execution_error(command: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ExecutionError {
            command: command.into(),
            message: message.into(),
        }
    }

    /// Create argument error
    pub fn argument_error(command: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ArgumentError {
            command: command.into(),
            message: message.into(),
        }
    }

    /// Create file reference error
    pub fn file_reference_error(file: impl Into<String>, source: std::io::Error) -> Self {
        Self::FileReferenceError {
            file: file.into(),
            source,
        }
    }

    /// Create bash execution error
    pub fn bash_execution_error(message: impl Into<String>) -> Self {
        Self::BashExecutionError {
            message: message.into(),
        }
    }

    /// Create security error
    pub fn security_error(command: impl Into<String>, message: impl Into<String>) -> Self {
        Self::SecurityError {
            command: command.into(),
            message: message.into(),
        }
    }

    /// Create dependency error
    pub fn dependency_error(command: impl Into<String>, dependency: impl Into<String>) -> Self {
        Self::DependencyError {
            command: command.into(),
            dependency: dependency.into(),
        }
    }

    /// Create configuration error
    pub fn config_error(message: impl Into<String>) -> Self {
        Self::ConfigError {
            message: message.into(),
        }
    }

    /// Create timeout error
    pub fn timeout_error(command: impl Into<String>, timeout_ms: u64) -> Self {
        Self::TimeoutError {
            command: command.into(),
            timeout_ms,
        }
    }

    /// Determine if error is fatal
    pub fn is_fatal(&self) -> bool {
        match self {
            Self::CommandNotFound(_) => false,   // Command not found is not fatal
            Self::ArgumentError { .. } => false, // Argument errors are fixable
            Self::SecurityError { .. } => true,  // Security errors are fatal
            Self::ConfigError { .. } => true,    // Configuration errors are fatal
            _ => false,
        }
    }

    /// Get user-facing message
    pub fn user_message(&self) -> String {
        match self {
            Self::CommandNotFound(cmd) => {
                format!(
                    "Command '{}' not found. Run '/help' to see available commands.",
                    cmd
                )
            },
            Self::ArgumentError { command, message } => {
                format!("Invalid arguments for command '{}': {}", command, message)
            },
            Self::FileReferenceError { file, .. } => {
                format!(
                    "Failed to read file '{}'. Please check the file path.",
                    file
                )
            },
            Self::SecurityError { command, message } => {
                format!(
                    "Cannot execute command '{}' for security reasons: {}",
                    command, message
                )
            },
            _ => self.to_string(),
        }
    }
}

/// Result type alias
pub type Result<T> = std::result::Result<T, CustomCommandError>;
