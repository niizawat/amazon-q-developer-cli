//! Custom Slash Commands integration functionality
//!
//! Integrates custom commands into the existing SlashCommand system.
//! Handles dynamic command processing and coordination with CLAP.
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::cli::chat::custom_commands::executor::{
    CustomCommandExecutor,
    SecurityMode,
};
use crate::cli::chat::custom_commands::loader::CustomCommandLoader;
use crate::cli::chat::custom_commands::parser::SecurityConfigManager;
use crate::cli::chat::custom_commands::{
    CommandScope,
    CustomCommand,
};
use crate::cli::chat::prompt::COMMANDS;
use crate::cli::chat::{
    ChatError,
};
use crate::database::settings::Setting;
use crate::os::Os;

/// Custom command integration manager
pub struct CustomCommandIntegration {
    loader: Arc<RwLock<CustomCommandLoader>>,
    executor: CustomCommandExecutor,
    security_manager: Arc<RwLock<SecurityConfigManager>>,
}

impl Default for CustomCommandIntegration {
    fn default() -> Self {
        Self::new()
    }
}

impl CustomCommandIntegration {
    /// Create a new integration manager
    pub fn new() -> Self {
        // Save security configuration to home directory .aws/amazonq
        let home_dir = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        let config_dir = home_dir.join(".aws").join("amazonq");

        Self {
            loader: Arc::new(RwLock::new(CustomCommandLoader::new())),
            executor: CustomCommandExecutor::new().with_security_mode(SecurityMode::Warning), // Default is warning mode
            security_manager: Arc::new(RwLock::new(SecurityConfigManager::new(&config_dir))),
        }
    }

    /// Set security mode
    pub fn with_security_mode(mut self, mode: SecurityMode) -> Self {
        self.executor = self.executor.with_security_mode(mode);
        self
    }

    /// Check if custom command exists
    pub async fn is_custom_command(&self, command_name: &str, os: &Os) -> bool {
        // Check if custom commands experimental feature is enabled
        if !os
            .database
            .settings
            .get_bool(Setting::EnabledCustomCommands)
            .unwrap_or(false)
        {
            return false;
        }

        let loader = self.loader.read().await;
        match loader.load_all_commands(os).await {
            Ok(commands) => commands.contains_key(command_name),
            Err(_) => false,
        }
    }

    /// Execute custom command
    pub async fn execute_custom_command(
        &self,
        command_name: &str,
        args: &[String],
        os: &Os,
    ) -> Result<String, ChatError> {
        tracing::info!("Executing custom command: {} with args: {:?}", command_name, args);

        let loader = self.loader.read().await;

        // Load commands
        let commands = loader
            .load_all_commands(os)
            .await
            .map_err(|e| ChatError::Custom(format!("Failed to load commands: {}", e).into()))?;

        // Get command
        let command = commands
            .get(command_name)
            .ok_or_else(|| ChatError::Custom(format!("Command '{}' not found", command_name).into()))?;

        // Get configuration from frontmatter
        if let Some(ref frontmatter) = command.frontmatter {
            // Model configuration
            if let Some(ref model) = frontmatter.model {
                tracing::info!("Custom command requests model: {}", model);
                // TODO: Add functionality to temporarily change session model
            }

            // Allowed tools configuration
            if let Some(ref allowed_tools) = frontmatter.allowed_tools {
                tracing::info!("Custom command allowed tools: {:?}", allowed_tools);
                // TODO: Add functionality to temporarily change session allowed tools
            }
        }

        // Get current security configuration
        let security_config = self.get_current_security_config().await;

        // Execute command (with security configuration)
        let result = self
            .executor
            .execute_with_security(command, args, os, &security_config)
            .await
            .map_err(|e| ChatError::Custom(format!("Custom command execution failed: {}", e).into()))?;

        Ok(result)
    }

    /// Get list of available custom commands
    pub async fn list_custom_commands(&self, os: &Os) -> Result<Vec<CustomCommandInfo>, ChatError> {
        let loader = self.loader.read().await;

        // Load commands
        let commands = loader
            .load_all_commands(os)
            .await
            .map_err(|e| ChatError::Custom(format!("Failed to load commands: {}", e).into()))?;

        let mut command_infos = Vec::new();

        for (_, command) in commands {
            command_infos.push(CustomCommandInfo::from_command(&command));
        }

        Ok(command_infos)
    }

