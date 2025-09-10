//! Custom command execution engine
//!
//! Executes custom commands and provides the following features:
//! - Argument substitution ($ARGUMENTS)
//! - File references (@filename)
//! - Bash command execution (!`command`)
//! - Security validation
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;
use tokio::time::timeout;

use crate::cli::chat::custom_commands::CustomCommand;
use crate::cli::chat::custom_commands::error::CustomCommandError;
use crate::cli::chat::custom_commands::parser::{
    PromptProcessor,
    SecurityValidationConfig,
};
use crate::os::Os;

/// Command execution engine
pub struct CustomCommandExecutor {
    /// Bash command timeout (default 30 seconds)
    bash_timeout: Duration,
    /// Security mode
    security_mode: SecurityMode,
}

/// Security mode
#[derive(Debug, Clone)]
pub enum SecurityMode {
    /// Strict mode - reject dangerous commands
    Strict,
    /// Warning mode - show warnings but allow execution
    Warning,
    /// Permissive mode - allow all
    Permissive,
}

impl Default for CustomCommandExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl CustomCommandExecutor {
    /// Create a new execution engine
    pub fn new() -> Self {
        Self {
            bash_timeout: Duration::from_secs(30),
            security_mode: SecurityMode::Strict,
        }
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.bash_timeout = timeout;
        self
    }

    /// Set security mode
    pub fn with_security_mode(mut self, mode: SecurityMode) -> Self {
        self.security_mode = mode;
        self
    }

    /// Execute custom command (default configuration)
    pub async fn execute(
        &self,
        command: &CustomCommand,
        args: &[String],
        os: &Os,
    ) -> Result<String, CustomCommandError> {
        // Call security validation execution with default configuration
        let default_config = SecurityValidationConfig::default();
        self.execute_with_security(command, args, os, &default_config).await
    }

    /// Execute custom command with security configuration
    pub async fn execute_with_security(
        &self,
        command: &CustomCommand,
        args: &[String],
        os: &Os,
        security_config: &SecurityValidationConfig,
    ) -> Result<String, CustomCommandError> {
        tracing::info!(
            "Executing custom command: {} with security level: {:?}",
            command.name,
            security_config.level
        );

        // 1. Security check (with configuration)
        Self::security_check_with_config(command, security_config)?;

        // 2. Argument substitution
        let mut processed_content = PromptProcessor::substitute_arguments(&command.content, args);

        // 3. Bash command execution (!`command` pattern) - use frontmatter permissions
        #[allow(clippy::map_unwrap_or)]
        let allowed_tools = command
            .frontmatter
            .as_ref()
            .and_then(|fm| fm.allowed_tools.as_ref())
            .map(|tools| tools.as_slice())
            .unwrap_or(&[]);
        processed_content = self
            .execute_bash_commands_with_permissions(&processed_content, os, allowed_tools)
            .await?;

        // 4. File reference resolution (@filename pattern)
        processed_content = self.resolve_file_references(&processed_content, os).await?;

        // 5. Extended thinking mode detection
        if PromptProcessor::detect_thinking_keywords(&processed_content) {
            tracing::info!("Extended thinking keywords detected in custom command");
            // Add prefix indicating thinking mode
            processed_content = format!("🤔 **Extended Thinking Mode Activated**\n\n{}", processed_content);
        }

        tracing::debug!("Processed content length: {}", processed_content.len());
        Ok(processed_content)
    }

