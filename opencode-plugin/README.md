# Superego OpenCode Plugin

TypeScript adapter for running superego with [OpenCode](https://opencode.ai).

## Status: In Development

This plugin enables superego's metacognitive oversight for OpenCode users, using the same `.superego/` configuration as the Claude Code plugin.

## Architecture

Superego uses a **shared core with adapters** pattern:

```text
.superego/                    # Shared core (language-agnostic)
├── prompt.md                 # Evaluation criteria
├── config.yaml               # Settings (threshold, model, etc.)
├── sessions/<id>/            # Per-session state & decisions
│   ├── state.json
│   ├── feedback
│   └── decisions/
└── ...

plugin/                       # Claude Code adapter (shell scripts)
opencode-plugin/              # OpenCode adapter (TypeScript)
```

### What's Shared

- Evaluation prompt (`prompt.md`)
- Configuration schema (`config.yaml`)
- Decision format: `DECISION: ALLOW|BLOCK\n\n<feedback>`
- Session state and decision journal structure

### What Adapters Handle

| Concern | Claude Code | OpenCode |
|---------|-------------|----------|
| Hook registration | Shell scripts in `plugin/` | TypeScript in `.opencode/plugin/` |
| LLM invocation | Claude CLI | Configurable (Gemini, etc.) |
| Transcript access | `$CLAUDE_TRANSCRIPT_PATH` env var | `client.session.messages()` SDK |
| Feedback delivery | Block hook with JSON | TBD |

## Hook Mapping

| Superego Hook | Claude Code | OpenCode |
|---------------|-------------|----------|
| Session start (inject contract) | `SessionStart` | `session.created` |
| Pre-tool evaluation | `PreToolUse` | `tool.execute.before` |
| Final evaluation | `Stop` | `session.idle` |

## Installation & Testing

```bash
# 1. Build the plugin
cd opencode-plugin
bun install
bun build src/index.ts --outdir dist --target bun

# 2. Copy to OpenCode plugin directory (project-level)
mkdir -p /path/to/your/project/.opencode/plugin
cp dist/index.js /path/to/your/project/.opencode/plugin/superego.js

# Or global:
mkdir -p ~/.config/opencode/plugin
cp dist/index.js ~/.config/opencode/plugin/superego.js

# 3. Ensure project has .superego/ initialized
# (prompt.md is required for evaluation)
```

## What to Test

1. **Plugin loads**: Look for `[superego] Plugin loaded` in console
2. **Session created**: Look for `[superego] Session created: <id>`
3. **Contract injection**: Look for `[superego] Contract injected`
4. **Session idle**: After model finishes, look for `[superego] Session idle: <id>`
5. **Message structure**: Plugin logs first message structure for validation
6. **LLM call**: Look for `[superego] Calling LLM via OpenCode...` and response

## Configuration

Requires:
- `.superego/prompt.md` - evaluation criteria (same as Claude Code)
- OpenCode configured with an LLM provider (uses whatever model OpenCode is configured with)

## Known Limitations (Needs Validation)

- `session.created` event structure assumed (`properties.id`)
- `client.session.messages()` response structure assumed
- `client.session.prompt()` API for contract injection untested
- No UI notification for feedback (writes to file only)

## Development

```bash
cd opencode-plugin
bun install
bun run typecheck  # Check types
bun build src/index.ts --outdir dist --target bun
```
