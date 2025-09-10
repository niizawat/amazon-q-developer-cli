//! Custom Slash Commands functionality implementation
//!
//! This functionality supports:
//! - Loading custom commands from markdown files
//! - Parsing frontmatter (YAML)
//! - Argument substitution ($ARGUMENTS)
//! - File references (@filename)
//! - Bash command execution (!command)

#![allow(dead_code)]

pub mod error;
pub mod executor;
pub mod integration;
pub mod loader;
pub mod parser;

// Tests are defined in tests.rs file

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{
    Deserialize,
    Serialize,
};

use crate::os::Os;

/// Custom command definition
#[derive(Debug, Clone)]
pub struct CustomCommand {
    /// Command name (filename without extension)
    pub name: String,
    /// Command content (markdown)
    pub content: String,
    /// Frontmatter (metadata)
    pub frontmatter: Option<CommandFrontmatter>,
    /// Whether it's a project command or global command
    pub scope: CommandScope,
    /// Command file path
    pub file_path: PathBuf,
    /// Namespace (classification by directory)
    pub namespace: Option<String>,
}

/// Command scope
#[derive(Debug, Clone, PartialEq)]
pub enum CommandScope {
    /// Project-specific commands (.amazonq/commands/)
    Project,
    /// User global commands (~/.aws/amazonq/commands/)
    Global,
}

/// Command frontmatter (YAML)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandFrontmatter {
    /// Allowed tools
    #[serde(rename = "allowed-tools")]
    pub allowed_tools: Option<Vec<String>>,

    /// Argument hint
    #[serde(rename = "argument-hint")]
    pub argument_hint: Option<String>,

    /// Command description
    pub description: Option<String>,

    /// Model to use
    pub model: Option<String>,

    /// Tsumiki compatible: development phase
    pub phase: Option<String>,

    /// Tsumiki compatible: dependent commands
    pub dependencies: Option<Vec<String>>,

    /// Tsumiki compatible: output format
    #[serde(rename = "output-format")]
    pub output_format: Option<String>,
}

/// Namespaced command information
#[derive(Debug, Clone)]
pub struct NamespacedCommand {
    pub namespace: CommandNamespace,
    pub base_name: String,
    pub command: Arc<CustomCommand>,
}

/// Command namespace
#[derive(Debug, Clone, PartialEq)]
pub enum CommandNamespace {
    /// Tsumiki Kairo flow
    Kairo,
    /// Tsumiki TDD flow
    Tdd,
    /// Tsumiki reverse engineering
    Rev,
    /// Other custom namespace
    Custom(String),
    /// No namespace
    None,
}

impl CommandNamespace {
    /// Infer namespace from command name
    pub fn from_command_name(name: &str) -> Self {
        if name.starts_with("kairo-") {
            Self::Kairo
        } else if name.starts_with("tdd-") {
            Self::Tdd
        } else if name.starts_with("rev-") {
            Self::Rev
        } else if let Some(prefix) = name.split('-').next() {
            if prefix != name {
                Self::Custom(prefix.to_string())
            } else {
                Self::None
            }
        } else {
            Self::None
        }
    }

    /// Display name of the namespace
    pub fn display_name(&self) -> &str {
        match self {
            Self::Kairo => "kairo",
            Self::Tdd => "tdd",
            Self::Rev => "rev",
            Self::Custom(name) => name,
            Self::None => "",
        }
    }
}

/// Custom command cache
#[derive(Debug)]
pub struct CustomCommandCache {
    /// Map of command name -> command definition
    commands: HashMap<String, Arc<CustomCommand>>,
    /// Last scan time
    last_scan: std::time::Instant,
    /// Scan interval
    scan_interval: std::time::Duration,
}

impl Default for CustomCommandCache {
    fn default() -> Self {
        Self {
            commands: HashMap::new(),
            last_scan: std::time::Instant::now(),
            scan_interval: std::time::Duration::from_secs(30), // 30 second interval
        }
    }
}

impl CustomCommandCache {
    /// Create a new cache
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if rescan is needed
    pub fn needs_rescan(&self) -> bool {
        self.last_scan.elapsed() > self.scan_interval
    }

    /// Get command (rescan if needed)
    pub async fn get_command(&mut self, name: &str, os: &Os) -> Option<Arc<CustomCommand>> {
        if self.needs_rescan() {
            if let Err(e) = self.refresh(os).await {
                tracing::warn!("Failed to refresh custom commands: {}", e);
            }
        }
        self.commands.get(name).cloned()
    }

    /// Get all command names
    pub fn command_names(&self) -> Vec<String> {
        self.commands.keys().cloned().collect()
    }

    /// Refresh cache
    pub async fn refresh(&mut self, os: &Os) -> Result<(), error::CustomCommandError> {
        let loader = loader::CustomCommandLoader::new();
        self.commands = loader.load_all_commands(os).await?;
        self.last_scan = std::time::Instant::now();
        Ok(())
    }

    /// Manually add command
    pub fn add_command(&mut self, command: CustomCommand) {
        let name = command.name.clone();
        self.commands.insert(name, Arc::new(command));
    }

    /// Remove command
    pub fn remove_command(&mut self, name: &str) -> Option<Arc<CustomCommand>> {
        self.commands.remove(name)
    }
}

/// Custom command manager
pub struct CustomCommandManager {
    cache: CustomCommandCache,
}

impl CustomCommandManager {
    /// Create a new manager
    pub fn new() -> Self {
        Self {
            cache: CustomCommandCache::new(),
        }
    }

    /// Execute command
    pub async fn execute_command(
        &mut self,
        command_name: &str,
        args: &[String],
        os: &Os,
    ) -> Result<String, error::CustomCommandError> {
        let command = self
            .cache
            .get_command(command_name, os)
            .await
            .ok_or_else(|| error::CustomCommandError::CommandNotFound(command_name.to_string()))?;

        let executor = executor::CustomCommandExecutor::new();
        executor.execute(&command, args, os).await
    }

    /// Get list of available commands
    pub async fn list_commands(&mut self, os: &Os) -> Result<Vec<String>, error::CustomCommandError> {
        if self.cache.needs_rescan() {
            self.cache.refresh(os).await?;
        }
        Ok(self.cache.command_names())
    }

    /// Get command details
    pub async fn get_command_info(
        &mut self,
        command_name: &str,
        os: &Os,
    ) -> Result<Arc<CustomCommand>, error::CustomCommandError> {
        self.cache
            .get_command(command_name, os)
            .await
            .ok_or_else(|| error::CustomCommandError::CommandNotFound(command_name.to_string()))
    }
}

impl Default for CustomCommandManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_namespace_detection() {
        assert_eq!(
            CommandNamespace::from_command_name("kairo-requirements"),
            CommandNamespace::Kairo
        );
        assert_eq!(CommandNamespace::from_command_name("tdd-red"), CommandNamespace::Tdd);
        assert_eq!(CommandNamespace::from_command_name("rev-tasks"), CommandNamespace::Rev);
        assert_eq!(
            CommandNamespace::from_command_name("custom-command"),
            CommandNamespace::Custom("custom".to_string())
        );
        assert_eq!(CommandNamespace::from_command_name("simple"), CommandNamespace::None);
    }

    #[test]
    fn test_command_scope() {
        let project_command = CustomCommand {
            name: "test".to_string(),
            content: "Test command".to_string(),
            frontmatter: None,
            scope: CommandScope::Project,
            file_path: PathBuf::from(".amazonq/commands/test.md"),
            namespace: None,
        };

        assert_eq!(project_command.scope, CommandScope::Project);
    }
}
