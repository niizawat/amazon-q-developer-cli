//! Custom Slash Commands integration tests
//! Note: Integration tests are temporarily commented out (due to complex Os initialization)

#[cfg(test)]
mod parser_tests {
    use super::super::parser::PromptProcessor;

    #[test]
    fn test_file_reference_extraction() {
        // Normal file reference patterns
        let content1 = "Please check @config.yaml for settings";
        let refs1 = PromptProcessor::extract_file_references(content1);
        assert_eq!(refs1, vec!["config.yaml"]);

        // File reference at line start
        let content2 = "@README.md contains important information";
        let refs2 = PromptProcessor::extract_file_references(content2);
        assert_eq!(refs2, vec!["README.md"]);

        // Multiple file references
        let content3 = "Check @src/main.rs and @tests/unit.rs for examples";
        let refs3 = PromptProcessor::extract_file_references(content3);
        assert_eq!(refs3, vec!["src/main.rs", "tests/unit.rs"]);

        // Email addresses should be excluded
        let content4 = "Contact admin@example.com or test@example.com for help";
        let refs4 = PromptProcessor::extract_file_references(content4);
        assert_eq!(refs4, Vec::<String>::new());

        // Mixed email addresses and valid file references
        let content5 = "Email test@example.com about @config/settings.json";
        let refs5 = PromptProcessor::extract_file_references(content5);
        assert_eq!(refs5, vec!["config/settings.json"]);

        // File references in quotes
        let content6 = "See '@data.csv' for example data";
        let refs6 = PromptProcessor::extract_file_references(content6);
        assert_eq!(refs6, vec!["data.csv"]);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use tokio::fs;

    #[tokio::test]
    async fn test_full_custom_command_workflow() {
        // Set up test directory
        let temp_dir = tempdir().unwrap();
        let commands_dir = temp_dir.path().join(".amazonq").join("commands");
        fs::create_dir_all(&commands_dir).await.unwrap();

        // Create test command file
        let test_command_content = r#"---
description: "Test workflow command"
argument-hint: "[message]"
allowed-tools: ["Bash"]
phase: "test"
---

# Test Workflow Command

This is a test command for the workflow.

## Input
Your message: $ARGUMENTS

## File Contents
Contents of test file: @test.txt

## Git Status
Current git status: !`echo "mock git status"`

## Action
Processing your request...
"#;

        // Create command file
        let command_file = commands_dir.join("test-workflow.md");
        fs::write(&command_file, test_command_content).await.unwrap();

        // Create test file
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "This is test content").await.unwrap();

        // Set up OS mock
        let mut os = crate::os::Os::default();
        os.env.set_current_dir(temp_dir.path()).unwrap();

        // Test custom command integration
        let integration = integration::CustomCommandIntegration::new()
            .with_security_mode(executor::SecurityMode::Permissive);

        // Check command existence
        assert!(integration.is_custom_command("test-workflow", &os).await);
        assert!(!integration.is_custom_command("nonexistent-command", &os).await);

        // Get command list
        let commands = integration.list_custom_commands(&os).await.unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "test-workflow");
        assert_eq!(commands[0].description, Some("Test workflow command".to_string()));

