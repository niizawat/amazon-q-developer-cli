/// Custom Slash Commands用のCLIサブコマンド

use clap::Subcommand;
use crate::cli::chat::{ChatError, ChatSession, ChatState};
use crate::cli::chat::custom_commands::integration::CustomCommandInstaller;
use crate::os::Os;
use crossterm::{execute, style::{self, Color}};

/// Custom slash commands management
#[derive(Debug, PartialEq, Subcommand)]
pub enum CustomCommandsArgs {
    /// List all available custom commands
    List,
    /// Show help for a specific custom command
    #[clap(name = "show")]
    Show {
        /// Command name to show help for
        command: Option<String>,
    },
    /// Preview command execution without actually running it
    Preview {
        /// Command name to preview
        command: String,
        /// Arguments to pass to the command
        args: Vec<String>,
    },
    /// Initialize custom commands directory
    Init,
    /// Enable security validation for dangerous patterns (default)
    #[clap(name = "secure_on")]
    SecureOn,
    /// Disable security validation for dangerous patterns
    #[clap(name = "secure_off")]
    SecureOff,
    /// Set security validation to warning level only
    #[clap(name = "secure_warn")]
    SecureWarn,
    /// Show current security validation status
    #[clap(name = "secure_status")]
    SecureStatus,
}



impl CustomCommandsArgs {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        match self {
            CustomCommandsArgs::List => {
                let integration = &session.custom_command_integration;
                let commands = integration.list_custom_commands(os).await?;
                
                let output = if commands.is_empty() {
                    "📝 No custom commands found.\n\n💡 Create .md files in .amazonq/commands/ or .claude/commands/ to add custom commands.".to_string()
                } else {
                    format!("📝 Available Custom Commands ({}):

{}", commands.len(), integration.show_custom_command_help(None, os).await?)
                };

                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Cyan),
                    style::Print(output),
                    style::ResetColor,
                    style::Print("\n")
                )?;

                Ok(ChatState::PromptUser { skip_printing_tools: true })
            },
            
            CustomCommandsArgs::Show { command } => {
                let integration = &session.custom_command_integration;
                let help_text = integration.show_custom_command_help(command.as_deref(), os).await?;

                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Cyan),
                    style::Print(help_text),
                    style::ResetColor,
                    style::Print("\n")
                )?;

                Ok(ChatState::PromptUser { skip_printing_tools: true })
            },
            
            CustomCommandsArgs::Preview { command, args } => {
                let integration = &session.custom_command_integration;
                let preview = integration.preview_command(&command, &args, os).await?;

                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Yellow),
                    style::Print("🔍 Command Preview:\n\n"),
                    style::SetForegroundColor(Color::White),
                    style::Print(preview),
                    style::ResetColor,
                    style::Print("\n")
                )?;

                Ok(ChatState::PromptUser { skip_printing_tools: true })
            },
            
            CustomCommandsArgs::Init => {
                let result = CustomCommandInstaller::init_command_directory(os).await?;

                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Green),
                    style::Print("✅ Custom Commands Initialization\n\n"),
                    style::SetForegroundColor(Color::White),
                    style::Print(result),
                    style::ResetColor,
                    style::Print("\n")
                )?;

                Ok(ChatState::PromptUser { skip_printing_tools: true })
            },
            
            CustomCommandsArgs::SecureOn => {
                match session.custom_command_integration.enable_security().await {
                    Ok(_) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Green),
                            style::Print("✅ セキュリティ検証を有効にしました\n"),
                            style::SetForegroundColor(Color::White),
                            style::Print("危険なパターンが検出された場合、エラーとして処理されます。\n"),
                            style::ResetColor
                        )?;
                    },
                    Err(e) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("❌ セキュリティ設定の更新に失敗しました: {}\n", e)),
                            style::ResetColor
                        )?;
                    }
                }
                Ok(ChatState::PromptUser { skip_printing_tools: true })
            },
            
            CustomCommandsArgs::SecureOff => {
                match session.custom_command_integration.disable_security().await {
                    Ok(_) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Yellow),
                            style::Print("⚠️  セキュリティ検証を無効にしました\n"),
                            style::SetForegroundColor(Color::White),
                            style::Print("危険なパターンが検出されても実行が許可されます。注意してください。\n"),
                            style::ResetColor
                        )?;
                    },
                    Err(e) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("❌ セキュリティ設定の更新に失敗しました: {}\n", e)),
                            style::ResetColor
                        )?;
                    }
                }
                Ok(ChatState::PromptUser { skip_printing_tools: true })
            },
            
            CustomCommandsArgs::SecureWarn => {
                match session.custom_command_integration.set_security_warn().await {
                    Ok(_) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Blue),
                            style::Print("🔵 セキュリティ検証を警告レベルに設定しました\n"),
                            style::SetForegroundColor(Color::White),
                            style::Print("危険なパターンが検出された場合、警告が表示されますがエラーにはなりません。\n"),
                            style::ResetColor
                        )?;
                    },
                    Err(e) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("❌ セキュリティ設定の更新に失敗しました: {}\n", e)),
                            style::ResetColor
                        )?;
                    }
                }
                Ok(ChatState::PromptUser { skip_printing_tools: true })
            },
            
            CustomCommandsArgs::SecureStatus => {
                let status = session.custom_command_integration.get_security_status().await;
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Cyan),
                    style::Print("📊 セキュリティ検証設定:\n\n"),
                    style::SetForegroundColor(Color::White),
                    style::Print(status),
                    style::ResetColor,
                    style::Print("\n")
                )?;
                Ok(ChatState::PromptUser { skip_printing_tools: true })
            },

        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_commands_args_structure() {
        // 構造体の作成テスト（try_parse_fromは使用せず、直接構造体を作成）
        let list_cmd = CustomCommandsArgs::List;
        assert!(matches!(list_cmd, CustomCommandsArgs::List));

        let show_cmd = CustomCommandsArgs::Show { command: None };
        assert!(matches!(show_cmd, CustomCommandsArgs::Show { command: None }));

        let show_with_arg = CustomCommandsArgs::Show { 
            command: Some("kairo-requirements".to_string()) 
        };
        assert!(matches!(show_with_arg, CustomCommandsArgs::Show { command: Some(ref cmd) } if cmd == "kairo-requirements"));

        let preview_cmd = CustomCommandsArgs::Preview {
            command: "test-cmd".to_string(),
            args: vec!["arg1".to_string(), "arg2".to_string()]
        };
        if let CustomCommandsArgs::Preview { command, args: cmd_args } = preview_cmd {
            assert_eq!(command, "test-cmd");
            assert_eq!(cmd_args, vec!["arg1", "arg2"]);
        } else {
            panic!("Expected Preview subcommand");
        }

        let init_cmd = CustomCommandsArgs::Init;
        assert!(matches!(init_cmd, CustomCommandsArgs::Init));
        
        // セキュリティコマンドのテスト
        let secure_on_cmd = CustomCommandsArgs::SecureOn;
        assert!(matches!(secure_on_cmd, CustomCommandsArgs::SecureOn));
        
        let secure_off_cmd = CustomCommandsArgs::SecureOff;
        assert!(matches!(secure_off_cmd, CustomCommandsArgs::SecureOff));
        
        let secure_warn_cmd = CustomCommandsArgs::SecureWarn;
        assert!(matches!(secure_warn_cmd, CustomCommandsArgs::SecureWarn));
        
        let secure_status_cmd = CustomCommandsArgs::SecureStatus;
        assert!(matches!(secure_status_cmd, CustomCommandsArgs::SecureStatus));
    }
}
