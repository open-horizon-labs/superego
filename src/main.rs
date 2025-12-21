use clap::{Parser, Subcommand};
use std::path::Path;

mod audit;
mod bd;
mod claude;
mod codex_llm;
mod decision;
mod evaluate;
mod feedback;
mod hooks;
mod init;
mod migrate;
mod oh;
mod state;
mod transcript;

#[derive(Parser)]
#[command(name = "sg")]
#[command(
    author,
    version,
    about = "Superego - Metacognitive advisor for Claude Code"
)]
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

    /// Check hooks and auto-update if outdated
    Check,

    /// Audit decision history with LLM analysis
    Audit {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Migrate from legacy hooks to plugin mode
    Migrate,

    /// Evaluate the most recent Codex session (for Codex skill)
    EvaluateCodex,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { force } => {
            // Check for legacy hooks before initializing
            let has_legacy = migrate::has_legacy_hooks(Path::new("."));

            match init::init(force) {
                Ok(()) => {
                    println!("Superego initialized:");
                    println!("  .superego/prompt.md   - system prompt (customize as needed)");
                    println!("  .superego/config.yaml - configuration");

                    if has_legacy {
                        println!("\n⚠️  Legacy hooks detected from a previous installation.");
                        println!("   Run 'sg migrate' to remove them.");
                    }

                    println!("\nSuperego is ready. Hooks will activate on next session start.");
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
                        result.has_concerns, result.cost_usd
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

            match decision::read_all_sessions(superego_dir) {
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
                        if let Some(hooks) =
                            settings.get_mut("hooks").and_then(|h| h.as_object_mut())
                        {
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
        Commands::EvaluateLlm {
            transcript_path,
            session_id,
        } => {
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
                        result.has_concerns, result.cost_usd
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
        Commands::Check => match hooks::check_and_update_hooks(Path::new(".")) {
            Ok(result) => {
                if result.updated.is_empty() {
                    println!("Hooks up to date.");
                } else {
                    println!("Updated hooks: {}", result.updated.join(", "));
                }
            }
            Err(e) => {
                eprintln!("Failed to check hooks: {}", e);
                std::process::exit(1);
            }
        },
        Commands::Audit { json } => {
            let superego_dir = Path::new(".superego");

            if !superego_dir.exists() {
                eprintln!("No .superego directory found. Run 'sg init' first.");
                std::process::exit(1);
            }

            // Read all decisions across sessions
            let decisions = match decision::read_all_sessions(superego_dir) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("Failed to read decisions: {}", e);
                    std::process::exit(1);
                }
            };

            if decisions.is_empty() {
                if json {
                    println!(
                        r#"{{"stats":{{"total":0,"start_date":null,"end_date":null,"session_count":0}},"analysis":"No decisions recorded yet."}}"#
                    );
                } else {
                    println!("No decisions recorded yet.");
                }
                return;
            }

            // Run audit with LLM analysis
            eprintln!("Analyzing {} decisions...", decisions.len());
            match audit::run_audit(&decisions) {
                Ok(result) => {
                    if json {
                        match serde_json::to_string_pretty(&result) {
                            Ok(json_str) => println!("{}", json_str),
                            Err(e) => {
                                eprintln!("Failed to serialize result: {}", e);
                                std::process::exit(1);
                            }
                        }
                    } else {
                        // Human-readable output
                        println!("Superego Audit Report");
                        println!("=====================");
                        println!("Total decisions: {}", result.stats.total);
                        if let (Some(start), Some(end)) =
                            (result.stats.start_date, result.stats.end_date)
                        {
                            println!(
                                "Date range: {} to {}",
                                start.format("%Y-%m-%d"),
                                end.format("%Y-%m-%d")
                            );
                        }
                        println!("Sessions: {}", result.stats.session_count);
                        println!("\n--- Analysis ---\n");
                        println!("{}", result.analysis);
                    }
                }
                Err(e) => {
                    eprintln!("Audit failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Migrate => {
            let base_dir = Path::new(".");
            match migrate::migrate(base_dir) {
                Ok(report) => {
                    println!("Migration complete:\n{}", report.summary());
                    println!("\nYour .superego/ configuration is preserved.");
                    println!("Hooks will now be provided by the superego plugin.");
                    println!("\nIf you haven't already, install the plugin:");
                    println!("  /plugin marketplace add cloud-atlas-ai/superego");
                    println!("  /plugin install superego@superego");
                }
                Err(migrate::MigrateError::NoLegacyHooks) => {
                    println!("No legacy hooks found. Nothing to migrate.");
                }
                Err(e) => {
                    eprintln!("Migration failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::EvaluateCodex => {
            let superego_dir = Path::new(".superego");

            // Log to .superego/codex.log
            let log = |msg: &str| {
                let log_path = superego_dir.join("codex.log");
                let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ");
                let line = format!("{} {}\n", timestamp, msg);
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path)
                    .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
            };

            log("evaluate-codex started");

            // Check if superego is initialized
            if !superego_dir.exists() {
                log("ERROR: .superego not initialized");
                eprintln!("Superego not initialized. Run 'sg init' first.");
                std::process::exit(1);
            }

            // Check for lock file to prevent concurrent evals
            let lock_path = superego_dir.join("codex.lock");
            let lock_timeout = std::time::Duration::from_secs(300); // 5 minutes

            if lock_path.exists() {
                if let Ok(meta) = lock_path.metadata() {
                    if let Ok(modified) = meta.modified() {
                        if modified.elapsed().unwrap_or(lock_timeout) < lock_timeout {
                            log("SKIP: Another evaluation in progress (lock file exists)");
                            eprintln!("Another evaluation in progress. Skipping.");
                            println!(r#"{{"has_concerns": false, "skipped": true}}"#);
                            return;
                        }
                    }
                }
                // Stale lock, remove it
                let _ = std::fs::remove_file(&lock_path);
            }

            // Create lock file
            if let Err(e) = std::fs::write(&lock_path, chrono::Utc::now().to_rfc3339()) {
                log(&format!("WARN: Could not create lock file: {}", e));
            }

            // Ensure lock is removed on exit (scope guard)
            struct LockGuard<'a>(&'a Path);
            impl<'a> Drop for LockGuard<'a> {
                fn drop(&mut self) {
                    let _ = std::fs::remove_file(self.0);
                }
            }
            let _lock_guard = LockGuard(&lock_path);

            // Find the most recent Codex session
            let session_path = match transcript::codex::find_latest_codex_session() {
                Some(p) => p,
                None => {
                    log("ERROR: No Codex sessions found");
                    eprintln!("No Codex sessions found in ~/.codex/sessions/");
                    eprintln!("Make sure you have an active Codex session.");
                    std::process::exit(1);
                }
            };

            // Log just the filename, not full path
            let session_name = session_path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| session_path.display().to_string());
            log(&format!("Session: {}", session_name));
            eprintln!("Evaluating: {}", session_path.display());

            // Read and format transcript
            let entries = match transcript::codex::read_codex_transcript(&session_path) {
                Ok(e) => e,
                Err(e) => {
                    log(&format!("ERROR reading transcript: {}", e));
                    eprintln!("Failed to read transcript: {}", e);
                    std::process::exit(1);
                }
            };

            if entries.is_empty() {
                log("No entries in transcript");
                println!(r#"{{"has_concerns": false, "cost_usd": 0}}"#);
                eprintln!("No concerns.");
                return;
            }

            let context = transcript::codex::format_codex_context(&entries);
            let context_kb = context.len() / 1024;
            log(&format!(
                "Context: {} entries, {}KB",
                entries.len(),
                context_kb
            ));

            // Load system prompt
            let prompt_path = superego_dir.join("prompt.md");
            let system_prompt = if prompt_path.exists() {
                std::fs::read_to_string(&prompt_path)
                    .unwrap_or_else(|_| include_str!("../default_prompt.md").to_string())
            } else {
                include_str!("../default_prompt.md").to_string()
            };

            let message = format!(
                "Review the following Codex conversation and provide feedback.\n\n\
                --- CONVERSATION ---\n{}\n--- END CONVERSATION ---",
                context
            );

            log("Calling Codex LLM...");
            let start_time = std::time::Instant::now();

            // Use Codex LLM (not Claude) for evaluation
            match codex_llm::invoke(&system_prompt, &message, None) {
                Ok(response) => {
                    let elapsed = start_time.elapsed().as_secs_f32();
                    log(&format!(
                        "Response in {:.1}s, cost=${:.4}",
                        elapsed, response.total_cost_usd
                    ));

                    // Parse decision from response
                    let has_concerns = !response.result.contains("DECISION: ALLOW");

                    println!(
                        r#"{{"has_concerns": {}, "cost_usd": {:.6}}}"#,
                        has_concerns, response.total_cost_usd
                    );

                    if has_concerns {
                        log("BLOCK - concerns found");
                        eprintln!("Feedback:\n{}", response.result);
                    } else {
                        log("ALLOW - no concerns");
                        eprintln!("No concerns.");
                    }
                }
                Err(codex_llm::CodexLlmError::RateLimited { resets_in_seconds }) => {
                    let msg = if let Some(secs) = resets_in_seconds {
                        format!("SKIP: Rate limited (resets in {} min)", secs / 60)
                    } else {
                        "SKIP: Rate limited".to_string()
                    };
                    log(&msg);
                    eprintln!("{}", msg);
                    println!(
                        r#"{{"has_concerns": false, "skipped": true, "reason": "rate_limited"}}"#
                    );
                    // Don't exit with error - this is expected behavior
                }
                Err(e) => {
                    log(&format!("ERROR: {}", e));
                    eprintln!("Evaluation failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
