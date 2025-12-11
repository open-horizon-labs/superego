use clap::{Parser, Subcommand};
use std::path::Path;

mod claude;
mod decision;
mod evaluate;
mod init;
mod state;
mod tools;
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
        Commands::Init { force } => {
            match init::init(force) {
                Ok(()) => {
                    println!("Superego initialized in .superego/");
                    println!("  - prompt.md: system prompt (customize as needed)");
                    println!("  - state.json: current phase (exploring)");
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
            let transcript = Path::new(&transcript_path);
            let superego_dir = Path::new(".superego");

            // Check if superego is initialized
            if !superego_dir.exists() {
                eprintln!("Superego not initialized. Run 'sg init' first.");
                std::process::exit(1);
            }

            // Run evaluation with fallback
            let result = evaluate::evaluate_with_fallback(transcript, superego_dir, 10);

            // Output result as JSON for hook consumption
            println!(
                r#"{{"phase": "{}", "previous": "{}", "changed": {}, "cost_usd": {:.6}}}"#,
                result.phase,
                result.previous_phase,
                result.changed,
                result.cost_usd
            );

            // Log concerns to stderr (visible but doesn't affect hook)
            for concern in &result.concerns {
                eprintln!("Concern: {}", concern);
            }

            if let Some(scope) = &result.approved_scope {
                eprintln!("Approved scope: {}", scope);
            }
        }
        Commands::Check { tool_name } => {
            let superego_dir = Path::new(".superego");
            let state_mgr = state::StateManager::new(superego_dir);

            // Read tools always pass - no state check needed
            if !tools::requires_gating(&tool_name) {
                println!(r#"{{"decision": "allow", "reason": "read-only tool"}}"#);
                return;
            }

            // Write tools - check state
            match state_mgr.load() {
                Ok(mut current_state) => {
                    if current_state.allows_write() {
                        // Check if this was an override (single-use)
                        if current_state.pending_override.is_some() {
                            current_state.consume_override();
                            if let Err(e) = state_mgr.save(&current_state) {
                                eprintln!("Warning: failed to clear override: {}", e);
                            }
                            println!(r#"{{"decision": "allow", "reason": "override consumed"}}"#);
                        } else {
                            println!(r#"{{"decision": "allow", "reason": "phase is ready"}}"#);
                        }
                    } else {
                        // Block with helpful message
                        let reason = format!(
                            "Phase is {}. User confirmation needed before write actions.",
                            current_state.phase
                        );
                        println!(r#"{{"decision": "block", "phase": "{}", "reason": "{}"}}"#,
                            current_state.phase, reason);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    // AIDEV-NOTE: On state read error, fail open with warning
                    // This prevents state corruption from blocking all work
                    eprintln!("Warning: failed to read state: {}", e);
                    println!(r#"{{"decision": "allow", "reason": "state read error - fail open"}}"#);
                }
            }
        }
        Commands::Acknowledge => {
            println!("sg acknowledge - not yet implemented");
        }
        Commands::Override { reason } => {
            let superego_dir = Path::new(".superego");
            let state_mgr = state::StateManager::new(superego_dir);
            let journal = decision::Journal::new(superego_dir);

            match state_mgr.update(|s| {
                s.set_override(reason.clone());
            }) {
                Ok(_) => {
                    // Also record in decision journal
                    let decision = decision::Decision::override_granted(None, reason);
                    if let Err(e) = journal.write(&decision) {
                        eprintln!("Warning: failed to write decision journal: {}", e);
                    }
                    println!("Override set. Next blocked action will be allowed.");
                }
                Err(e) => {
                    eprintln!("Error setting override: {}", e);
                    std::process::exit(1);
                }
            }
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
                            if let Some(from) = d.from_state {
                                println!("From: {}", from);
                            }
                            if let Some(to) = d.to_state {
                                println!("To: {}", to);
                            }
                            if let Some(trigger) = &d.trigger {
                                println!("Trigger: {}", trigger);
                            }
                            if let Some(scope) = &d.approved_scope {
                                println!("Scope: {}", scope);
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