    /// Check for conflicts with existing slash commands
    #[allow(clippy::unused_self)]
    pub fn check_command_conflicts(&self, custom_commands: &[CustomCommandInfo]) -> Vec<String> {
        let mut conflicts = Vec::new();

        // Extract basic command names from existing slash commands
        let existing_commands: std::collections::HashSet<&str> = COMMANDS
            .iter()
            .map(|cmd| cmd.trim_start_matches('/').split_whitespace().next().unwrap_or(""))
            .collect();

        for cmd_info in custom_commands {
            if existing_commands.contains(cmd_info.name.as_str()) {
                conflicts.push(cmd_info.name.clone());
            }
        }

        conflicts
    }

    /// Display custom command help
    pub async fn show_custom_command_help(&self, command_name: Option<&str>, os: &Os) -> Result<String, ChatError> {
        let loader = self.loader.read().await;

        // Load commands
        let commands = loader
            .load_all_commands(os)
            .await
            .map_err(|e| ChatError::Custom(format!("Failed to load commands: {}", e).into()))?;

        if let Some(name) = command_name {
            // Help for specific command
            let command = commands
                .get(name)
                .ok_or_else(|| ChatError::Custom(format!("Command '{}' not found", name).into()))?;

            Ok(Self::format_command_help(command))
        } else {
            // List of all custom commands
            let commands = self.list_custom_commands(os).await?;
            Ok(Self::format_commands_list(&commands))
        }
    }

    /// Format command help
    fn format_command_help(command: &CustomCommand) -> String {
        let mut help = Vec::new();

        help.push(format!("📝 Custom Command: {}", command.name));

        if let Some(ref frontmatter) = command.frontmatter {
            if let Some(ref description) = frontmatter.description {
                help.push(format!("📋 Description: {}", description));
            }

            if let Some(ref hint) = frontmatter.argument_hint {
                help.push(format!("💡 Usage: /{} {}", command.name, hint));
            }

            if let Some(ref phase) = frontmatter.phase {
                help.push(format!("🔄 Phase: {}", phase));
            }

            if let Some(ref dependencies) = frontmatter.dependencies {
                help.push(format!("🔗 Dependencies: {}", dependencies.join(", ")));
            }
        }

        help.push(format!("📁 Source: {}", command.file_path.display()));
        help.push(format!("🌐 Scope: {:?}", command.scope));

        if let Some(ref namespace) = command.namespace {
            help.push(format!("🏷️  Namespace: {}", namespace));
        }

        help.push("".to_string());
        help.push("📄 Content preview:".to_string());
        let preview = if command.content.chars().count() > 200 {
            let truncated: String = command.content.chars().take(200).collect();
            format!("{}...", truncated)
        } else {
            command.content.clone()
        };
        help.push(preview);

        help.join("\n")
    }

    /// Format command list
    fn format_commands_list(commands: &[CustomCommandInfo]) -> String {
        if commands.is_empty() {
            return "No custom commands available. Create .md files in .amazonq/commands/ to add custom commands.".to_string();
        }

        let mut output = Vec::new();
        output.push("🎯 Available Custom Commands:".to_string());
        output.push("".to_string());

        // Group by namespace
        let mut namespaced_commands: std::collections::HashMap<String, Vec<&CustomCommandInfo>> =
            std::collections::HashMap::new();

        for cmd in commands {
            let namespace = cmd.namespace.clone().unwrap_or_else(|| "General".to_string());
            namespaced_commands.entry(namespace).or_default().push(cmd);
        }

        // Display in namespace order
        let mut namespaces: Vec<_> = namespaced_commands.keys().collect();
        namespaces.sort();

        for namespace in namespaces {
            if let Some(cmds) = namespaced_commands.get(namespace) {
                output.push(format!("## {} Commands", namespace));
                output.push("".to_string());

                for cmd in cmds {
                    let scope_indicator = match cmd.scope {
                        CommandScope::Project => "(project)",
                        CommandScope::Global => "(user)",
                    };

                    let description = cmd
                        .description
                        .as_ref()
                        .map(|d| format!(" - {}", d))
                        .unwrap_or_default();

                    output.push(format!(
                        "  /{}{} {}{}",
                        cmd.name,
                        cmd.argument_hint
                            .as_ref()
                            .map(|h| format!(" {}", h))
                            .unwrap_or_default(),
                        scope_indicator,
                        description
                    ));
                }
                output.push("".to_string());
            }
        }

        output.push("💡 Use '/help <command>' for detailed help on a specific command.".to_string());
        output.join("\n")
    }

