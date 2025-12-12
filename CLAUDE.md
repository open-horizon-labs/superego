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
- `init.rs` - Creates `.superego/` directory structure and configures Claude Code hooks
- `evaluate.rs` - LLM-based evaluation logic; calls Claude to review conversation transcripts
- `claude.rs` - Wrapper for invoking Claude CLI (`claude -p --output-format json`)
- `transcript/` - Parses Claude Code JSONL transcript files
  - `types.rs` - Serde structs for transcript entries (User, Assistant, Summary, etc.)
  - `reader.rs` - Reads and filters transcript messages since last evaluation
- `bd.rs` - Integration with beads (`bd`) task tracking; provides current task context
- `state.rs` - Manages `.superego/state.json` (last_evaluated timestamp)
- `decision.rs` - Decision journal for audit trail (`.superego/decisions/`)
- `feedback.rs` - Feedback queue (`.superego/feedback` file)

### Hook Scripts (embedded in binary via `include_str!`)

Located in `hooks/`:
- `session-start.sh` - Injects superego contract via `additionalContext`
- `evaluate.sh` - Runs `sg evaluate-llm`, blocks if concerns found
- `pre-tool-use.sh` - Evaluates pending changes before large Edit/Write operations

### Key Design Patterns

**Recursion prevention:** Superego's own Claude calls set `SUPEREGO_DISABLED=1` to prevent hooks from triggering on themselves.

**Decision format:** LLM responses must follow `DECISION: ALLOW|BLOCK\n\n<feedback>` format. Unknown decisions default to BLOCK for safety.

**State tracking:** `last_evaluated` timestamp in state.json ensures only new conversation content is evaluated.

## Environment Variables

- `SUPEREGO_DISABLED=1` - Disables superego entirely
- `SUPEREGO_CHANGE_THRESHOLD=N` - Lines required to trigger PreToolUse evaluation (default: 20)

## Files Created by `sg init`

```
.superego/
├── prompt.md          # Customizable system prompt for evaluation
├── state.json         # Evaluation state (last_evaluated timestamp)
├── config.yaml        # Placeholder config
├── decisions/         # Decision journal (audit trail)
├── session/           # Superego Claude session persistence
└── feedback           # Pending feedback queue (transient)

.claude/
├── settings.json      # Hook configuration
└── hooks/superego/    # Hook scripts
```
