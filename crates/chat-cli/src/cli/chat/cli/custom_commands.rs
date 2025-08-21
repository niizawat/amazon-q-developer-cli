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
    }
}
