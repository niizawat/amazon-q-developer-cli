/// カスタムコマンドの実行エンジン
/// 
/// カスタムコマンドを実行し、以下の機能を提供します：
/// - 引数置換（$ARGUMENTS）
/// - ファイル参照（@filename）
/// - Bashコマンド実行（!`command`）
/// - セキュリティ検証

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

use crate::cli::chat::custom_commands::{
    CustomCommand,
    parser::PromptProcessor,
    error::CustomCommandError,
};
use crate::os::Os;

/// コマンド実行エンジン
pub struct CustomCommandExecutor {
    /// Bashコマンドのタイムアウト（デフォルト30秒）
    bash_timeout: Duration,
    /// セキュリティモード
    security_mode: SecurityMode,
}

/// セキュリティモード
#[derive(Debug, Clone)]
pub enum SecurityMode {
    /// 厳格モード - 危険なコマンドを拒否
    Strict,
    /// 警告モード - 警告を表示するが実行は許可
    Warning,
    /// 許可モード - すべて許可
    Permissive,
}

impl Default for CustomCommandExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl CustomCommandExecutor {
    /// 新しい実行エンジンを作成
    pub fn new() -> Self {
        Self {
            bash_timeout: Duration::from_secs(30),
            security_mode: SecurityMode::Strict,
        }
    }
    
