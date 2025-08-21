/// カスタムコマンドのローダー機能
/// 
/// ディレクトリからマークダウンファイルを読み込み、カスタムコマンドとして登録します。
/// Claude Code互換性のため .claude/commands/ もサポートします。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use futures::future::try_join_all;
use walkdir::WalkDir;

use crate::cli::chat::custom_commands::{
    CustomCommand, 
    CommandScope, 
    parser::MarkdownParser,
    error::CustomCommandError,
};
use crate::os::Os;


/// カスタムコマンドローダー
pub struct CustomCommandLoader {
    parser: MarkdownParser,
}

impl Default for CustomCommandLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl CustomCommandLoader {
    /// 新しいローダーを作成
    pub fn new() -> Self {
        Self {
            parser: MarkdownParser::new(),
        }
    }
    
    /// すべてのカスタムコマンドを読み込み
    pub async fn load_all_commands(&self, os: &Os) -> Result<HashMap<String, Arc<CustomCommand>>, CustomCommandError> {
        let mut commands = HashMap::new();
        
        // 各ディレクトリから並行してコマンドを読み込み
        let directories = self.get_command_directories(os)?;
        let futures: Vec<_> = directories.into_iter()
            .map(|(dir, scope)| self.load_commands_from_directory(dir, scope))
            .collect();
        
        let results = try_join_all(futures).await?;
        
        // 結果をマージ（プロジェクト > グローバルの優先順位）
        for dir_commands in results {
            for (name, command) in dir_commands {
                // プロジェクトコマンドが既に存在する場合、グローバルコマンドは無視
                if !commands.contains_key(&name) || command.scope == CommandScope::Project {
                    commands.insert(name, Arc::new(command));
                }
            }
        }
        
        tracing::info!("Loaded {} custom commands", commands.len());
        Ok(commands)
    }
    
    /// 指定ディレクトリからコマンドを読み込み
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
        
        // ディレクトリを再帰的に走査
        for entry in WalkDir::new(&dir_path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            
            // マークダウンファイルのみを処理
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
                    // 個別のファイル読み込みエラーは続行
                },
            }
        }
        
        tracing::debug!("Loaded {} commands from {}", commands.len(), dir_path.display());
        Ok(commands)
    }
    
    /// 単一ファイルからコマンドを読み込み
    pub async fn load_command_from_file(
        &self,
        file_path: &Path,
        base_dir: &Path,
        scope: CommandScope,
    ) -> Result<Option<CustomCommand>, CustomCommandError> {
        // ファイル名からコマンド名を取得
        let command_name = self.extract_command_name(file_path)?;
        
        // マークダウンファイルを解析
        let parsed = self.parser.parse_file(file_path).await?;
        
        // 名前空間を決定
        let namespace = self.extract_namespace(file_path, base_dir);
        
        let command = CustomCommand {
            name: command_name,
            content: parsed.content,
            frontmatter: parsed.frontmatter,
            scope,
            file_path: file_path.to_path_buf(),
            namespace,
        };
        
        // 基本的な検証
        self.validate_command(&command)?;
        
        Ok(Some(command))
    }
    
    /// コマンドディレクトリ一覧を取得
    fn get_command_directories(&self, os: &Os) -> Result<Vec<(PathBuf, CommandScope)>, CustomCommandError> {
        let mut directories = Vec::new();
        
        // プロジェクトディレクトリ（優先順位高）
        let project_dirs = vec![
            os.env.current_dir()?.join(".amazonq").join("commands"),  // Amazon Q標準
            os.env.current_dir()?.join(".claude").join("commands"),   // Claude Code互換
        ];
        
        for dir in project_dirs {
            if dir.exists() {
                directories.push((dir, CommandScope::Project));
            }
        }
        
        // グローバルディレクトリ
        if let Some(home) = os.env.home() {
            let global_dirs = vec![
                home.join(".aws").join("amazonq").join("commands"),  // Amazon Q標準
                home.join(".claude").join("commands"),               // Claude Code互換
            ];
            
            for dir in global_dirs {
                if dir.exists() {
                    directories.push((dir, CommandScope::Global));
                }
            }
        }
        
        Ok(directories)
    }
    
    /// ファイルパスからコマンド名を抽出
    fn extract_command_name(&self, file_path: &Path) -> Result<String, CustomCommandError> {
        file_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(|name| name.to_string())
            .ok_or_else(|| CustomCommandError::markdown_parse_error(
                file_path.to_path_buf(),
                "Invalid file name for command".to_string(),
            ))
    }
    
    /// ファイルパスから名前空間を抽出
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
    
    /// コマンドの基本検証
    fn validate_command(&self, command: &CustomCommand) -> Result<(), CustomCommandError> {
        // 名前の検証
        if command.name.is_empty() {
            return Err(CustomCommandError::config_error("Command name cannot be empty"));
        }
        
        // 名前に無効な文字が含まれていないかチェック
        if command.name.contains(char::is_whitespace) || command.name.contains('/') {
            return Err(CustomCommandError::config_error(
                format!("Invalid characters in command name: '{}'", command.name)
            ));
        }
        
        // コンテンツの検証
        if command.content.trim().is_empty() {
            return Err(CustomCommandError::config_error(
                format!("Command '{}' has empty content", command.name)
            ));
        }
        
        // セキュリティ検証（必要に応じて）
        if let Some(ref frontmatter) = command.frontmatter {
            // allowed-toolsにBashが含まれている場合のみBashコマンドチェックを実行
            if frontmatter.allowed_tools.as_ref()
                .map(|tools| tools.iter().any(|tool| tool.to_lowercase().contains("bash")))
                .unwrap_or(false)
            {
                // Bashコマンドを含む場合は追加の検証を実行
                crate::cli::chat::custom_commands::parser::PromptProcessor::validate_content(&command.content)?;
            }
        }
        
        Ok(())
    }
    
    /// コマンドのリロード
    pub async fn reload_command(
        &self,
        command_name: &str,
        os: &Os,
    ) -> Result<Option<CustomCommand>, CustomCommandError> {
        let directories = self.get_command_directories(os)?;
        
        // 各ディレクトリでコマンドファイルを検索
        for (dir, scope) in directories {
            let file_path = dir.join(format!("{}.md", command_name));
            if file_path.exists() {
                return self.load_command_from_file(&file_path, &dir, scope)
                    .await;
            }
        }
        
        Ok(None)
    }
    
    /// 利用可能なコマンド名一覧を取得（ファイルスキャンのみ）
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
    use super::*;
    use tempfile::tempdir;
    use std::io::Write;

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
        let result = loader.load_command_from_file(
            &file_path,
            temp_dir.path(),
            CommandScope::Project,
        ).await.unwrap();
        
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