        // Test command preview
        let preview = integration.preview_command("test-workflow", &["Hello World"], &os).await.unwrap();
        assert!(preview.contains("Hello World"));
        assert!(preview.contains("test.txt"));
        assert!(preview.contains("echo \"mock git status\""));
    }

    #[tokio::test]
    async fn test_custom_command_security() {
        let temp_dir = tempdir().unwrap();
        let commands_dir = temp_dir.path().join(".amazonq").join("commands");
        fs::create_dir_all(&commands_dir).await.unwrap();

        // Create dangerous command
        let dangerous_command = r#"---
description: "Dangerous command"
---

# Dangerous Command

Execute: !`rm -rf /`
"#;

        let command_file = commands_dir.join("dangerous.md");
        fs::write(&command_file, dangerous_command).await.unwrap();

        let mut os = crate::os::Os::default();
        os.env.set_current_dir(temp_dir.path()).unwrap();

        // Test with strict mode
        let strict_integration = integration::CustomCommandIntegration::new()
            .with_security_mode(executor::SecurityMode::Strict);

        // Confirm security risks are detected in preview
        let preview = strict_integration.preview_command("dangerous", &[], &os).await.unwrap();
        assert!(preview.contains("Security warnings"));
        assert!(preview.contains("rm -rf"));

        // Test with permissive mode
        let permissive_integration = integration::CustomCommandIntegration::new()
            .with_security_mode(executor::SecurityMode::Permissive);
            
        // Permissive mode allows execution
        let permissive_preview = permissive_integration.preview_command("dangerous", &[], &os).await.unwrap();
        assert!(permissive_preview.contains("dangerous"));
    }

    #[tokio::test]
    async fn test_namespace_handling() {
        let temp_dir = tempdir().unwrap();
        let commands_dir = temp_dir.path().join(".amazonq").join("commands");
        
        // Test namespace with subdirectory
        let utils_dir = commands_dir.join("utils");
        fs::create_dir_all(&utils_dir).await.unwrap();

        let namespaced_command = r#"---
description: "Namespaced utility command"
---

# Utility Command

This is a utility command in a namespace.
"#;

        let command_file = utils_dir.join("helper.md");
        fs::write(&command_file, namespaced_command).await.unwrap();

        let mut os = crate::os::Os::default();
        os.env.set_current_dir(temp_dir.path()).unwrap();

        let integration = integration::CustomCommandIntegration::new();
        
        assert!(integration.is_custom_command("helper", &os).await);
        
        let commands = integration.list_custom_commands(&os).await.unwrap();
        let helper_cmd = commands.iter().find(|cmd| cmd.name == "helper").unwrap();
        assert_eq!(helper_cmd.namespace, Some("utils".to_string()));
    }

    #[tokio::test]
    async fn test_tsumiki_style_commands() {
        let temp_dir = tempdir().unwrap();
        let commands_dir = temp_dir.path().join(".amazonq").join("commands");
        fs::create_dir_all(&commands_dir).await.unwrap();

        // Create Tsumiki-style commands
        let tsumiki_commands = vec![
            ("kairo-requirements", "kairo", "Requirements definition"),
            ("tdd-red", "tdd", "Red phase of TDD"),
            ("rev-tasks", "rev", "Reverse engineering tasks"),
        ];

        for (name, phase, desc) in tsumiki_commands {
            let content = format!(r#"---
description: "{}"
phase: "{}"
---

# {} Command

This is a {} phase command.
"#, desc, phase, name, phase);

            let command_file = commands_dir.join(format!("{}.md", name));
            fs::write(&command_file, content).await.unwrap();
        }

        let mut os = crate::os::Os::default();
        os.env.set_current_dir(temp_dir.path()).unwrap();

        let integration = integration::CustomCommandIntegration::new();
        
        // Confirm all Tsumiki commands are recognized
        for (name, _, _) in [
            ("kairo-requirements", "kairo", "Requirements definition"),
            ("tdd-red", "tdd", "Red phase of TDD"), 
            ("rev-tasks", "rev", "Reverse engineering tasks"),
        ] {
            assert!(integration.is_custom_command(name, &os).await);
        }

        let commands = integration.list_custom_commands(&os).await.unwrap();
        assert_eq!(commands.len(), 3);

        // Confirm phase information is loaded correctly
        let kairo_cmd = commands.iter().find(|cmd| cmd.name == "kairo-requirements").unwrap();
        assert_eq!(kairo_cmd.phase, Some("kairo".to_string()));
    }

    #[tokio::test]
    async fn test_error_handling() {
        let temp_dir = tempdir().unwrap();
        let mut os = crate::os::Os::default();
        os.env.set_current_dir(temp_dir.path()).unwrap();

        let integration = integration::CustomCommandIntegration::new();
        
        // Test nonexistent command
        assert!(!integration.is_custom_command("nonexistent", &os).await);
        
        // Help display for nonexistent command
        let help_result = integration.show_custom_command_help(Some("nonexistent"), &os).await;
        assert!(help_result.is_err());
        
        // List display when no commands exist
        let commands = integration.list_custom_commands(&os).await.unwrap();
        assert!(commands.is_empty());
        
        let help_text = integration.show_custom_command_help(None, &os).await.unwrap();
        assert!(help_text.contains("No custom commands available"));
    }
}

