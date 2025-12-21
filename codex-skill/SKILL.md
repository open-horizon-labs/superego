---
name: superego
description: Evaluate this conversation for alignment, focus, and proportionality. Use before large changes or when unsure.
---

# Superego Evaluation

Run the superego metacognitive evaluation on the current Codex session.

## Usage

Execute this command:

```bash
sg evaluate-codex
```

This will:
1. Find the most recent Codex session file automatically
2. Analyze the conversation against superego criteria
3. Report any concerns found

## Output

The command outputs JSON to stdout:
```json
{"has_concerns": true, "cost_usd": 0.05}
```

Feedback text is printed to stderr.

## What To Do With Feedback

**If `has_concerns` is true:**
1. **STOP** what you were doing
2. **SHOW** the feedback to the user verbatim (quote it)
3. **ASK** the user: "Superego raised concerns. Should I address these before continuing, or proceed anyway?"
4. **WAIT** for user response before taking further action

**If `has_concerns` is false:**
- Tell the user "Superego: No concerns with current approach"
- Continue with the task

**Important:** Do NOT silently incorporate feedback. The user must see it and decide.

## Requirements

- `sg` binary must be installed
- `.superego/` directory must exist in the project (run `sg init`)

## Installation

If `sg` is not installed:
```bash
# Via Homebrew
brew install cloud-atlas-ai/tap/superego

# Or via Cargo
cargo install superego
```

If `.superego/` doesn't exist:
```bash
sg init
```
