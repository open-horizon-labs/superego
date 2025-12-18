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
# 1. Install the Claude Code plugin
/plugin marketplace add cloud-atlas-ai/superego
/plugin install superego@superego

# 2. Initialize in your project (installs binary if needed)
/superego:init
```

That's it. The `/superego:init` command detects if the binary is missing and offers to install it via Homebrew or Cargo.

## Slash Commands

| Command | Description |
|---------|-------------|
| `/superego:init` | Initialize superego for this project (offers binary install if needed) |
| `/superego:status` | Check if plugin, binary, and project are configured |
| `/superego:enable` | Enable superego (offers init if not set up) |
| `/superego:disable` | Temporarily disable for current session |
| `/superego:remove` | Remove superego from project |

## Manual Installation

If you prefer to install the binary manually:

**Homebrew (macOS):**
```bash
brew install cloud-atlas-ai/superego/superego
```

**Cargo (cross-platform):**
```bash
cargo install superego
```

**From source:**
```bash
git clone https://github.com/cloud-atlas-ai/superego.git
cd superego
cargo install --path .
```

Then run `sg init` in your project to create the `.superego/` configuration.

## Plugin Installation Details

Claude Code uses a marketplace system for plugins. The superego plugin contains hook scripts that run automatically.

**From GitHub (recommended):**
```bash
/plugin marketplace add cloud-atlas-ai/superego
/plugin install superego@superego
```

**From a local clone (for development):**
```bash
/plugin marketplace add /absolute/path/to/superego
/plugin install superego@superego
```

The plugin includes:
- `hooks/hooks.json` - Defines which events trigger superego
- `scripts/*.sh` - Hook scripts that call the `sg` binary
- `commands/*.md` - Slash commands for superego management

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
sg reset    # Removes .superego/ directory
sg init     # Fresh start
```

### Migrating from legacy hooks

If you previously used `sg init` before v0.4.0 (which created `.claude/hooks/superego/`):
```bash
/plugin marketplace add cloud-atlas-ai/superego
/plugin install superego@superego
sg migrate  # Remove legacy hooks
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

PreToolUse hook (before any tool)
    ├── Checks if periodic eval is due (time-based)
    ├── For Edit/Write: checks change size (lines added/modified)
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
sg init              # Initialize superego (creates .superego/)
sg migrate           # Remove legacy hooks (for users upgrading from < v0.4.0)
sg reset             # Remove .superego/ directory
sg evaluate-llm      # Run LLM evaluation (called by hooks)
sg has-feedback      # Check for pending feedback (exit 0=yes, 1=no)
sg get-feedback      # Get and clear pending feedback
sg --version         # Show version
```

## Requirements

- Claude Code CLI
- `jq` (for hook JSON parsing)
- Rust toolchain (to build from source) or Homebrew (for pre-built binary)

## License

Source-available. See [LICENSE](LICENSE) for details.
