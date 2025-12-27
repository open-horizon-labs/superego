# Superego

A metacognitive advisor for AI coding assistants. Monitors conversations, evaluates the assistant's approach, and provides feedback before finishing.

**Supported platforms:**
- **Claude Code** - Full support via plugin
- **OpenAI Codex CLI** - Alpha support via skill (see [codex-skill/](codex-skill/))
- **OpenCode** - Alpha support via TypeScript plugin (see [opencode-plugin/](opencode-plugin/))

## What It Does

When you use Claude Code (or OpenCode) with superego enabled:

1. **Session starts** - Claude is told superego is active and to take feedback seriously
2. **Claude works** - You interact normally with Claude
3. **Before large edits** - Superego evaluates proposed changes in context (Edit/Write over 20 lines)
4. **Before Claude finishes** - Superego evaluates the full conversation
5. **If concerns found** - Claude is blocked and shown the feedback
6. **Claude continues** - Incorporates feedback, may ask you clarifying questions
7. **Clean exit** - Once addressed (or no concerns), Claude finishes normally

This creates feedback loops where Claude can course-correct both during work and before presenting results.

## Quickstart: Claude Code

```bash
# 1. Install the plugin
/plugin marketplace add cloud-atlas-ai/superego
/plugin install superego@superego

# 2. Initialize in your project (installs binary if needed)
/superego:init
```

The `/superego:init` command detects if the binary is missing and offers to install it via Homebrew or Cargo.

### Level Up: Strategic Alignment with Open Horizons

Superego provides metacognitive feedback—but feedback without strategic context is incomplete. For the full power of aligned AI development, combine superego with **[Open Horizons MCP](https://github.com/cloud-atlas-ai/oh-mcp-server)**:

```bash
# Add OH MCP marketplace
/plugin marketplace add cloud-atlas-ai/oh-mcp-server
/plugin install oh-mcp@oh-mcp-server

# Configure
/oh-mcp:setup
```

**What you get:**
- Superego monitors *how* Claude works (metacognitive feedback)
- OH MCP connects *why* Claude works (strategic alignment)
- Every decision traces back to your missions and aims
- Claude logs decisions directly to your strategic framework

Learn more: [OH MCP Server](https://github.com/cloud-atlas-ai/oh-mcp-server) | [Open Horizons](https://app.openhorizons.me)

### Slash Commands

| Command | Description |
|---------|-------------|
| `/superego:init` | Initialize superego for this project (offers binary install if needed) |
| `/superego:status` | Check if plugin, binary, and project are configured |
| `/superego:prompt` | Manage prompts: list, switch (code/writing), show current |
| `/superego:enable` | Enable superego (offers init if not set up) |
| `/superego:disable` | Temporarily disable for current session |
| `/superego:remove` | Remove superego from project |

### Updating

```bash
claude plugin marketplace update superego   # Refresh cache
claude plugin update superego@superego      # Update plugin
# Restart Claude Code to apply
```

### Manual binary installation

If you prefer to install the `sg` binary manually instead of via `/superego:init`:

```bash
# Homebrew (macOS)
brew install cloud-atlas-ai/superego/superego

# Cargo (cross-platform)
cargo install superego

# From source
git clone https://github.com/cloud-atlas-ai/superego.git
cd superego && cargo install --path .
```

Then run `sg init` in your project to create `.superego/`.

## Quickstart: OpenCode (Alpha)

OpenCode support is in alpha. It uses a TypeScript plugin that runs entirely within OpenCode—no separate binary needed.

```bash
# 1. Download plugin to your project
mkdir -p .opencode/plugin
curl -L -o .opencode/plugin/superego.js \
  https://github.com/cloud-atlas-ai/superego/releases/latest/download/index.js

# 2. Start OpenCode and initialize
opencode
# Ask: "use superego init"
```

Or install globally:
```bash
mkdir -p ~/.config/opencode/plugin
curl -L -o ~/.config/opencode/plugin/superego.js \
  https://github.com/cloud-atlas-ai/superego/releases/latest/download/index.js
```

See [opencode-plugin/README.md](opencode-plugin/README.md) for build-from-source instructions and detailed configuration.

## Quickstart: OpenAI Codex CLI (Alpha)

Codex support uses a skill that the agent can invoke at decision points.

```bash
# 1. Install the skill
mkdir -p ~/.codex/skills/superego
curl -L -o ~/.codex/skills/superego/SKILL.md \
  https://raw.githubusercontent.com/cloud-atlas-ai/superego/main/codex-skill/SKILL.md

# 2. In Codex, ask the agent to set up:
#    "$superego init"
```

The `$superego init` command installs the binary, creates `.superego/`, and adds AGENTS.md guidance automatically.

After setup, the agent calls `$superego` at decision points to evaluate the conversation.

See [codex-skill/](codex-skill/) for details.

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

### Prompt Types

Superego ships with multiple prompts for different use cases:

| Prompt | Description |
|--------|-------------|
| `code` | Metacognitive advisor for coding agents (default) |
| `writing` | Co-author reviewer for writing and content creation |

Switch prompts via CLI or slash command:

```bash
sg prompt list              # Show available prompts
sg prompt switch writing    # Switch to writing prompt
sg prompt show              # Show current prompt info

# Or in Claude Code:
/superego:prompt switch writing
```

Your customizations are preserved when switching—each prompt type has its own backup (`prompt.<type>.md.bak`).

### Custom Prompt Editing

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
sg prompt list       # Show available prompts
sg prompt switch X   # Switch to prompt X (code, writing)
sg prompt show       # Show current prompt info
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
