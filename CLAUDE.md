# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Superego is a metacognitive advisor for Claude Code. It monitors conversations, evaluates Claude's approach, and provides feedback via Claude Code hooks before Claude finishes or makes large edits.

**Binary:** `sg` (short for superego)

## Build & Test Commands

```bash
cargo build              # Development build
cargo build --release    # Release build
cargo test               # Run all tests
cargo test <test_name>   # Run single test
cargo run -- <args>      # Run with args (e.g., cargo run -- init)
```

## Architecture

### Core Flow
1. **SessionStart hook** → Injects superego contract into Claude's context
2. **PreToolUse hook** → Evaluates large Edit/Write operations (≥20 lines by default)
3. **Stop/PreCompact hooks** → Runs LLM evaluation before Claude finishes

### Module Structure

- `main.rs` - CLI entry point using clap, defines all subcommands
- `init.rs` - Creates `.superego/` directory structure (hooks are now provided by plugin)
- `migrate.rs` - Migration from legacy hooks to plugin mode
- `evaluate.rs` - LLM-based evaluation logic; calls Claude to review conversation transcripts
- `claude.rs` - Wrapper for invoking Claude CLI (`claude -p --output-format json`)
- `audit.rs` - Audit command: aggregates decisions and runs LLM analysis
- `transcript/` - Parses Claude Code JSONL transcript files
  - `types.rs` - Serde structs for transcript entries (User, Assistant, Summary, etc.)
  - `reader.rs` - Reads and filters transcript messages since last evaluation; dedupes system reminders (keeps last)
- `bd.rs` - Integration with beads (`bd`) task tracking; provides current task context
- `state.rs` - Manages `.superego/state.json` (last_evaluated timestamp)
- `decision.rs` - Decision journal for audit trail; `read_all_sessions()` aggregates from all session dirs
- `feedback.rs` - Feedback queue (`.superego/feedback` file)

### Plugin Structure (Claude Code Plugin)

Located in `plugin/`:
- `.claude-plugin/plugin.json` - Plugin manifest
- `hooks/hooks.json` - Hook event → script mappings
- `scripts/session-start.sh` - Injects superego contract via `additionalContext`
- `scripts/evaluate.sh` - Runs `sg evaluate-llm`, blocks if concerns found
- `scripts/pre-tool-use.sh` - Evaluates pending changes before large Edit/Write operations

### Legacy Hook Scripts (kept for reference)

Located in `hooks/` (embedded in binary for legacy mode):
- `session-start.sh`, `evaluate.sh`, `pre-tool-use.sh`

### Key Design Patterns

**Recursion prevention:** Superego's own Claude calls set `SUPEREGO_DISABLED=1` to prevent hooks from triggering on themselves.

**Decision format:** LLM responses must follow `DECISION: ALLOW|BLOCK\n\n<feedback>` format. Unknown decisions default to BLOCK for safety.

**State tracking:** `last_evaluated` timestamp in state.json ensures only new conversation content is evaluated.

## Dependencies

Minimal dependency set (no regex, no async runtime):
- `chrono` - DateTime handling, RFC3339 parsing/formatting, serde integration
- `clap` - CLI argument parsing with derive macros
- `serde` + `serde_json` - JSON serialization for transcripts, state, decisions
- `tempfile` (dev) - Test fixtures

## Environment Variables

- `SUPEREGO_DISABLED=1` - Disables superego entirely
- `SUPEREGO_CHANGE_THRESHOLD=N` - Lines required to trigger PreToolUse evaluation (default: 20)

## Files Created by `sg init`

```
.superego/
├── prompt.md          # Customizable system prompt for evaluation
├── state.json         # Evaluation state (last_evaluated timestamp)
├── config.yaml        # Configuration (eval interval, model, etc.)
├── sessions/          # Per-session state and decisions
│   └── <session-id>/
│       ├── state.json
│       ├── decisions/  # Decision journal (audit trail) - JSON files
│       └── superego_session
└── feedback           # Pending feedback queue (transient)
```

Note: Hook configuration is now provided by the Claude Code plugin (`/plugin install superego`).
The plugin's hooks use `${CLAUDE_PROJECT_DIR}` to find the project's `.superego/` directory.

## CLI Commands

- `sg init` - Initialize superego for a project
- `sg migrate` - Remove legacy hooks (for users upgrading from < v0.4.0)
- `sg audit` - Analyze decision history with LLM (patterns, timeline, insights)
- `sg audit --json` - JSON output for programmatic use
- `sg history --limit N` - Show recent decisions
- `sg check` - Verify hooks are up to date
- `sg reset` - Remove superego configuration

## Decision Journal

Decisions are stored as JSON files in `.superego/sessions/<session-id>/decisions/`.

**Format:**
```json
{
  "timestamp": "2025-12-17T22:16:39.368740Z",
  "session_id": "855f6568-...",
  "type": "feedback_delivered",
  "context": "The feedback text...",
  "trigger": null
}
```

**YAML Migration:** Legacy `.yaml` decision files can be converted to JSON:
```bash
#!/bin/bash
# IMPORTANT: Backup .superego/ before running!
set -e

find .superego -name "*.yaml" -path "*/decisions/*" | while IFS= read -r f; do
  # Extract fields
  timestamp=$(grep "^timestamp:" "$f" | cut -d' ' -f2-)
  session_id=$(grep "^session_id:" "$f" | cut -d' ' -f2-)
  dtype=$(grep "^type:" "$f" | cut -d' ' -f2-)
  context=$(awk '/^context:/{flag=1;next}/^[a-z_]+:/{flag=0}flag' "$f" | sed 's/^  //')

  # Use environment variables for safe JSON encoding
  json_out=$(TIMESTAMP="$timestamp" SESSION_ID="$session_id" DTYPE="$dtype" CONTEXT="$context" python3 -c "
import os, json
print(json.dumps({
    'timestamp': os.environ['TIMESTAMP'],
    'session_id': None if os.environ['SESSION_ID'] == 'null' else os.environ['SESSION_ID'],
    'type': os.environ['DTYPE'],
    'context': os.environ['CONTEXT'].strip(),
    'trigger': None
}, indent=2))
")

  # Validate JSON before writing
  if echo "$json_out" | python3 -m json.tool > /dev/null 2>&1; then
    echo "$json_out" > "${f%.yaml}.json"
    echo "Converted: $f"
  else
    echo "ERROR: Invalid JSON for $f" >&2
  fi
done
```

## Debugging

### Evaluation failures
Check `.superego/hook.log` for recent activity:
```bash
tail -50 .superego/hook.log
```

### Common issues

**"EOF while parsing" error:** stdout not piped in `claude.rs`. The `invoke()` function MUST have:
```rust
cmd.stdout(Stdio::piped());
cmd.stderr(Stdio::piped());
```
Without this, `wait_with_output()` returns empty and JSON parsing fails.

**No decisions recorded:** Check that evaluations complete successfully in hook.log. Look for "Evaluation complete" not "ERROR".

**SKIP messages in log:**
- `SUPEREGO_DISABLED=1` - Normal for superego's own Claude calls (recursion prevention)
- `stop_hook_active=true` - Normal, prevents infinite loops after blocking once

### Testing the full flow
```bash
# Trigger evaluation manually
sg evaluate-llm --transcript-path <path-to-jsonl> --session-id test

# Check for new decision files
find .superego -name "*.json" -path "*/decisions/*" -mmin -5

# Verify hook receives transcript path
grep "Running:" .superego/hook.log | tail -5
```
