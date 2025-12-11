use clap::{Parser, Subcommand};
use std::path::Path;

mod transcript;

#[derive(Parser)]
#[command(name = "sg")]
#[command(author, version, about = "Superego - Metacognitive advisor for Claude Code")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize superego for a project
    Init,

    /// Evaluate phase from user message (called by UserPromptSubmit hook)
    Evaluate {
        /// Path to the transcript JSONL file
        #[arg(long)]
        transcript_path: String,
    },

    /// Check if a tool action is allowed (called by PreToolUse hook)
    Check {
        /// Name of the tool being used
        #[arg(long)]
        tool_name: String,
    },

    /// Accept feedback and clear pending state
    Acknowledge,

    /// Override a block with user approval (allows single action)
    Override {
        /// Reason for the override
        reason: String,
    },

    /// Query decision history
    History {
        /// Maximum number of decisions to return
        #[arg(long, default_value = "10")]
        limit: usize,
    },

    /// Inject context into Claude session (called by SessionStart hook)
    ContextInject,

    /// Snapshot state before context compaction (called by PreCompact hook)
    Precompact {
        /// Path to the transcript JSONL file
        #[arg(long)]
        transcript_path: String,
    },

    /// Reset superego state (recovery from corruption)
    Reset {
        /// Also clear the superego Claude session
        #[arg(long)]
        clear_session: bool,
    },

    /// Disable superego for this project
    Disable,

    /// Re-enable superego for this project
    Enable,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            println!("sg init - not yet implemented");
        }
        Commands::Evaluate { transcript_path } => {
            let path = Path::new(&transcript_path);
            match transcript::read_transcript(path) {
                Ok(entries) => {
                    let messages: Vec<_> = entries.iter().filter(|e| e.is_message()).collect();
                    let session_id = transcript::extract_session_id(&entries);

                    println!("Transcript loaded: {} entries, {} messages", entries.len(), messages.len());
                    if let Some(sid) = session_id {
                        println!("Session ID: {}", sid);
                    }

                    // Show recent context
                    println!("\n--- Recent context (last 5 messages) ---");
                    let context = transcript::format_recent_context(&entries, 5);
                    println!("{}", context);

                    // TODO: Call superego LLM for phase evaluation
                    println!("sg evaluate - phase inference not yet implemented");
                }
                Err(e) => {
                    eprintln!("Error reading transcript: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Check { tool_name } => {
            println!("sg check --tool-name {} - not yet implemented", tool_name);
        }
        Commands::Acknowledge => {
            println!("sg acknowledge - not yet implemented");
        }
        Commands::Override { reason } => {
            println!("sg override {:?} - not yet implemented", reason);
        }
        Commands::History { limit } => {
            println!("sg history --limit {} - not yet implemented", limit);
        }
        Commands::ContextInject => {
            println!("sg context-inject - not yet implemented");
        }
        Commands::Precompact { transcript_path } => {
            println!("sg precompact --transcript-path {} - not yet implemented", transcript_path);
        }
        Commands::Reset { clear_session } => {
            println!("sg reset --clear-session={} - not yet implemented", clear_session);
        }
        Commands::Disable => {
            println!("sg disable - not yet implemented");
        }
        Commands::Enable => {
            println!("sg enable - not yet implemented");
        }
    }
}
