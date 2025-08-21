/// マークダウンファイルの解析機能
/// 
/// フロントマッター（YAML）とマークダウンコンテンツを分離して解析します。
/// Claude Code互換の形式をサポートします。

use std::path::Path;
use regex::Regex;
use crate::cli::chat::custom_commands::{CommandFrontmatter, error::CustomCommandError};

/// マークダウンファイルの解析結果
#[derive(Debug, Clone)]
pub struct ParsedMarkdown {
    /// フロントマッター（オプション）
    pub frontmatter: Option<CommandFrontmatter>,
    /// マークダウンコンテンツ
    pub content: String,
    /// 元のファイル内容
    pub raw_content: String,
}

/// マークダウンファイルパーサー
pub struct MarkdownParser {
    /// フロントマッター用の正規表現
    frontmatter_regex: Regex,
}

impl Default for MarkdownParser {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownParser {
    /// 新しいパーサーを作成
    pub fn new() -> Self {
        // フロントマッターの正規表現: ---\n...YAML...\n---
        let frontmatter_regex = Regex::new(r"^---\s*\n(.*?)\n---\s*\n(.*)$")
            .expect("Failed to compile frontmatter regex");
        
        Self {
            frontmatter_regex,
        }
    }
    
    /// マークダウンファイルを解析
    pub fn parse(&self, content: &str, file_path: &Path) -> Result<ParsedMarkdown, CustomCommandError> {
        let content = content.trim();
        
        // フロントマッターの抽出を試行
        if let Some(captures) = self.frontmatter_regex.captures(content) {
            // フロントマッターあり
            let frontmatter_yaml = captures.get(1)
                .map(|m| m.as_str())
                .unwrap_or("");
            let markdown_content = captures.get(2)
                .map(|m| m.as_str())
                .unwrap_or("")
                .trim();
            
            // YAMLフロントマッターを解析
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
                    }
                }
            };
            
            Ok(ParsedMarkdown {
                frontmatter,
                content: markdown_content.to_string(),
                raw_content: content.to_string(),
            })
        } else {
            // フロントマッターなし - 全体をマークダウンコンテンツとして扱う
            Ok(ParsedMarkdown {
                frontmatter: None,
                content: content.to_string(),
                raw_content: content.to_string(),
            })
        }
    }
    
    /// ファイルから直接解析
    pub async fn parse_file(&self, file_path: &Path) -> Result<ParsedMarkdown, CustomCommandError> {
        let content = tokio::fs::read_to_string(file_path)
            .await
            .map_err(|e| CustomCommandError::file_read_error(file_path.to_path_buf(), e))?;
        
        self.parse(&content, file_path)
    }
    
    /// コンテンツからフロントマッターのみを抽出
    pub fn extract_frontmatter(&self, content: &str, file_path: &Path) -> Result<Option<CommandFrontmatter>, CustomCommandError> {
        if let Some(captures) = self.frontmatter_regex.captures(content) {
            let frontmatter_yaml = captures.get(1)
                .map(|m| m.as_str())
                .unwrap_or("");
            
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
    
    /// コンテンツからマークダウン部分のみを抽出
    pub fn extract_content(&self, content: &str) -> String {
        if let Some(captures) = self.frontmatter_regex.captures(content) {
            captures.get(2)
                .map(|m| m.as_str().trim())
                .unwrap_or("")
                .to_string()
        } else {
            content.trim().to_string()
        }
    }
    
    /// フロントマッターがあるかどうかを判定
    pub fn has_frontmatter(&self, content: &str) -> bool {
        self.frontmatter_regex.is_match(content)
    }
    
    /// マークダウンファイルかどうかを判定
    pub fn is_markdown_file(file_path: &Path) -> bool {
        file_path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
            .unwrap_or(false)
    }
}

/// プロンプト処理ユーティリティ
pub struct PromptProcessor;

impl PromptProcessor {
    /// 引数置換を実行（$ARGUMENTS プレースホルダー）
    pub fn substitute_arguments(content: &str, args: &[String]) -> String {
        if args.is_empty() {
            // 引数がない場合は$ARGUMENTSを空文字に置換
            content.replace("$ARGUMENTS", "")
        } else {
            // 複数の引数をスペース区切りで結合
            let args_string = shell_words::join(args);
            content.replace("$ARGUMENTS", &args_string)
        }
    }
    
    /// ファイル参照を抽出（@filename パターン）  
    /// メールアドレス（word@domain）は除外し、行頭・空白・特定記号後の@filenameのみ対象
    pub fn extract_file_references(content: &str) -> Vec<String> {
        let file_ref_regex = Regex::new(r"(?:^|[\s\n\r>])\s*@([a-zA-Z0-9._/-]+)")
            .expect("Failed to compile file reference regex");
        
        file_ref_regex
            .captures_iter(content)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect()
    }
    
    /// Bashコマンドを抽出（!`command` パターン）
    pub fn extract_bash_commands(content: &str) -> Vec<String> {
        let bash_cmd_regex = Regex::new(r"!`([^`]+)`")
            .expect("Failed to compile bash command regex");
        
        bash_cmd_regex
            .captures_iter(content)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect()
    }
    
    /// 危険なパターンをチェック
    pub fn check_security_risks(content: &str) -> Vec<String> {
        let mut risks = Vec::new();
        
        // 危険なBashコマンドパターン
        let dangerous_patterns = [
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
        
        for pattern in &dangerous_patterns {
            let regex = match Regex::new(pattern) {
                Ok(r) => r,
                Err(_) => continue,
            };
            
            if regex.is_match(content) {
                risks.push(format!("Potentially dangerous pattern detected: {}", pattern));
            }
        }
        
        // ファイル参照の危険なパターン
        let file_refs = Self::extract_file_references(content);
        for file_ref in file_refs {
            if file_ref.starts_with('/') || file_ref.contains("..") {
                risks.push(format!("Potentially unsafe file reference: {}", file_ref));
            }
        }
        
        risks
    }
    
    /// コンテンツの検証
    pub fn validate_content(content: &str) -> Result<(), CustomCommandError> {
        let risks = Self::check_security_risks(content);
        if !risks.is_empty() {
            return Err(CustomCommandError::security_error(
                "content_validation",
                format!("Security risks detected: {}", risks.join(", ")),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
}
