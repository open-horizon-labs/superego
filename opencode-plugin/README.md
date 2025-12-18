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

## Quick Start

### Option A: Project-level install (recommended)

```bash
# In your project directory
cd /path/to/your/project

# 1. Clone and build the plugin
git clone https://github.com/cloud-atlas-ai/superego.git /tmp/superego
cd /tmp/superego/opencode-plugin
bun install
bun build src/index.ts --outdir dist --target bun

# 2. Install to your project
mkdir -p /path/to/your/project/.opencode/plugin
cp dist/index.js /path/to/your/project/.opencode/plugin/superego.js

# 3. Initialize superego config
mkdir -p /path/to/your/project/.superego
cp /tmp/superego/default_prompt.md /path/to/your/project/.superego/prompt.md

# 4. Start OpenCode
opencode
```

### Option B: Global install (all projects)

```bash
# 1. Clone and build the plugin
git clone https://github.com/cloud-atlas-ai/superego.git /tmp/superego
cd /tmp/superego/opencode-plugin
bun install
bun build src/index.ts --outdir dist --target bun

# 2. Install globally
mkdir -p ~/.config/opencode/plugin
cp dist/index.js ~/.config/opencode/plugin/superego.js

# 3. Initialize superego in each project you want oversight
cd /path/to/your/project
mkdir -p .superego
cp /tmp/superego/default_prompt.md .superego/prompt.md
```

## Test Plan

After installation, verify each step in order:

| Step | What to do | Expected log output |
|------|------------|---------------------|
| 1. Plugin loads | Start OpenCode in a project with `.superego/` | `[superego] Plugin loaded` |
| 2. Session created | Start a new chat | `[superego] Session created: <uuid>` |
| 3. Contract injected | (automatic) | `[superego] Contract injected` |
| 4. Have a conversation | Ask OpenCode to do something, wait for response | `[superego] Session idle: <uuid>` |
| 5. Evaluation runs | (automatic on idle) | `[superego] Got N messages`, `[superego] Calling LLM via OpenCode...` |
| 6. Response logged | (automatic) | `[superego] LLM response: DECISION: ALLOW...` or `BLOCK...` |

### Troubleshooting

- **"Not initialized, skipping"**: Create `.superego/` directory with `prompt.md`
- **"No prompt.md found"**: Copy `default_prompt.md` to `.superego/prompt.md`
- **No logs at all**: Check OpenCode console output, verify plugin file is named `superego.js`

## Configuration

Requires:
- `.superego/prompt.md` - evaluation criteria (same as Claude Code)
- OpenCode configured with an LLM provider

**Key architectural advantage:** The OpenCode plugin uses OpenCode's own session/LLM API for evaluation. This means:
- **No separate API keys** - uses whatever model/provider OpenCode is configured with
- **No `sg` binary required** - pure TypeScript, no Rust toolchain needed
- **Same model for work and oversight** - evaluation runs on the same LLM as the main session

This is a departure from the Claude Code plugin which shells out to `claude -p`. Consider backporting this pattern to Claude Code for consistency.

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