#[cfg(test)]
mod unit_tests {
    use super::super::*;

    #[test]
    fn test_command_namespace_detection() {
        // Test CommandNamespace
        assert_eq!(
            CommandNamespace::from_command_name("kairo-requirements"),
            CommandNamespace::Kairo
        );
        assert_eq!(
            CommandNamespace::from_command_name("tdd-red"),
            CommandNamespace::Tdd
        );
        assert_eq!(
            CommandNamespace::from_command_name("rev-tasks"),
            CommandNamespace::Rev
        );
        assert_eq!(
            CommandNamespace::from_command_name("custom-helper"),
            CommandNamespace::Custom("custom".to_string())
        );
        assert_eq!(
            CommandNamespace::from_command_name("simple"),
            CommandNamespace::None
        );
    }

    #[test]
    fn test_command_scope() {
        // Test CommandScope
        let project_command = CustomCommand {
            name: "test".to_string(),
            content: "Test content".to_string(),
            frontmatter: None,
            scope: CommandScope::Project,
            file_path: PathBuf::from(".amazonq/commands/test.md"),
            namespace: None,
        };

        assert_eq!(project_command.scope, CommandScope::Project);

        let global_command = CustomCommand {
            name: "global-test".to_string(),
            content: "Global test content".to_string(),
            frontmatter: None,
            scope: CommandScope::Global,
            file_path: PathBuf::from("~/.aws/amazonq/commands/global-test.md"),
            namespace: None,
        };

        assert_eq!(global_command.scope, CommandScope::Global);
    }

    #[test]
    fn test_frontmatter_parsing() {
        // Test frontmatter parsing
        let frontmatter = CommandFrontmatter {
            allowed_tools: Some(vec!["Bash".to_string(), "Git".to_string()]),
            argument_hint: Some("[message]".to_string()),
            description: Some("Test command description".to_string()),
            model: Some("claude-3-5-sonnet-20241022".to_string()),
            phase: Some("kairo".to_string()),
            dependencies: Some(vec!["prerequisite-command".to_string()]),
            output_format: Some("markdown".to_string()),
        };

        assert_eq!(frontmatter.allowed_tools.as_ref().unwrap().len(), 2);
        assert_eq!(frontmatter.phase.as_ref().unwrap(), "kairo");
        assert!(frontmatter.dependencies.as_ref().unwrap().contains(&"prerequisite-command".to_string()));
    }

    #[test]
    fn test_custom_command_info_from_command() {
        // Test CustomCommandInfo conversion
        let frontmatter = CommandFrontmatter {
            description: Some("Test description".to_string()),
            argument_hint: Some("[test-arg]".to_string()),
            phase: Some("test".to_string()),
            allowed_tools: None,
            model: None,
            dependencies: None,
            output_format: None,
        };

        let command = CustomCommand {
            name: "test-cmd".to_string(),
            content: "Test content".to_string(),
            frontmatter: Some(frontmatter),
            scope: CommandScope::Project,
            file_path: PathBuf::from("test.md"),
            namespace: Some("utils".to_string()),
        };

        let info = integration::CustomCommandInfo::from_command(&command);
        
        assert_eq!(info.name, "test-cmd");
        assert_eq!(info.description, Some("Test description".to_string()));
        assert_eq!(info.argument_hint, Some("[test-arg]".to_string()));
        assert_eq!(info.scope, CommandScope::Project);
        assert_eq!(info.namespace, Some("utils".to_string()));
        assert_eq!(info.phase, Some("test".to_string()));
    }
}