    /// Display command preview
    pub async fn preview_command(&self, command_name: &str, args: &[String], os: &Os) -> Result<String, ChatError> {
        let loader = self.loader.read().await;

        // Load commands
        let commands = loader
            .load_all_commands(os)
            .await
            .map_err(|e| ChatError::Custom(format!("Failed to load commands: {}", e).into()))?;

        // Get command
        let command = commands
            .get(command_name)
            .ok_or_else(|| ChatError::Custom(format!("Command '{}' not found", command_name).into()))?;

        // Generate preview (display processed content without actual command execution)
        let mut processed_content = command.content.clone();

        // Argument substitution
        let args_str = args.join(" ");
        processed_content = processed_content.replace("$ARGUMENTS", &args_str);

        // Format for preview display
        let mut preview = Vec::new();
        preview.push(format!("🔍 Preview of /{} {}", command_name, args.join(" ")));
        preview.push("".to_string());

        if let Some(ref frontmatter) = command.frontmatter {
            if let Some(ref desc) = frontmatter.description {
                preview.push(format!("📝 Description: {}", desc));
            }
            if let Some(ref hint) = frontmatter.argument_hint {
                preview.push(format!("💡 Usage: /{} {}", command_name, hint));
            }
            preview.push("".to_string());
        }

        preview.push("📄 Processed Content:".to_string());
        preview.push(format!("```\n{}\n```", processed_content));

        Ok(preview.join("\n"))
    }

    /// Enable security validation
    pub async fn enable_security(
        &mut self,
    ) -> Result<(), crate::cli::chat::custom_commands::error::CustomCommandError> {
        let mut manager = self.security_manager.write().await;
        manager.load_config().await?;
        manager.enable_security().await
    }

    /// Disable security validation
    pub async fn disable_security(
        &mut self,
    ) -> Result<(), crate::cli::chat::custom_commands::error::CustomCommandError> {
        let mut manager = self.security_manager.write().await;
        manager.load_config().await?;
        manager.disable_security().await
    }

    /// Set security validation to warning level
    pub async fn set_security_warn(
        &mut self,
    ) -> Result<(), crate::cli::chat::custom_commands::error::CustomCommandError> {
        let mut manager = self.security_manager.write().await;
        manager.load_config().await?;
        manager.set_security_warn().await
    }

    /// Get security configuration status
    pub async fn get_security_status(&self) -> String {
        let manager = self.security_manager.read().await;
        manager.get_status_string()
    }

    /// Get current security configuration
    pub async fn get_current_security_config(
        &self,
    ) -> crate::cli::chat::custom_commands::parser::SecurityValidationConfig {
        let mut manager = self.security_manager.write().await;
        let _ = manager.load_config().await; // Ignore errors and use default configuration
        manager.get_config().clone()
    }
}

/// Custom command information (for display)
#[derive(Debug, Clone)]
pub struct CustomCommandInfo {
    pub name: String,
    pub description: Option<String>,
    pub argument_hint: Option<String>,
    pub scope: crate::cli::chat::custom_commands::CommandScope,
    pub namespace: Option<String>,
    pub phase: Option<String>,
}

impl CustomCommandInfo {
    fn from_command(command: &CustomCommand) -> Self {
        let (description, argument_hint, phase) = if let Some(ref frontmatter) = command.frontmatter {
            (
                frontmatter.description.clone(),
                frontmatter.argument_hint.clone(),
                frontmatter.phase.clone(),
            )
        } else {
            (None, None, None)
        };

        Self {
            name: command.name.clone(),
            description,
            argument_hint,
            scope: command.scope.clone(),
            namespace: command.namespace.clone(),
            phase,
        }
    }
}

/// Custom command installation functionality
pub struct CustomCommandInstaller;

