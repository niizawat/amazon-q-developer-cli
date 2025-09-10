//! CLI subcommands for Custom Slash Commands
use clap::Subcommand;
use crossterm::execute;
use crossterm::style::{
    self,
    Color,
};

use crate::cli::chat::custom_commands::integration::CustomCommandInstaller;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::database::settings::Setting;
use crate::os::Os;

/// Custom slash commands management
#[derive(Debug, PartialEq, Subcommand)]
pub enum CustomCommandsArgs {
    /// List all available custom commands
    List,
    /// Show help for a specific custom command
    #[command(name = "show")]
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
    #[command(name = "secure_on")]
    SecureOn,
    /// Disable security validation for dangerous patterns
    #[command(name = "secure_off")]
    SecureOff,
    /// Set security validation to warning level only
    #[command(name = "secure_warn")]
    SecureWarn,
    /// Show current security validation status
    #[command(name = "secure_status")]
    SecureStatus,
}

impl CustomCommandsArgs {
    pub async fn execute(self, os: &mut Os, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        // Check if custom commands experimental feature is enabled
        if !os
            .database
            .settings
            .get_bool(Setting::EnabledCustomCommands)
            .unwrap_or(false)
        {
            execute!(
                session.stderr,
                style::SetForegroundColor(Color::Yellow),
                style::Print("⚠️  Custom Commands is an experimental feature.\n"),
                style::SetForegroundColor(Color::White),
                style::Print("Enable it using: "),
                style::SetForegroundColor(Color::Green),
                style::Print("/experiment"),
                style::SetForegroundColor(Color::White),
                style::Print(" and select 'Custom Commands'\n\n"),
                style::ResetColor
            )?;
            return Ok(ChatState::PromptUser {
                skip_printing_tools: true,
            });
        }

        match self {
            CustomCommandsArgs::List => {
                let integration = &session.custom_command_integration;
                let commands = integration.list_custom_commands(os).await?;

                let output = if commands.is_empty() {
                    "📝 No custom commands found.\n\n💡 Create .md files in .amazonq/commands/ to add custom commands.".to_string()
                } else {
                    // Check for duplicates
                    let conflicts = integration.check_command_conflicts(&commands);
                    let mut output = format!(
                        "📝 Available Custom Commands ({}):\n\n{}",
                        commands.len(),
                        integration.show_custom_command_help(None, os).await?
                    );

                    // Show warning if there are conflicts
                    if !conflicts.is_empty() {
                        output.push_str(
                            "\n⚠️  WARNING: The following custom commands conflict with existing slash commands:\n",
                        );
                        for conflict in conflicts {
                            output.push_str(&format!("   • /{} (custom command will be ignored)\n", conflict));
                        }
                        output.push_str("\n💡 Consider renaming these commands to avoid conflicts.\n");
                    }

                    output
                };

                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Cyan),
                    style::Print(output),
                    style::ResetColor,
                    style::Print("\n")
                )?;

                Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                })
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

                Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                })
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

                Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                })
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

                Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                })
            },

            CustomCommandsArgs::SecureOn => {
                match session.custom_command_integration.enable_security().await {
                    Ok(_) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Green),
                            style::Print("✅ Security validation enabled\n"),
                            style::SetForegroundColor(Color::White),
                            style::Print("Dangerous patterns will be treated as errors.\n"),
                            style::ResetColor
                        )?;
                    },
                    Err(e) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("❌ Failed to update security settings: {}\n", e)),
                            style::ResetColor
                        )?;
                    },
                }
                Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                })
            },

            CustomCommandsArgs::SecureOff => {
                match session.custom_command_integration.disable_security().await {
                    Ok(_) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Yellow),
                            style::Print("⚠️  Security validation disabled\n"),
                            style::SetForegroundColor(Color::White),
                            style::Print("Dangerous patterns will be allowed to execute. Use with caution.\n"),
                            style::ResetColor
                        )?;
                    },
                    Err(e) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("❌ Failed to update security settings: {}\n", e)),
                            style::ResetColor
                        )?;
                    },
                }
                Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                })
            },

            CustomCommandsArgs::SecureWarn => {
                match session.custom_command_integration.set_security_warn().await {
                    Ok(_) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Blue),
                            style::Print("🔵 Security validation set to warning level\n"),
                            style::SetForegroundColor(Color::White),
                            style::Print("Dangerous patterns will show warnings but won't cause errors.\n"),
                            style::ResetColor
                        )?;
                    },
                    Err(e) => {
                        execute!(
                            session.stderr,
                            style::SetForegroundColor(Color::Red),
                            style::Print(format!("❌ Failed to update security settings: {}\n", e)),
                            style::ResetColor
                        )?;
                    },
                }
                Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                })
            },

            CustomCommandsArgs::SecureStatus => {
                let status = session.custom_command_integration.get_security_status().await;
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Cyan),
                    style::Print("📊 Security Validation Settings:\n\n"),
                    style::SetForegroundColor(Color::White),
                    style::Print(status),
                    style::ResetColor,
                    style::Print("\n")
                )?;
                Ok(ChatState::PromptUser {
                    skip_printing_tools: true,
                })
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_commands_args_structure() {
        // Test struct creation (create structs directly without using try_parse_from)
        let list_cmd = CustomCommandsArgs::List;
        assert!(matches!(list_cmd, CustomCommandsArgs::List));

        let show_cmd = CustomCommandsArgs::Show { command: None };
        assert!(matches!(show_cmd, CustomCommandsArgs::Show { command: None }));

        let show_with_arg = CustomCommandsArgs::Show {
            command: Some("kairo-requirements".to_string()),
        };
        assert!(
            matches!(show_with_arg, CustomCommandsArgs::Show { command: Some(ref cmd) } if cmd == "kairo-requirements")
        );

        let preview_cmd = CustomCommandsArgs::Preview {
            command: "test-cmd".to_string(),
            args: vec!["arg1".to_string(), "arg2".to_string()],
        };
        if let CustomCommandsArgs::Preview {
            command,
            args: cmd_args,
        } = preview_cmd
        {
            assert_eq!(command, "test-cmd");
            assert_eq!(cmd_args, vec!["arg1", "arg2"]);
        } else {
            panic!("Expected Preview subcommand");
        }

        let init_cmd = CustomCommandsArgs::Init;
        assert!(matches!(init_cmd, CustomCommandsArgs::Init));

        // Test security commands
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
