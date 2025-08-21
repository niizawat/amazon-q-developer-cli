/// Custom Slash Commands統合機能
/// 
/// 既存のSlashCommandシステムにカスタムコマンドを統合します。
/// 動的コマンドの処理とCLAPとの協調を担当します。

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::cli::chat::custom_commands::{
    CustomCommand,
    CommandScope,
    loader::CustomCommandLoader,
    executor::{CustomCommandExecutor, SecurityMode},
};
use crate::cli::chat::{ChatError, ChatSession, ChatState};
use crate::os::Os;

/// カスタムコマンド統合マネージャー
pub struct CustomCommandIntegration {
    loader: Arc<RwLock<CustomCommandLoader>>,
    executor: CustomCommandExecutor,
}

impl Default for CustomCommandIntegration {
    fn default() -> Self {
        Self::new()
    }
}

impl CustomCommandIntegration {
    /// 新しい統合マネージャーを作成
    pub fn new() -> Self {
        Self {
            loader: Arc::new(RwLock::new(CustomCommandLoader::new())),
            executor: CustomCommandExecutor::new()
                .with_security_mode(SecurityMode::Warning), // デフォルトは警告モード
        }
    }
    
    /// セキュリティモードを設定
    pub fn with_security_mode(mut self, mode: SecurityMode) -> Self {
        self.executor = self.executor.with_security_mode(mode);
        self
    }
    
    /// カスタムコマンドが存在するかチェック
    pub async fn is_custom_command(&self, command_name: &str, os: &Os) -> bool {
        let loader = self.loader.read().await;
        match loader.load_all_commands(os).await {
            Ok(commands) => commands.contains_key(command_name),
            Err(_) => false,
        }
    }
    
    /// カスタムコマンドを実行
    pub async fn execute_custom_command(
        &self,
        command_name: &str,
        args: &[String],
        os: &Os,
    ) -> Result<String, ChatError> {
        tracing::info!("Executing custom command: {} with args: {:?}", command_name, args);
        
        let loader = self.loader.read().await;
        
        // コマンドをロード
        let commands = loader.load_all_commands(os).await
            .map_err(|e| ChatError::Custom(format!("Failed to load commands: {}", e).into()))?;
        
        // コマンドを取得
        let command = commands.get(command_name)
            .ok_or_else(|| ChatError::Custom(format!("Command '{}' not found", command_name).into()))?;
        
        // フロントマッターから設定を取得
        if let Some(ref frontmatter) = command.frontmatter {
            // モデル設定
            if let Some(ref model) = frontmatter.model {
                tracing::info!("Custom command requests model: {}", model);
                // TODO: セッションのモデルを一時的に変更する機能を追加
            }
            
            // 許可ツール設定
            if let Some(ref allowed_tools) = frontmatter.allowed_tools {
                tracing::info!("Custom command allowed tools: {:?}", allowed_tools);
                // TODO: セッションの許可ツールを一時的に変更する機能を追加
            }
        }
        
        // コマンドを実行
        let result = self.executor.execute(&command, args, os)
            .await
            .map_err(|e| ChatError::Custom(format!("Custom command execution failed: {}", e).into()))?;
        
        Ok(result)
    }
    
    /// コマンド実行結果を処理
    async fn process_command_result(
        &self,
        result: String,
        _session: &mut ChatSession,
    ) -> Result<ChatState, ChatError> {
        // カスタムコマンドの結果をユーザー入力として扱う
        // これによりAIがカスタムコマンドの内容に基づいて応答する
        Ok(ChatState::HandleInput { input: result })
    }
    
    /// 利用可能なカスタムコマンド一覧を取得
    pub async fn list_custom_commands(&self, os: &Os) -> Result<Vec<CustomCommandInfo>, ChatError> {
        let loader = self.loader.read().await;
        
        // コマンドをロード
        let commands = loader.load_all_commands(os).await
            .map_err(|e| ChatError::Custom(format!("Failed to load commands: {}", e).into()))?;
        
        let mut command_infos = Vec::new();
        
        for (_, command) in commands {
            command_infos.push(CustomCommandInfo::from_command(&command));
        }
        
        Ok(command_infos)
    }
    
