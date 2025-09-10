//! Custom command loader functionality
//!
//! Loads markdown files from directories and registers them as custom commands.
use std::collections::HashMap;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;

use futures::future::try_join_all;
use walkdir::WalkDir;

use crate::cli::chat::custom_commands::error::CustomCommandError;
use crate::cli::chat::custom_commands::parser::MarkdownParser;
use crate::cli::chat::custom_commands::{
    CommandScope,
    CustomCommand,
};
use crate::os::Os;

/// Custom command loader
pub struct CustomCommandLoader {
    parser: MarkdownParser,
}

impl Default for CustomCommandLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl CustomCommandLoader {
    /// Create a new loader
    pub fn new() -> Self {
        Self {
            parser: MarkdownParser::new(),
        }
    }

    /// Load all custom commands
    pub async fn load_all_commands(&self, os: &Os) -> Result<HashMap<String, Arc<CustomCommand>>, CustomCommandError> {
        let mut commands = HashMap::new();

        // Load commands from each directory in parallel
        let directories = self.get_command_directories(os)?;
        let futures: Vec<_> = directories
            .into_iter()
            .map(|(dir, scope)| self.load_commands_from_directory(dir, scope))
            .collect();

        let results = try_join_all(futures).await?;

        // Merge results (Project > Global priority)
        for dir_commands in results {
            for (name, command) in dir_commands {
                // If a project command already exists, ignore global commands
                if !commands.contains_key(&name) || command.scope == CommandScope::Project {
                    commands.insert(name, Arc::new(command));
                }
            }
        }

        tracing::info!("Loaded {} custom commands", commands.len());
        Ok(commands)
    }

    /// Load commands from specified directory
    pub async fn load_commands_from_directory(
        &self,
        dir_path: PathBuf,
        scope: CommandScope,
    ) -> Result<HashMap<String, CustomCommand>, CustomCommandError> {
        let mut commands = HashMap::new();

        if !dir_path.exists() {
            tracing::debug!("Command directory does not exist: {}", dir_path.display());
            return Ok(commands);
        }

        // Recursively traverse directory
        for entry in WalkDir::new(&dir_path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Process only markdown files
            if !MarkdownParser::is_markdown_file(path) {
                continue;
            }

            match self.load_command_from_file(path, &dir_path, scope.clone()).await {
                Ok(Some(command)) => {
                    let name = command.name.clone();
                    if commands.contains_key(&name) {
                        tracing::warn!("Duplicate command name '{}' found in {}", name, path.display());
                    }
                    commands.insert(name, command);
                },
                Ok(None) => {
                    tracing::debug!("Skipped file: {}", path.display());
                },
                Err(e) => {
                    tracing::error!("Failed to load command from {}: {}", path.display(), e);
                    // Continue on individual file loading errors
                },
            }
        }

        tracing::debug!("Loaded {} commands from {}", commands.len(), dir_path.display());
        Ok(commands)
    }

    /// Load command from single file
    pub async fn load_command_from_file(
        &self,
        file_path: &Path,
        base_dir: &Path,
        scope: CommandScope,
    ) -> Result<Option<CustomCommand>, CustomCommandError> {
        // Get command name from filename
        let command_name = self.extract_command_name(file_path)?;

        // Parse markdown file
        let parsed = self.parser.parse_file(file_path).await?;

        // Determine namespace
        let namespace = self.extract_namespace(file_path, base_dir);

        let command = CustomCommand {
            name: command_name,
            content: parsed.content,
            frontmatter: parsed.frontmatter,
            scope,
            file_path: file_path.to_path_buf(),
            namespace,
        };

        // Basic validation
        self.validate_command(&command)?;

        Ok(Some(command))
    }

    /// Get list of command directories
    #[allow(clippy::unused_self)]
    fn get_command_directories(&self, os: &Os) -> Result<Vec<(PathBuf, CommandScope)>, CustomCommandError> {
        let mut directories = Vec::new();

        // Project directory (high priority)
        let project_dir = os.env.current_dir()?.join(".amazonq").join("commands");
        if project_dir.exists() {
            directories.push((project_dir, CommandScope::Project));
        }

        // Global directory
        if let Some(home) = os.env.home() {
            let global_dir = home.join(".aws").join("amazonq").join("commands");
            if global_dir.exists() {
                directories.push((global_dir, CommandScope::Global));
            }
        }

        Ok(directories)
    }

