use clap::{Parser, Subcommand};
use std::path::Path;

mod bd;
mod claude;
mod decision;
mod evaluate;
mod feedback;
mod init;
mod state;
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
    Init {
        /// Force re-initialization even if .superego/ exists
        #[arg(long)]
        force: bool,
    },

    /// Evaluate phase from user message (called by UserPromptSubmit hook)
    Evaluate {
        /// Path to the transcript JSONL file
        #[arg(long)]
        transcript_path: String,
    },

    /// Accept feedback and clear pending state
    Acknowledge,

    /// Query decision history
    History {
        /// Maximum number of decisions to return
        #[arg(long, default_value = "10")]
        limit: usize,
    },

    /// Inject context into Claude session (called by SessionStart hook)
    ContextInject,

    /// Check if there's pending feedback (instant, for hooks)
    HasFeedback,

    /// Get pending feedback and clear queue
    GetFeedback,

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

    /// LLM-based evaluation with natural language feedback
    EvaluateLlm {
        /// Path to the transcript JSONL file
        #[arg(long)]
        transcript_path: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { force } => {
            match init::init(force) {
                Ok(()) => {
                    println!("Superego initialized in .superego/");
                    println!("  - prompt.md: system prompt (customize as needed)");
                    println!("  - state.json: override/disabled state");
                    println!("  - decisions/: decision journal");
                    println!("\nNext: configure hooks in .claude/settings.json");
                }
                Err(init::InitError::AlreadyExists) => {
                    eprintln!(".superego/ already exists. Use --force to reinitialize.");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("Error initializing: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Evaluate { transcript_path } => {
            // AIDEV-NOTE: This command now redirects to evaluate-llm
            // The old phase-based evaluation is removed.
            let transcript = Path::new(&transcript_path);
            let superego_dir = Path::new(".superego");

            // Check if superego is initialized
            if !superego_dir.exists() {
                eprintln!("Superego not initialized. Run 'sg init' first.");
                std::process::exit(1);
            }

            // Run LLM evaluation
            match evaluate::evaluate_llm(transcript, superego_dir) {
                Ok(result) => {
                    println!(
                        r#"{{"has_concerns": {}, "cost_usd": {:.6}}}"#,
                        result.has_concerns,
                        result.cost_usd
                    );

                    if result.has_concerns {
                        eprintln!("Feedback:\n{}", result.feedback);
                    } else {
                        eprintln!("No concerns.");
                    }
                }
                Err(e) => {
                    eprintln!("Evaluation failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Acknowledge => {
            println!("sg acknowledge - not yet implemented");
        }
        Commands::History { limit } => {
            let superego_dir = Path::new(".superego");
            let journal = decision::Journal::new(superego_dir);

            match journal.read_all() {
                Ok(decisions) => {
                    let start = decisions.len().saturating_sub(limit);
                    let recent: Vec<_> = decisions.into_iter().skip(start).collect();

                    if recent.is_empty() {
                        println!("No decisions recorded yet.");
                    } else {
                        println!("Last {} decision(s):\n", recent.len());
                        for d in recent {
                            println!("---");
                            println!("Timestamp: {}", d.timestamp);
                            println!("Type: {:?}", d.decision_type);
                            if let Some(trigger) = &d.trigger {
                                println!("Trigger: {}", trigger);
                            }
                            if let Some(ctx) = &d.context {
                                println!("Context: {}", ctx);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error reading decisions: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::ContextInject => {
            println!("sg context-inject - not yet implemented");
        }
        Commands::HasFeedback => {
            let superego_dir = Path::new(".superego");
            let queue = feedback::FeedbackQueue::new(superego_dir);

            if queue.has_feedback() {
                // Exit 0 = has feedback
                std::process::exit(0);
            } else {
                // Exit 1 = no feedback
                std::process::exit(1);
            }
        }
        Commands::GetFeedback => {
            let superego_dir = Path::new(".superego");
            let queue = feedback::FeedbackQueue::new(superego_dir);

            match queue.get_and_clear() {
                Some(content) => {
                    println!("{}", content);
                }
                None => {
                    println!("No pending feedback.");
                }
            }
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
        Commands::EvaluateLlm { transcript_path } => {
            let transcript = Path::new(&transcript_path);
            let superego_dir = Path::new(".superego");

            // Check if superego is initialized
            if !superego_dir.exists() {
                eprintln!("Superego not initialized. Run 'sg init' first.");
                std::process::exit(1);
            }

            // Run LLM evaluation
            match evaluate::evaluate_llm(transcript, superego_dir) {
                Ok(result) => {
                    // Output for hook/debugging
                    println!(
                        r#"{{"has_concerns": {}, "cost_usd": {:.6}}}"#,
                        result.has_concerns,
                        result.cost_usd
                    );

                    // Log feedback to stderr
                    if result.has_concerns {
                        eprintln!("Feedback:\n{}", result.feedback);
                    } else {
                        eprintln!("No concerns.");
                    }
                }
                Err(e) => {
                    eprintln!("Evaluation failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