    /// タイムアウトを設定
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.bash_timeout = timeout;
        self
    }
    
    /// セキュリティモードを設定
    pub fn with_security_mode(mut self, mode: SecurityMode) -> Self {
        self.security_mode = mode;
        self
    }
    
    /// カスタムコマンドを実行
    pub async fn execute(
        &self,
        command: &CustomCommand,
        args: &[String],
        os: &Os,
    ) -> Result<String, CustomCommandError> {
        tracing::info!("Executing custom command: {}", command.name);
        
        // 1. セキュリティチェック
        self.security_check(command)?;
        
        // 2. 引数置換
        let mut processed_content = PromptProcessor::substitute_arguments(&command.content, args);
        
        // 3. Bashコマンド実行（!`command`パターン）
        processed_content = self.execute_bash_commands(&processed_content, os).await?;
        
        // 4. ファイル参照解決（@filenameパターン）
        processed_content = self.resolve_file_references(&processed_content, os).await?;
        
        tracing::debug!("Processed content length: {}", processed_content.len());
        Ok(processed_content)
    }
    
    /// セキュリティチェック
    fn security_check(&self, command: &CustomCommand) -> Result<(), CustomCommandError> {
        match self.security_mode {
            SecurityMode::Permissive => return Ok(()), // すべて許可
            SecurityMode::Warning | SecurityMode::Strict => {
                let risks = PromptProcessor::check_security_risks(&command.content);
                if !risks.is_empty() {
                    match self.security_mode {
                        SecurityMode::Warning => {
                            tracing::warn!("Security risks detected in command '{}': {:?}", command.name, risks);
                        },
                        SecurityMode::Strict => {
                            return Err(CustomCommandError::security_error(
                                &command.name,
                                format!("Security risks detected: {}", risks.join(", ")),
                            ));
                        },
                        _ => unreachable!(),
                    }
                }
            },
        }
        Ok(())
    }
    
    /// Bashコマンドを実行
    async fn execute_bash_commands(
        &self,
        content: &str,
        os: &Os,
    ) -> Result<String, CustomCommandError> {
        let bash_commands = PromptProcessor::extract_bash_commands(content);
        if bash_commands.is_empty() {
            return Ok(content.to_string());
        }
        
        let mut result = content.to_string();
        
        for bash_cmd in bash_commands {
            tracing::debug!("Executing bash command: {}", bash_cmd);
            
            let output = self.run_bash_command(&bash_cmd, os).await?;
            
            // !`command` パターンを結果で置換
            let pattern = format!("!`{}`", bash_cmd);
            result = result.replace(&pattern, &output);
        }
        
        Ok(result)
    }
    
    /// 単一のBashコマンドを実行
    async fn run_bash_command(&self, cmd: &str, _os: &Os) -> Result<String, CustomCommandError> {
        // セキュリティチェック
        let risks = PromptProcessor::check_security_risks(cmd);
        if !risks.is_empty() && matches!(self.security_mode, SecurityMode::Strict) {
            return Err(CustomCommandError::bash_execution_error(
                format!("Dangerous command rejected: {}", cmd),
            ));
        }
        
        // Bashコマンドを実行
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
        
        // タイムアウト付きで実行
        let child = command.spawn()
            .map_err(|e| CustomCommandError::bash_execution_error(
                format!("Failed to spawn command '{}': {}", cmd, e),
            ))?;
        
        let output = timeout(self.bash_timeout, child.wait_with_output())
            .await
            .map_err(|_| CustomCommandError::timeout_error(cmd, self.bash_timeout.as_millis() as u64))?
            .map_err(|e| CustomCommandError::bash_execution_error(
                format!("Command execution failed '{}': {}", cmd, e),
            ))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CustomCommandError::bash_execution_error(
                format!("Command failed '{}': {}", cmd, stderr),
            ));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.trim().to_string())
    }
    
    /// ファイル参照を解決
    async fn resolve_file_references(
        &self,
        content: &str,
        os: &Os,
    ) -> Result<String, CustomCommandError> {
        let file_refs = PromptProcessor::extract_file_references(content);
        if file_refs.is_empty() {
            return Ok(content.to_string());
        }
        
        let mut result = content.to_string();
        let current_dir = os.env.current_dir()
            .map_err(|e| CustomCommandError::config_error(format!("Failed to get current directory: {}", e)))?;
        
        for file_ref in file_refs {
            tracing::debug!("Resolving file reference: {}", file_ref);
            
            let file_content = self.read_file_reference(&file_ref, &current_dir).await?;
            
            // @filename パターンを内容で置換
            let pattern = format!("@{}", file_ref);
            let replacement = format!("```\n{}\n```", file_content);
            result = result.replace(&pattern, &replacement);
        }
        
        Ok(result)
    }
    
    /// ファイル参照を読み込み
    async fn read_file_reference(
        &self,
        file_ref: &str,
        current_dir: &Path,
    ) -> Result<String, CustomCommandError> {
        // セキュリティチェック: 相対パスの外側へのアクセスを防ぐ
        if file_ref.contains("..") || file_ref.starts_with('/') {
            return Err(CustomCommandError::security_error(
                "file_reference",
                format!("Unsafe file reference: {}", file_ref),
            ));
        }
        
        let file_path = current_dir.join(file_ref);
        
        // ファイルの存在チェック
        if !file_path.exists() {
            return Err(CustomCommandError::file_reference_error(
                file_ref.to_string(),
                std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"),
            ));
        }
        
        // ファイルサイズチェック（大きすぎるファイルを防ぐ）
        let metadata = tokio::fs::metadata(&file_path)
            .await
            .map_err(|e| CustomCommandError::file_reference_error(file_ref.to_string(), e))?;
        
        const MAX_FILE_SIZE: u64 = 1024 * 1024; // 1MB
        if metadata.len() > MAX_FILE_SIZE {
            return Err(CustomCommandError::file_reference_error(
                file_ref.to_string(),
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("File too large: {} bytes (max: {} bytes)", metadata.len(), MAX_FILE_SIZE),
                ),
            ));
        }
        
        // ファイル内容を読み込み
        let content = tokio::fs::read_to_string(&file_path)
            .await
            .map_err(|e| CustomCommandError::file_reference_error(file_ref.to_string(), e))?;
        
        Ok(content)
    }
    
    /// プレビューモードでの実行（実際には実行せず、処理内容を表示）
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
            estimated_execution_time: self.estimate_execution_time(command),
        };
        
        // セキュリティチェック結果を追加
        if let Err(e) = self.security_check(command) {
            preview.security_risks.push(e.to_string());
        }
        
        Ok(preview)
    }
    
    /// 実行時間を推定
    fn estimate_execution_time(&self, command: &CustomCommand) -> Duration {
        let bash_commands = PromptProcessor::extract_bash_commands(&command.content);
        let file_refs = PromptProcessor::extract_file_references(&command.content);
        
        let base_time = Duration::from_millis(100); // 基本処理時間
        let bash_time = Duration::from_millis(500 * bash_commands.len() as u64); // Bashコマンド1つあたり500ms
        let file_time = Duration::from_millis(50 * file_refs.len() as u64); // ファイル参照1つあたり50ms
        
        base_time + bash_time + file_time
    }
}

/// 実行プレビュー結果
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
    /// プレビューを表示用文字列に変換
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

/*
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::chat::custom_commands::{CommandScope, CommandFrontmatter};
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_argument_substitution() {
        let command = CustomCommand {
            name: "test".to_string(),
            content: "Process issue #$ARGUMENTS".to_string(),
            frontmatter: None,
            scope: CommandScope::Project,
            file_path: PathBuf::from("test.md"),
            namespace: None,
        };
        
        let executor = CustomCommandExecutor::new().with_security_mode(SecurityMode::Permissive);
        let os = crate::os::Os::default();
        
        let result = executor.execute(&command, &["123".to_string()], &os).await.unwrap();
        assert_eq!(result, "Process issue #123");
    }

    #[tokio::test]
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
        
        let executor = CustomCommandExecutor::new().with_security_mode(SecurityMode::Permissive);
        let mut os = crate::os::Os::default();
        os.env.set_current_dir(temp_dir.path()).unwrap();
        
        let result = executor.execute(&command, &[], &os).await.unwrap();
        assert!(result.contains("Test content"));
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
*/