    /// Extract command name from file path
    #[allow(clippy::unused_self)]
    fn extract_command_name(&self, file_path: &Path) -> Result<String, CustomCommandError> {
        file_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(|name| name.to_string())
            .ok_or_else(|| {
                CustomCommandError::markdown_parse_error(
                    file_path.to_path_buf(),
                    "Invalid file name for command".to_string(),
                )
            })
    }

    /// Extract namespace from file path
    #[allow(clippy::unused_self)]
    fn extract_namespace(&self, file_path: &Path, base_dir: &Path) -> Option<String> {
        if let Ok(relative_path) = file_path.strip_prefix(base_dir) {
            if let Some(parent) = relative_path.parent() {
                if parent != Path::new("") {
                    return Some(parent.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "_"));
                }
            }
        }
        None
    }

    /// Basic command validation
    #[allow(clippy::unused_self)]
    fn validate_command(&self, command: &CustomCommand) -> Result<(), CustomCommandError> {
        // Name validation
        if command.name.is_empty() {
            return Err(CustomCommandError::config_error("Command name cannot be empty"));
        }

        // Check for invalid characters in name
        if command.name.contains(char::is_whitespace) || command.name.contains('/') {
            return Err(CustomCommandError::config_error(format!(
                "Invalid characters in command name: '{}'",
                command.name
            )));
        }

        // Content validation
        if command.content.trim().is_empty() {
            return Err(CustomCommandError::config_error(format!(
                "Command '{}' has empty content",
                command.name
            )));
        }

        // Security validation (if needed)
        if let Some(ref frontmatter) = command.frontmatter {
            // Execute bash command check only if allowed-tools contains Bash
            if frontmatter
                .allowed_tools
                .as_ref()
                .is_some_and(|tools| tools.iter().any(|tool| tool.to_lowercase().contains("bash")))
            {
                // Execute additional validation if contains bash commands
                crate::cli::chat::custom_commands::parser::PromptProcessor::validate_content(&command.content)?;
            }
        }

        Ok(())
    }

    /// Reload command
    pub async fn reload_command(
        &self,
        command_name: &str,
        os: &Os,
    ) -> Result<Option<CustomCommand>, CustomCommandError> {
        let directories = self.get_command_directories(os)?;

        // Search for command files in each directory
        for (dir, scope) in directories {
            let file_path = dir.join(format!("{}.md", command_name));
            if file_path.exists() {
                return self.load_command_from_file(&file_path, &dir, scope).await;
            }
        }

        Ok(None)
    }

    /// Get list of available command names (file scan only)
    pub async fn list_available_commands(&self, os: &Os) -> Result<Vec<String>, CustomCommandError> {
        let directories = self.get_command_directories(os)?;
        let mut command_names = Vec::new();

        for (dir, _scope) in directories {
            for entry in WalkDir::new(&dir)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if MarkdownParser::is_markdown_file(path) {
                    if let Ok(name) = self.extract_command_name(path) {
                        if !command_names.contains(&name) {
                            command_names.push(name);
                        }
                    }
                }
            }
        }

        command_names.sort();
        Ok(command_names)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn test_load_command_from_file() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("test-command.md");

        let content = r#"---
description: "Test command"
---

# Test Command
This is a test command."#;

        std::fs::write(&file_path, content).unwrap();

        let loader = CustomCommandLoader::new();
        let result = loader
            .load_command_from_file(&file_path, temp_dir.path(), CommandScope::Project)
            .await
            .unwrap();

        assert!(result.is_some());
        let command = result.unwrap();
        assert_eq!(command.name, "test-command");
        assert!(command.frontmatter.is_some());
    }

    #[test]
    fn test_extract_command_name() {
        let loader = CustomCommandLoader::new();

        let path = PathBuf::from("/path/to/my-command.md");
        let name = loader.extract_command_name(&path).unwrap();
        assert_eq!(name, "my-command");
    }

    #[test]
    fn test_extract_namespace() {
        let loader = CustomCommandLoader::new();

        let base_dir = PathBuf::from("/commands");
        let file_path = PathBuf::from("/commands/utils/helper.md");

        let namespace = loader.extract_namespace(&file_path, &base_dir);
        assert_eq!(namespace, Some("utils".to_string()));
    }
}
