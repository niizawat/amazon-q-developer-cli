/// Custom Slash Commands機能の実装
/// 
/// この機能は以下をサポートします：
/// - マークダウンファイルからのカスタムコマンド読み込み
/// - フロントマッター（YAML）の解析
/// - 引数置換（$ARGUMENTS）
/// - ファイル参照（@filename）
/// - Bashコマンド実行（!command）
/// - Claude Code互換性（.claude/commands/）

pub mod loader;
pub mod parser;
pub mod executor;
pub mod error;
pub mod integration;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use crate::os::Os;

/// カスタムコマンドの定義
#[derive(Debug, Clone)]
pub struct CustomCommand {
    /// コマンド名（ファイル名から拡張子を除いたもの）
    pub name: String,
    /// コマンドの内容（マークダウン）
    pub content: String,
    /// フロントマッター（メタデータ）
    pub frontmatter: Option<CommandFrontmatter>,
    /// プロジェクトコマンドかグローバルコマンドか
    pub scope: CommandScope,
    /// コマンドファイルのパス
    pub file_path: PathBuf,
    /// 名前空間（ディレクトリによる分類）
    pub namespace: Option<String>,
}

/// コマンドのスコープ
#[derive(Debug, Clone, PartialEq)]
pub enum CommandScope {
    /// プロジェクト固有のコマンド (.amazonq/commands/ または .claude/commands/)
    Project,
    /// ユーザーグローバルコマンド (~/.aws/amazonq/commands/)
    Global,
}

/// コマンドのフロントマッター（YAML）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CommandFrontmatter {
    /// 許可されるツール
    #[serde(rename = "allowed-tools")]
    pub allowed_tools: Option<Vec<String>>,
    
    /// 引数のヒント
    #[serde(rename = "argument-hint")]
    pub argument_hint: Option<String>,
    
    /// コマンドの説明
    pub description: Option<String>,
    
    /// 使用するモデル
    pub model: Option<String>,
    
    /// Tsumiki互換: 開発フェーズ
    pub phase: Option<String>,
    
    /// Tsumiki互換: 依存コマンド
    pub dependencies: Option<Vec<String>>,
    
    /// Tsumiki互換: 出力形式
    #[serde(rename = "output-format")]
    pub output_format: Option<String>,
}

/// 名前空間付きコマンド情報
#[derive(Debug, Clone)]
pub struct NamespacedCommand {
    pub namespace: CommandNamespace,
    pub base_name: String,
    pub command: Arc<CustomCommand>,
}

/// コマンドの名前空間
#[derive(Debug, Clone, PartialEq)]
pub enum CommandNamespace {
    /// Tsumiki Kairoフロー
    Kairo,
    /// Tsumiki TDDフロー
    Tdd,
    /// Tsumiki リバースエンジニアリング
    Rev,
    /// その他のカスタム名前空間
    Custom(String),
    /// 名前空間なし
    None,
}

impl CommandNamespace {
    /// コマンド名から名前空間を推測
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
    
    /// 名前空間の表示名
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

/// カスタムコマンドのキャッシュ
#[derive(Debug)]
pub struct CustomCommandCache {
    /// コマンド名 -> コマンド定義のマップ
    commands: HashMap<String, Arc<CustomCommand>>,
    /// 最後にスキャンした時刻
    last_scan: std::time::Instant,
    /// スキャン間隔
    scan_interval: std::time::Duration,
}

impl Default for CustomCommandCache {
    fn default() -> Self {
        Self {
            commands: HashMap::new(),
            last_scan: std::time::Instant::now(),
            scan_interval: std::time::Duration::from_secs(30), // 30秒間隔
        }
    }
}

impl CustomCommandCache {
    /// 新しいキャッシュを作成
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 再スキャンが必要かチェック
    pub fn needs_rescan(&self) -> bool {
        self.last_scan.elapsed() > self.scan_interval
    }
    
    /// コマンドを取得（必要に応じて再スキャン）
    pub async fn get_command(&mut self, name: &str, os: &Os) -> Option<Arc<CustomCommand>> {
        if self.needs_rescan() {
            if let Err(e) = self.refresh(os).await {
                tracing::warn!("Failed to refresh custom commands: {}", e);
            }
        }
        self.commands.get(name).cloned()
    }
    
    /// すべてのコマンド名を取得
    pub fn command_names(&self) -> Vec<String> {
        self.commands.keys().cloned().collect()
    }
    
    /// キャッシュを更新
    pub async fn refresh(&mut self, os: &Os) -> Result<(), error::CustomCommandError> {
        let loader = loader::CustomCommandLoader::new();
        self.commands = loader.load_all_commands(os).await?;
        self.last_scan = std::time::Instant::now();
        Ok(())
    }
    
    /// コマンドを手動で追加
    pub fn add_command(&mut self, command: CustomCommand) {
        let name = command.name.clone();
        self.commands.insert(name, Arc::new(command));
    }
    
    /// コマンドを削除
    pub fn remove_command(&mut self, name: &str) -> Option<Arc<CustomCommand>> {
        self.commands.remove(name)
    }
}

/// カスタムコマンドマネージャー
pub struct CustomCommandManager {
    cache: CustomCommandCache,
}

impl CustomCommandManager {
    /// 新しいマネージャーを作成
    pub fn new() -> Self {
        Self {
            cache: CustomCommandCache::new(),
        }
    }
    
    /// コマンドを実行
    pub async fn execute_command(
        &mut self,
        command_name: &str,
        args: &[String],
        os: &Os,
    ) -> Result<String, error::CustomCommandError> {
        let command = self.cache.get_command(command_name, os).await
            .ok_or_else(|| error::CustomCommandError::CommandNotFound(command_name.to_string()))?;
            
        let executor = executor::CustomCommandExecutor::new();
        executor.execute(&command, args, os).await
    }
    
    /// 利用可能なコマンド一覧を取得
    pub async fn list_commands(&mut self, os: &Os) -> Result<Vec<String>, error::CustomCommandError> {
        if self.cache.needs_rescan() {
            self.cache.refresh(os).await?;
        }
        Ok(self.cache.command_names())
    }
    
    /// コマンドの詳細情報を取得
    pub async fn get_command_info(
        &mut self,
        command_name: &str,
        os: &Os,
    ) -> Result<Arc<CustomCommand>, error::CustomCommandError> {
        self.cache.get_command(command_name, os).await
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
        assert_eq!(CommandNamespace::from_command_name("kairo-requirements"), CommandNamespace::Kairo);
        assert_eq!(CommandNamespace::from_command_name("tdd-red"), CommandNamespace::Tdd);
        assert_eq!(CommandNamespace::from_command_name("rev-tasks"), CommandNamespace::Rev);
        assert_eq!(CommandNamespace::from_command_name("custom-command"), CommandNamespace::Custom("custom".to_string()));
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
