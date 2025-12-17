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

    /// Query decision history
    History {
        /// Maximum number of decisions to return
        #[arg(long, default_value = "10")]
        limit: usize,
    },

    /// Check if there's pending feedback (instant, for hooks)
    HasFeedback,

    /// Get pending feedback and clear queue
    GetFeedback,

    /// Reset superego state (recovery from corruption)
    Reset {
        /// Also clear the superego Claude session
        #[arg(long)]
        clear_session: bool,
    },

    /// LLM-based evaluation with natural language feedback
    EvaluateLlm {
        /// Path to the transcript JSONL file
        #[arg(long)]
        transcript_path: String,
        /// Claude session ID (for per-session state isolation)
        #[arg(long)]
        session_id: Option<String>,
    },

    /// Check if periodic evaluation is due (for hooks)
    ShouldEval {
        /// Claude session ID (for per-session state isolation)
        #[arg(long)]
        session_id: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { force } => {
            match init::init(force) {
                Ok(()) => {
                    println!("Superego initialized:");
                    println!("  .superego/prompt.md   - system prompt (customize as needed)");
                    println!("  .claude/settings.json - hooks configured");
                    println!("\nReady to use. Superego will evaluate after each Claude response.");
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

            // Run LLM evaluation (no session_id for legacy command)
            match evaluate::evaluate_llm(transcript, superego_dir, None) {
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
        Commands::Reset { clear_session: _ } => {
            // Remove .superego directory
            if Path::new(".superego").exists() {
                if let Err(e) = std::fs::remove_dir_all(".superego") {
                    eprintln!("Failed to remove .superego: {}", e);
                } else {
                    println!("Removed .superego/");
                }
            }

            // Remove superego hooks from .claude/hooks/superego
            let hooks_dir = Path::new(".claude/hooks/superego");
            if hooks_dir.exists() {
                if let Err(e) = std::fs::remove_dir_all(hooks_dir) {
                    eprintln!("Failed to remove {}: {}", hooks_dir.display(), e);
                } else {
                    println!("Removed .claude/hooks/superego/");
                }
            }

            // Remove superego hooks from .claude/settings.json
            let settings_path = Path::new(".claude/settings.json");
            if settings_path.exists() {
                if let Ok(content) = std::fs::read_to_string(settings_path) {
                    if let Ok(mut settings) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
                            for (_name, hook_array) in hooks.iter_mut() {
                                if let Some(arr) = hook_array.as_array_mut() {
                                    arr.retain(|h| {
                                        !h.get("hooks")
                                            .and_then(|hs| hs.as_array())
                                            .and_then(|hs| hs.first())
                                            .and_then(|h| h.get("command"))
                                            .and_then(|c| c.as_str())
                                            .map(|c| c.contains("superego"))
                                            .unwrap_or(false)
                                    });
                                }
                            }
                            if let Ok(formatted) = serde_json::to_string_pretty(&settings) {
                                if let Err(e) = std::fs::write(settings_path, formatted) {
                                    eprintln!("Failed to update settings.json: {}", e);
                                } else {
                                    println!("Removed superego hooks from .claude/settings.json");
                                }
                            }
                        }
                    }
                }
            }

            println!("\nSuperego reset complete. Run 'sg init' to reinitialize.");
        }
        Commands::EvaluateLlm { transcript_path, session_id } => {
            let transcript = Path::new(&transcript_path);
            let superego_dir = Path::new(".superego");

            // Check if superego is initialized
            if !superego_dir.exists() {
                eprintln!("Superego not initialized. Run 'sg init' first.");
                std::process::exit(1);
            }

            // Run LLM evaluation
            match evaluate::evaluate_llm(transcript, superego_dir, session_id.as_deref()) {
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
        Commands::ShouldEval { session_id } => {
            let superego_dir = Path::new(".superego");

            // Check if superego is initialized
            if !superego_dir.exists() {
                println!("no");
                std::process::exit(1);
            }

            // Use session-namespaced state dir if session_id provided
            let state_dir = if let Some(ref sid) = session_id {
                superego_dir.join("sessions").join(sid)
            } else {
                superego_dir.to_path_buf()
            };

            // Read state to get last_evaluated
            let state_mgr = state::StateManager::new(&state_dir);
            let current_state = match state_mgr.load() {
                Ok(s) => s,
                Err(_) => {
                    // Can't read state, assume eval needed
                    println!("yes");
                    std::process::exit(0);
                }
            };

            // Read config to get eval_interval_minutes (default: 5)
            let config_path = superego_dir.join("config.yaml");
            let interval_minutes: i64 = if config_path.exists() {
                std::fs::read_to_string(&config_path)
                    .ok()
                    .and_then(|content| {
                        // Simple parsing: look for "eval_interval_minutes: N"
                        for line in content.lines() {
                            let line = line.trim();
                            if line.starts_with("eval_interval_minutes:") {
                                return line
                                    .strip_prefix("eval_interval_minutes:")
                                    .and_then(|v| v.trim().parse().ok());
                            }
                        }
                        None
                    })
                    .unwrap_or(5)
            } else {
                5
            };

            // Check if eval is due
            match current_state.last_evaluated {
                None => {
                    // Never evaluated, should eval
                    println!("yes");
                    std::process::exit(0);
                }
                Some(last) => {
                    let now = chrono::Utc::now();
                    let elapsed = now.signed_duration_since(last);
                    let threshold = chrono::Duration::minutes(interval_minutes);

                    if elapsed >= threshold {
                        println!("yes");
                        std::process::exit(0);
                    } else {
                        println!("no");
                        std::process::exit(1);
                    }
                }
            }
        }
    }
}
