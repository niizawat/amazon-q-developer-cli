/// カスタムコマンド機能のエラー定義

use thiserror::Error;
use std::path::PathBuf;

/// カスタムコマンドに関するエラー
#[derive(Error, Debug)]
pub enum CustomCommandError {
    /// コマンドが見つからない
    #[error("Custom command '{0}' not found")]
    CommandNotFound(String),
    
    /// ファイル読み込みエラー
    #[error("Failed to read command file '{path}': {source}")]
    FileReadError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    
    /// ディレクトリアクセスエラー
    #[error("Failed to access directory '{path}': {source}")]
    DirectoryError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    
    /// マークダウン解析エラー
    #[error("Failed to parse markdown file '{path}': {message}")]
    MarkdownParseError {
        path: PathBuf,
        message: String,
    },
    
    /// フロントマッター解析エラー
    #[error("Failed to parse frontmatter in '{path}': {source}")]
    FrontmatterParseError {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },
    
    /// コマンド実行エラー
    #[error("Failed to execute custom command '{command}': {message}")]
    ExecutionError {
        command: String,
        message: String,
    },
    
    /// 引数処理エラー
    #[error("Invalid arguments for command '{command}': {message}")]
    ArgumentError {
        command: String,
        message: String,
    },
    
    /// ファイル参照エラー
    #[error("Failed to resolve file reference '{file}': {source}")]
    FileReferenceError {
        file: String,
        #[source]
        source: std::io::Error,
    },
    
    /// Bashコマンド実行エラー
    #[error("Failed to execute bash command: {message}")]
    BashExecutionError {
        message: String,
    },
    
    /// セキュリティエラー
    #[error("Security violation in command '{command}': {message}")]
    SecurityError {
        command: String,
        message: String,
    },
    
    /// 依存関係エラー
    #[error("Dependency error for command '{command}': missing '{dependency}'")]
    DependencyError {
        command: String,
        dependency: String,
    },
    
    /// 設定エラー
    #[error("Configuration error: {message}")]
    ConfigError {
        message: String,
    },
    
    /// タイムアウトエラー
    #[error("Command '{command}' timed out after {timeout_ms}ms")]
    TimeoutError {
        command: String,
        timeout_ms: u64,
    },
    
    /// 一般的なI/Oエラー
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    
    /// JSON解析エラー
    #[error("JSON parse error: {0}")]
    JsonError(#[from] serde_json::Error),
    
    /// YAML解析エラー
    #[error("YAML parse error: {0}")]
    YamlError(#[from] serde_yaml::Error),
    
    /// その他のエラー
    #[error("Unknown error: {0}")]
    Other(String),
}

impl CustomCommandError {
    /// ファイル読み込みエラーを作成
    pub fn file_read_error(path: PathBuf, source: std::io::Error) -> Self {
        Self::FileReadError { path, source }
    }
    
    /// ディレクトリエラーを作成
    pub fn directory_error(path: PathBuf, source: std::io::Error) -> Self {
        Self::DirectoryError { path, source }
    }
    
    /// マークダウン解析エラーを作成
    pub fn markdown_parse_error(path: PathBuf, message: impl Into<String>) -> Self {
        Self::MarkdownParseError {
            path,
            message: message.into(),
        }
    }
    
    /// フロントマッター解析エラーを作成
    pub fn frontmatter_parse_error(path: PathBuf, source: serde_yaml::Error) -> Self {
        Self::FrontmatterParseError { path, source }
    }
    
    /// コマンド実行エラーを作成
    pub fn execution_error(command: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ExecutionError {
            command: command.into(),
            message: message.into(),
        }
    }
    
    /// 引数エラーを作成
    pub fn argument_error(command: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ArgumentError {
            command: command.into(),
            message: message.into(),
        }
    }
    
    /// ファイル参照エラーを作成
    pub fn file_reference_error(file: impl Into<String>, source: std::io::Error) -> Self {
        Self::FileReferenceError {
            file: file.into(),
            source,
        }
    }
    
    /// Bash実行エラーを作成
    pub fn bash_execution_error(message: impl Into<String>) -> Self {
        Self::BashExecutionError {
            message: message.into(),
        }
    }
    
    /// セキュリティエラーを作成
    pub fn security_error(command: impl Into<String>, message: impl Into<String>) -> Self {
        Self::SecurityError {
            command: command.into(),
            message: message.into(),
        }
    }
    
    /// 依存関係エラーを作成
    pub fn dependency_error(command: impl Into<String>, dependency: impl Into<String>) -> Self {
        Self::DependencyError {
            command: command.into(),
            dependency: dependency.into(),
        }
    }
    
    /// 設定エラーを作成
    pub fn config_error(message: impl Into<String>) -> Self {
        Self::ConfigError {
            message: message.into(),
        }
    }
    
    /// タイムアウトエラーを作成
    pub fn timeout_error(command: impl Into<String>, timeout_ms: u64) -> Self {
        Self::TimeoutError {
            command: command.into(),
            timeout_ms,
        }
    }
    
    /// エラーが致命的かどうかを判定
    pub fn is_fatal(&self) -> bool {
        match self {
            Self::CommandNotFound(_) => false, // コマンドが見つからないのは致命的ではない
            Self::ArgumentError { .. } => false, // 引数エラーは修正可能
            Self::SecurityError { .. } => true, // セキュリティエラーは致命的
            Self::ConfigError { .. } => true, // 設定エラーは致命的
            _ => false,
        }
    }
    
    /// ユーザー向けのメッセージを取得
    pub fn user_message(&self) -> String {
        match self {
            Self::CommandNotFound(cmd) => {
                format!("コマンド '{}' が見つかりません。利用可能なコマンドを確認するには '/help' を実行してください。", cmd)
            },
            Self::ArgumentError { command, message } => {
                format!("コマンド '{}' の引数が正しくありません: {}", command, message)
            },
            Self::FileReferenceError { file, .. } => {
                format!("ファイル '{}' を読み込めませんでした。ファイルパスを確認してください。", file)
            },
            Self::SecurityError { command, message } => {
                format!("セキュリティ上の理由により、コマンド '{}' を実行できません: {}", command, message)
            },
            _ => self.to_string(),
        }
    }
}

/// 結果型のエイリアス
pub type Result<T> = std::result::Result<T, CustomCommandError>;