    /// カスタムコマンドのヘルプを表示
    pub async fn show_custom_command_help(
        &self,
        command_name: Option<&str>,
        os: &Os,
    ) -> Result<String, ChatError> {
        let loader = self.loader.read().await;
        
        // コマンドをロード
        let commands = loader.load_all_commands(os).await
            .map_err(|e| ChatError::Custom(format!("Failed to load commands: {}", e).into()))?;
        
        if let Some(name) = command_name {
            // 特定のコマンドのヘルプ
            let command = commands.get(name)
                .ok_or_else(|| ChatError::Custom(format!("Command '{}' not found", name).into()))?;
            
            Ok(self.format_command_help(&command))
        } else {
            // すべてのカスタムコマンドの一覧
            let commands = self.list_custom_commands(os).await?;
            Ok(self.format_commands_list(&commands))
        }
    }
    
    /// コマンドのヘルプをフォーマット
    fn format_command_help(&self, command: &CustomCommand) -> String {
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
    
    /// コマンド一覧をフォーマット
    fn format_commands_list(&self, commands: &[CustomCommandInfo]) -> String {
        if commands.is_empty() {
            return "No custom commands available. Create .md files in .amazonq/commands/ or .claude/commands/ to add custom commands.".to_string();
        }
        
        let mut output = Vec::new();
        output.push("🎯 Available Custom Commands:".to_string());
        output.push("".to_string());
        
        // 名前空間別にグループ化
        let mut namespaced_commands: std::collections::HashMap<String, Vec<&CustomCommandInfo>> = std::collections::HashMap::new();
        
        for cmd in commands {
            let namespace = cmd.namespace.clone().unwrap_or_else(|| "General".to_string());
            namespaced_commands.entry(namespace).or_default().push(cmd);
        }
        
        // 名前空間順に表示
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
                    
                    let description = cmd.description.as_ref()
                        .map(|d| format!(" - {}", d))
                        .unwrap_or_default();
                    
                    output.push(format!("  /{}{} {}{}", cmd.name, cmd.argument_hint.as_ref().map(|h| format!(" {}", h)).unwrap_or_default(), scope_indicator, description));
                }
                output.push("".to_string());
            }
        }
        
        output.push("💡 Use '/help <command>' for detailed help on a specific command.".to_string());
        output.join("\n")
    }
    
    /// コマンドプレビューを表示
    pub async fn preview_command(
        &self,
        command_name: &str,
        args: &[String],
        os: &Os,
    ) -> Result<String, ChatError> {
        let loader = self.loader.read().await;
        
        // コマンドをロード
        let commands = loader.load_all_commands(os).await
            .map_err(|e| ChatError::Custom(format!("Failed to load commands: {}", e).into()))?;
        
        // コマンドを取得
        let command = commands.get(command_name)
            .ok_or_else(|| ChatError::Custom(format!("Command '{}' not found", command_name).into()))?;
        
        // プレビューを生成（実際のコマンド実行はせずに処理後の内容を表示）
        let mut processed_content = command.content.clone();
        
        // 引数置換
        let args_str = args.join(" ");
        processed_content = processed_content.replace("$ARGUMENTS", &args_str);
        
        // プレビュー表示用のフォーマット
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
}

/// カスタムコマンド情報（表示用）
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

/// カスタムコマンドのインストール機能
pub struct CustomCommandInstaller;

impl CustomCommandInstaller {

    /// カスタムコマンドディレクトリを初期化
    pub async fn init_command_directory(os: &Os) -> Result<String, ChatError> {
        let commands_dir = os.env.current_dir()?.join(".amazonq").join("commands");
        
        if commands_dir.exists() {
            return Ok(format!("Custom commands directory already exists: {}", commands_dir.display()));
        }
        
        tokio::fs::create_dir_all(&commands_dir)
            .await
            .map_err(|e| ChatError::Custom(format!("Failed to create commands directory: {}", e).into()))?;
        
        // サンプルコマンドを作成
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
        
        Ok(format!("✅ Custom commands directory initialized: {}\n\n📝 Sample command created: sample-command.md\n\n💡 Create more .md files in this directory to add custom commands.", commands_dir.display()))
    }
}

/*
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
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
        
        let integration = CustomCommandIntegration::new();
        let mut os = crate::os::Os::default();
        os.env.set_current_dir(temp_dir.path()).unwrap();
        
        // コマンドの存在確認
        assert!(integration.is_custom_command("test-cmd", &os).await);
        assert!(!integration.is_custom_command("nonexistent", &os).await);
        
        // コマンド一覧の取得
        let commands = integration.list_custom_commands(&os).await.unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].name, "test-cmd");
    }
}
*/