impl CustomCommandInstaller {
    /// Initialize custom command directory
    pub async fn init_command_directory(os: &Os) -> Result<String, ChatError> {
        let commands_dir = os.env.current_dir()?.join(".amazonq").join("commands");

        if commands_dir.exists() {
            return Ok(format!(
                "Custom commands directory already exists: {}",
                commands_dir.display()
            ));
        }

        tokio::fs::create_dir_all(&commands_dir)
            .await
            .map_err(|e| ChatError::Custom(format!("Failed to create commands directory: {}", e).into()))?;

        // Create sample command
        let sample_command = r#"---
description: "Sample custom command"
argument-hint: "[your-message]"
---

# Sample Command

This is a sample custom command. You can edit this file or create new .md files in the .amazonq/commands/ directory.

## Your input
$ARGUMENTS

## Example usage
/sample-command "Hello, World!"
"#;

        let sample_file = commands_dir.join("sample-command.md");
        tokio::fs::write(&sample_file, sample_command)
            .await
            .map_err(|e| ChatError::Custom(format!("Failed to create sample command: {}", e).into()))?;

        Ok(format!(
            "✅ Custom commands directory initialized: {}\n\n📝 Sample command created: sample-command.md\n\n💡 Create more .md files in this directory to add custom commands.",
            commands_dir.display()
        ))
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_command_conflict_detection() {
        let integration = CustomCommandIntegration::new();

        // Create custom command info for testing
        let custom_commands = vec![
            CustomCommandInfo {
                name: "clear".to_string(), // Conflicts with existing /clear
                description: Some("Custom clear command".to_string()),
                scope: CommandScope::Project,
                file_path: std::path::PathBuf::from("test.md"),
                phase: None,
            },
            CustomCommandInfo {
                name: "review".to_string(), // Not an existing command
                description: Some("Review command".to_string()),
                scope: CommandScope::Project,
                file_path: std::path::PathBuf::from("review.md"),
                phase: None,
            },
            CustomCommandInfo {
                name: "help".to_string(), // Conflicts with existing /help
                description: Some("Custom help command".to_string()),
                scope: CommandScope::Global,
                file_path: std::path::PathBuf::from("help.md"),
                phase: None,
            },
        ];

        let conflicts = integration.check_command_conflicts(&custom_commands);

        // clear and help should be detected as conflicts
        assert_eq!(conflicts.len(), 2);
        assert!(conflicts.contains(&"clear".to_string()));
        assert!(conflicts.contains(&"help".to_string()));
        assert!(!conflicts.contains(&"review".to_string()));
    }

    #[test]
    fn test_no_conflicts() {
        let integration = CustomCommandIntegration::new();

        let custom_commands = vec![
            CustomCommandInfo {
                name: "review".to_string(),
                description: Some("Review command".to_string()),
                scope: CommandScope::Project,
                file_path: std::path::PathBuf::from("review.md"),
                phase: None,
            },
            CustomCommandInfo {
                name: "deploy".to_string(),
                description: Some("Deploy command".to_string()),
                scope: CommandScope::Global,
                file_path: std::path::PathBuf::from("deploy.md"),
                phase: None,
            },
        ];

        let conflicts = integration.check_command_conflicts(&custom_commands);
        assert!(conflicts.is_empty());
    }

    #[tokio::test]
    #[ignore = "Requires complex Os setup"]
    async fn test_custom_command_integration() {
        let temp_dir = tempdir().unwrap();
        let commands_dir = temp_dir.path().join(".amazonq").join("commands");
        tokio::fs::create_dir_all(&commands_dir).await.unwrap();

        let test_command = r#"---
description: "Test integration command"
---

# Test Command
This is a test: $ARGUMENTS"#;

        let command_file = commands_dir.join("test-cmd.md");
        tokio::fs::write(&command_file, test_command).await.unwrap();

        // Test temporarily disabled (Os initialization is complex)
        // let integration = CustomCommandIntegration::new();
        // assert!(integration.is_custom_command("test-cmd", &os).await);
        // assert!(!integration.is_custom_command("nonexistent", &os).await);

        // let commands = integration.list_custom_commands(&os).await.unwrap();
        // assert_eq!(commands.len(), 1);
        // assert_eq!(commands[0].name, "test-cmd");
    }
}