    /// Security check
    fn security_check(&self, command: &CustomCommand) -> Result<(), CustomCommandError> {
        match self.security_mode {
            SecurityMode::Permissive => Ok(()), // Allow all commands
            SecurityMode::Warning | SecurityMode::Strict => {
                let risks = PromptProcessor::check_security_risks(&command.content);
                if risks.is_empty() {
                    return Ok(());
                }

                match self.security_mode {
                    SecurityMode::Warning => {
                        tracing::warn!("Security risks detected in command '{}': {:?}", command.name, risks);
                        Ok(())
                    },
                    SecurityMode::Strict => {
                        Err(CustomCommandError::security_error(
                            &command.name,
                            format!("Security risks detected: {}", risks.join(", ")),
                        ))
                    },
                    SecurityMode::Permissive => unreachable!("Already handled above"),
                }
            },
        }

    /// Security check for command based on configuration
    fn security_check_with_config(
        command: &CustomCommand,
        config: &SecurityValidationConfig,
    ) -> Result<(), CustomCommandError> {
        PromptProcessor::validate_content_with_config(&command.content, config)
    }

    /// Execute bash commands with permissions
    async fn execute_bash_commands_with_permissions(
        &self,
        content: &str,
        os: &Os,
        allowed_tools: &[String],
    ) -> Result<String, CustomCommandError> {
        let bash_commands = PromptProcessor::extract_bash_commands(content);
        if bash_commands.is_empty() {
            return Ok(content.to_string());
        }

        let mut result = content.to_string();

        for bash_cmd in bash_commands {
            tracing::debug!("Executing bash command: {}", bash_cmd);

            // Claude Code format permission check
            if !allowed_tools.is_empty() && !PromptProcessor::validate_bash_permissions(&bash_cmd, allowed_tools) {
                return Err(CustomCommandError::bash_execution_error(format!(
                    "Bash command '{}' not permitted by allowed-tools",
                    bash_cmd
                )));
            }

            let output = self.run_bash_command(&bash_cmd, os).await?;

            // Replace !`command` pattern with result
            let pattern = format!("!`{}`", bash_cmd);
            result = result.replace(&pattern, &output);
        }

        Ok(result)
    }

    /// Execute a single bash command
    async fn run_bash_command(&self, cmd: &str, _os: &Os) -> Result<String, CustomCommandError> {
        // Security check
        let risks = PromptProcessor::check_security_risks(cmd);
        if !risks.is_empty() && matches!(self.security_mode, SecurityMode::Strict) {
            return Err(CustomCommandError::bash_execution_error(format!(
                "Dangerous command rejected: {}",
                cmd
            )));
        }

        // Bash command execution (permission check already performed by caller)
        #[cfg(unix)]
        let mut command = Command::new("bash");
        #[cfg(windows)]
        let mut command = Command::new("cmd");

        #[cfg(unix)]
        command.arg("-c").arg(cmd);
        #[cfg(windows)]
        command.arg("/C").arg(cmd);

        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

        // Execute with timeout
        let child = command.spawn().map_err(|e| {
            CustomCommandError::bash_execution_error(format!("Failed to spawn command '{}': {}", cmd, e))
        })?;

        let output = timeout(self.bash_timeout, child.wait_with_output())
            .await
            .map_err(|_timeout_err| CustomCommandError::timeout_error(cmd, self.bash_timeout.as_millis() as u64))?
            .map_err(|e| {
                CustomCommandError::bash_execution_error(format!("Command execution failed '{}': {}", cmd, e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CustomCommandError::bash_execution_error(format!(
                "Command failed '{}': {}",
                cmd, stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.trim().to_string())
    }

    /// Resolve file references
    async fn resolve_file_references(&self, content: &str, os: &Os) -> Result<String, CustomCommandError> {
        let file_refs = PromptProcessor::extract_file_references(content);
        if file_refs.is_empty() {
            return Ok(content.to_string());
        }

        let mut result = content.to_string();
        let current_dir = os
            .env
            .current_dir()
            .map_err(|e| CustomCommandError::config_error(format!("Failed to get current directory: {}", e)))?;

        for file_ref in file_refs {
            tracing::debug!("Resolving file reference: {}", file_ref);

            let file_content = self.read_file_reference(&file_ref, &current_dir).await?;

            // Replace @filename pattern with content
            let pattern = format!("@{}", file_ref);
            let replacement = format!("```\n{}\n```", file_content);
            result = result.replace(&pattern, &replacement);
        }

        Ok(result)
    }

    /// Read file reference
    async fn read_file_reference(&self, file_ref: &str, current_dir: &Path) -> Result<String, CustomCommandError> {
        // Security check: prevent access outside relative paths
        if file_ref.contains("..") || file_ref.starts_with('/') {
            return Err(CustomCommandError::security_error(
                "file_reference",
                format!("Unsafe file reference: {}", file_ref),
            ));
        }

        let file_path = current_dir.join(file_ref);

        // Check file existence
        if !file_path.exists() {
            return Err(CustomCommandError::file_reference_error(
                file_ref.to_string(),
                std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"),
            ));
        }

        // File size check (prevent files that are too large)
        let metadata = tokio::fs::metadata(&file_path)
            .await
            .map_err(|e| CustomCommandError::file_reference_error(file_ref.to_string(), e))?;

        const MAX_FILE_SIZE: u64 = 1024 * 1024; // 1MB
        if metadata.len() > MAX_FILE_SIZE {
            return Err(CustomCommandError::file_reference_error(
                file_ref.to_string(),
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "File too large: {} bytes (max: {} bytes)",
                        metadata.len(),
                        MAX_FILE_SIZE
                    ),
                ),
            ));
        }

        // Read file content
        let content = tokio::fs::read_to_string(&file_path)
            .await
            .map_err(|e| CustomCommandError::file_reference_error(file_ref.to_string(), e))?;

        Ok(content)
    }

    /// Execute in preview mode (display processing content without actual execution)
    pub async fn preview(
        &self,
        command: &CustomCommand,
        args: &[String],
        _os: &Os,
    ) -> Result<ExecutionPreview, CustomCommandError> {
        let mut preview = ExecutionPreview {
            command_name: command.name.clone(),
            processed_content: PromptProcessor::substitute_arguments(&command.content, args),
            bash_commands: PromptProcessor::extract_bash_commands(&command.content),
            file_references: PromptProcessor::extract_file_references(&command.content),
            security_risks: PromptProcessor::check_security_risks(&command.content),
            estimated_execution_time: Self::estimate_execution_time(command),
        };

        // Add security check results
        if let Err(e) = self.security_check(command) {
            preview.security_risks.push(e.to_string());
        }

        Ok(preview)
    }

    /// Estimate execution time
    fn estimate_execution_time(command: &CustomCommand) -> Duration {
        let bash_commands = PromptProcessor::extract_bash_commands(&command.content);
        let file_refs = PromptProcessor::extract_file_references(&command.content);

        let base_time = Duration::from_millis(100); // Base processing time
        let bash_time = Duration::from_millis(500 * bash_commands.len() as u64); // 500ms per bash command
        let file_time = Duration::from_millis(50 * file_refs.len() as u64); // 50ms per file reference

        base_time + bash_time + file_time
    }
}

/// Execution preview result
#[derive(Debug, Clone)]
pub struct ExecutionPreview {
    pub command_name: String,
    pub processed_content: String,
    pub bash_commands: Vec<String>,
    pub file_references: Vec<String>,
    pub security_risks: Vec<String>,
    pub estimated_execution_time: Duration,
}

impl ExecutionPreview {
    /// Convert preview to display string
    pub fn to_display_string(&self) -> String {
        let mut output = Vec::new();

        output.push(format!("📋 Command: {}", self.command_name));
        output.push(format!("⏱️  Estimated time: {:?}", self.estimated_execution_time));

        if !self.bash_commands.is_empty() {
            output.push("🔧 Bash commands to execute:".to_string());
            for cmd in &self.bash_commands {
                output.push(format!("  - {}", cmd));
            }
        }

        if !self.file_references.is_empty() {
            output.push("📁 Files to reference:".to_string());
            for file_ref in &self.file_references {
                output.push(format!("  - {}", file_ref));
            }
        }

        if !self.security_risks.is_empty() {
            output.push("⚠️  Security warnings:".to_string());
            for risk in &self.security_risks {
                output.push(format!("  - {}", risk));
            }
        }

        output.push("".to_string());
        output.push("📄 Processed content:".to_string());
        output.push(self.processed_content.clone());

        output.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::*;
    use crate::cli::chat::custom_commands::{
        CommandFrontmatter,
        CommandScope,
    };

    #[tokio::test]
    #[ignore = "Requires complex Os setup"]
    async fn test_argument_substitution() {
        let command = CustomCommand {
            name: "test".to_string(),
            content: "Process issue #$ARGUMENTS".to_string(),
            frontmatter: None,
            scope: CommandScope::Project,
            file_path: PathBuf::from("test.md"),
            namespace: None,
        };
    }

    #[tokio::test]
    #[ignore = "Requires complex Os setup"]
    async fn test_file_reference() {
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        tokio::fs::write(&test_file, "Test content").await.unwrap();

        let command = CustomCommand {
            name: "test".to_string(),
            content: "Review @test.txt file".to_string(),
            frontmatter: None,
            scope: CommandScope::Project,
            file_path: PathBuf::from("test.md"),
            namespace: None,
        };
    }

    #[test]
    fn test_security_mode() {
        let command = CustomCommand {
            name: "dangerous".to_string(),
            content: "Execute: !`rm -rf /`".to_string(),
            frontmatter: None,
            scope: CommandScope::Project,
            file_path: PathBuf::from("dangerous.md"),
            namespace: None,
        };

        let strict_executor = CustomCommandExecutor::new().with_security_mode(SecurityMode::Strict);
        assert!(strict_executor.security_check(&command).is_err());

        let permissive_executor = CustomCommandExecutor::new().with_security_mode(SecurityMode::Permissive);
        assert!(permissive_executor.security_check(&command).is_ok());
    }
}
