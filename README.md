# Superego

A metacognitive advisor for Claude Code. Monitors conversations, evaluates Claude's approach, and provides feedback that Claude sees and acts on before finishing.

## What It Does

When you use Claude Code with superego enabled:

1. **Session starts** - Claude is told superego is active and to take feedback seriously
2. **Claude works** - You interact normally with Claude
3. **Before large edits** - Superego evaluates proposed changes in context (Edit/Write over 20 lines)
4. **Before Claude finishes** - Superego evaluates the full conversation
5. **If concerns found** - Claude is blocked and shown the feedback
6. **Claude continues** - Incorporates feedback, may ask you clarifying questions
7. **Clean exit** - Once addressed (or no concerns), Claude finishes normally

This creates feedback loops where Claude can course-correct both during work and before presenting results.

## Quickstart

```bash
# Build and install
git clone <repo>
cd higher-peak
cargo build --release
sudo cp target/release/sg /usr/local/bin/

# Initialize in your project
cd /path/to/your/project
sg init

# Start Claude Code - superego is now active
claude
```

That's it. Superego runs automatically via Claude Code hooks.

## What You'll See

When superego has feedback, Claude will continue working instead of stopping, addressing concerns like:
- Scope drift from the current task
- Missing error handling or edge cases
- Approaches that don't align with project conventions
- Incomplete implementations

If Claude disagrees with non-trivial feedback, it will escalate to you for a decision.

## Debugging

### Check if hooks are firing
```bash
tail -f .superego/hook.log
```

You'll see:
```
[15:42:01] Hook fired
[15:42:01] Running: sg evaluate-llm
[15:42:03] Evaluation complete
[15:42:03] Blocking with feedback: Consider error handling...
```

### Check superego state
```bash
cat .superego/state.json        # Last evaluation timestamp
cat .superego/feedback          # Pending feedback (if any)
ls .superego/decisions/         # Audit trail of all feedback
```

### Manual evaluation
```bash
sg evaluate-llm --transcript-path ~/.claude/projects/<project>/transcript.jsonl
```

### Reset everything
```bash
sg reset    # Removes all superego files and hooks
sg init     # Fresh start
```

## Customization

Edit `.superego/prompt.md` to customize what superego evaluates:
- Add project-specific guidelines
- Adjust strictness
- Focus on particular concerns

### Environment Variables

- `SUPEREGO_DISABLED=1` - Disable superego entirely
- `SUPEREGO_CHANGE_THRESHOLD=N` - Lines required to trigger PreToolUse evaluation (default: 20)

## How It Works

```
SessionStart hook
    └── Injects contract: "SUPEREGO ACTIVE: critically evaluate feedback..."

PreToolUse hook (before Edit/Write)
    ├── Checks change size (lines added/modified)
    ├── If >= threshold (default 20): runs sg evaluate-llm with pending change
    ├── If concerns: returns {"decision":"block","reason":"SUPEREGO FEEDBACK: ..."}
    │   └── Claude sees feedback, reconsiders the change
    └── If small or clean: allows tool execution

Stop hook (when Claude tries to finish)
    ├── Runs sg evaluate-llm
    ├── Reads transcript since last evaluation
    ├── Sends to LLM with superego prompt
    ├── If concerns: returns {"decision":"block","reason":"SUPEREGO FEEDBACK: ..."}
    │   └── Claude sees feedback, continues working
    └── If clean: allows stop

PreCompact hook (before context truncation)
    └── Same as Stop - evaluates before transcript is lost
```

## Commands

```bash
sg init              # Initialize superego (creates .superego/, configures hooks)
sg reset             # Remove all superego files and hooks
sg evaluate-llm      # Run LLM evaluation (called by hooks)
sg has-feedback      # Check for pending feedback (exit 0=yes, 1=no)
sg get-feedback      # Get and clear pending feedback
```

## Requirements

- Claude Code CLI
- `jq` (for hook JSON parsing)
- Rust toolchain (to build)

## License

MIT
