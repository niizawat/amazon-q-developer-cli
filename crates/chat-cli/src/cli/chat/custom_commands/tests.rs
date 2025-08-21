/// Custom Slash Commands機能の統合テスト
/// 注意: 統合テストは一時的にコメントアウト（Osの初期化が複雑なため）

#[cfg(test)]
mod parser_tests {
    use super::super::parser::PromptProcessor;

    #[test]
    fn test_file_reference_extraction() {
        // 正常なファイル参照パターン
        let content1 = "Please check @config.yaml for settings";
        let refs1 = PromptProcessor::extract_file_references(content1);
        assert_eq!(refs1, vec!["config.yaml"]);

        // 行頭のファイル参照
        let content2 = "@README.md contains important information";
        let refs2 = PromptProcessor::extract_file_references(content2);
        assert_eq!(refs2, vec!["README.md"]);

        // 複数のファイル参照
        let content3 = "Check @src/main.rs and @tests/unit.rs for examples";
        let refs3 = PromptProcessor::extract_file_references(content3);
        assert_eq!(refs3, vec!["src/main.rs", "tests/unit.rs"]);

        // メールアドレスは除外されるべき
        let content4 = "Contact admin@example.com or test@example.com for help";
        let refs4 = PromptProcessor::extract_file_references(content4);
        assert_eq!(refs4, Vec::<String>::new());

        // メールアドレスと正当なファイル参照の混在
        let content5 = "Email test@example.com about @config/settings.json";
        let refs5 = PromptProcessor::extract_file_references(content5);
        assert_eq!(refs5, vec!["config/settings.json"]);

        // 引用符内のファイル参照
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
        // テスト用ディレクトリ設定
        let temp_dir = tempdir().unwrap();
        let commands_dir = temp_dir.path().join(".amazonq").join("commands");
        fs::create_dir_all(&commands_dir).await.unwrap();

        // テストコマンドファイルを作成
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

        // コマンドファイルを作成
        let command_file = commands_dir.join("test-workflow.md");
        fs::write(&command_file, test_command_content).await.unwrap();

        // テスト用ファイルを作成
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "This is test content").await.unwrap();

        // OSモックを設定
        let mut os = crate::os::Os::default();
        os.env.set_current_dir(temp_dir.path()).unwrap();

        // カスタムコマンド統合をテスト
        let integration = integration::CustomCommandIntegration::new()
            .with_security_mode(executor::SecurityMode::Permissive);

        // コマンドの存在確認
        assert!(integration.is_custom_command("test-workflow", &os).await);
        assert!(!integration.is_custom_command("nonexistent-command", &os).await);

        // コマンド一覧の取得
        let commands = integration.list_custom_commands(&os).await.unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "test-workflow");
        assert_eq!(commands[0].description, Some("Test workflow command".to_string()));

        // コマンドプレビューのテスト
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

        // 危険なコマンドを作成
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

        // 厳格モードでテスト
        let strict_integration = integration::CustomCommandIntegration::new()
            .with_security_mode(executor::SecurityMode::Strict);

        // プレビューでセキュリティリスクが検出されることを確認
        let preview = strict_integration.preview_command("dangerous", &[], &os).await.unwrap();
        assert!(preview.contains("Security warnings"));
        assert!(preview.contains("rm -rf"));

        // 許可モードでテスト
        let permissive_integration = integration::CustomCommandIntegration::new()
            .with_security_mode(executor::SecurityMode::Permissive);
            
        // 許可モードでは実行可能
        let permissive_preview = permissive_integration.preview_command("dangerous", &[], &os).await.unwrap();
        assert!(permissive_preview.contains("dangerous"));
    }

    #[tokio::test]
    async fn test_claude_code_compatibility() {
        let temp_dir = tempdir().unwrap();
        
        // .claude/commands/ ディレクトリを作成（Claude Code互換性テスト）
        let claude_commands_dir = temp_dir.path().join(".claude").join("commands");
        fs::create_dir_all(&claude_commands_dir).await.unwrap();

        let claude_command = r#"---
description: "Claude Code compatible command"
---

# Claude Compatible Command

This command is in .claude/commands/ directory.
"#;

        let command_file = claude_commands_dir.join("claude-compat.md");
        fs::write(&command_file, claude_command).await.unwrap();

        let mut os = crate::os::Os::default();
        os.env.set_current_dir(temp_dir.path()).unwrap();

        let integration = integration::CustomCommandIntegration::new();
        
        // Claude Code形式のコマンドが読み込まれることを確認
        assert!(integration.is_custom_command("claude-compat", &os).await);
        
        let commands = integration.list_custom_commands(&os).await.unwrap();
        assert!(commands.iter().any(|cmd| cmd.name == "claude-compat"));
    }

    #[tokio::test]
    async fn test_namespace_handling() {
        let temp_dir = tempdir().unwrap();
        let commands_dir = temp_dir.path().join(".amazonq").join("commands");
        
        // サブディレクトリでネームスペースをテスト
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

        // Tsumikiスタイルのコマンドを作成
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
        
        // すべてのTsumikiコマンドが認識されることを確認
        for (name, _, _) in [
            ("kairo-requirements", "kairo", "Requirements definition"),
            ("tdd-red", "tdd", "Red phase of TDD"), 
            ("rev-tasks", "rev", "Reverse engineering tasks"),
        ] {
            assert!(integration.is_custom_command(name, &os).await);
        }

        let commands = integration.list_custom_commands(&os).await.unwrap();
        assert_eq!(commands.len(), 3);

        // フェーズ情報が正しく読み込まれることを確認
        let kairo_cmd = commands.iter().find(|cmd| cmd.name == "kairo-requirements").unwrap();
        assert_eq!(kairo_cmd.phase, Some("kairo".to_string()));
    }

    #[tokio::test]
    async fn test_error_handling() {
        let temp_dir = tempdir().unwrap();
        let mut os = crate::os::Os::default();
        os.env.set_current_dir(temp_dir.path()).unwrap();

        let integration = integration::CustomCommandIntegration::new();
        
        // 存在しないコマンドのテスト
        assert!(!integration.is_custom_command("nonexistent", &os).await);
        
        // 存在しないコマンドのヘルプ表示
        let help_result = integration.show_custom_command_help(Some("nonexistent"), &os).await;
        assert!(help_result.is_err());
        
        // コマンドがない場合のリスト表示
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
        // CommandNamespaceのテスト
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
        // CommandScopeのテスト
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
        // フロントマッターの解析テスト
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
        // CustomCommandInfoの変換テスト
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
