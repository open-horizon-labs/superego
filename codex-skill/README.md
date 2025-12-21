# Superego Skill for OpenAI Codex

A Codex skill that provides metacognitive oversight via superego.

See [DECISION.md](./DECISION.md) for architectural context and why this is advisory-only.

## Limitations

Unlike the Claude Code and OpenCode plugins, the Codex skill is **advisory-only**:

- No automatic evaluation on session completion
- No blocking capability
- Requires explicit invocation via `$superego`

This is due to Codex not having a plugin/hook system. See [GitHub Issue #2109](https://github.com/openai/codex/issues/2109) for the feature request.

## Installation

### 1. Install the `sg` binary

```bash
# Via Homebrew (recommended)
brew install cloud-atlas-ai/tap/superego

# Or via Cargo
cargo install superego
```

### 2. Enable skills in Codex

Add to `~/.codex/config.toml`:
```toml
[features]
skills = true
```

### 3. Install the skill

```bash
mkdir -p ~/.codex/skills/superego
curl -o ~/.codex/skills/superego/SKILL.md \
  https://raw.githubusercontent.com/cloud-atlas-ai/superego/main/codex-skill/SKILL.md
```

### 4. Initialize superego in your project

```bash
cd /path/to/your/project
sg init
```

### 5. (Recommended) Add AGENTS.md guidance

Add to your project's `AGENTS.md` or `~/.codex/AGENTS.md`:

```markdown
## Superego Metacognitive Oversight

This project uses superego. You have the `$superego` skill available.

**Use $superego before:**
- Large changes (10+ lines across multiple files)
- Refactoring or architectural decisions
- Claiming work is "done"

When superego reports concerns, STOP and show them to the user before proceeding.
```

This tells Codex to use superego proactively, making it semi-automatic.

## Usage

In Codex, invoke the skill by typing `$superego` in your message:

```
$superego

Please evaluate my current approach before I proceed with the refactoring.
```

Or use the `/skills` command to browse and select.

## What It Does

When invoked, the skill:

1. Finds your most recent Codex session automatically
2. Sends the conversation to superego for evaluation
3. Reports any concerns about:
   - Intent clarity (is the goal clear?)
   - X-Y problems (solving the right problem?)
   - Necessity (needed now vs hypothetical?)
   - Local maxima (alternatives explored?)
   - Simplicity (could be simpler?)
   - Alignment (fits the stated goal?)

## Commands

The skill uses the `sg evaluate-codex` command under the hood:

```bash
# Run evaluation manually
sg evaluate-codex

# Output:
# {"has_concerns": true, "cost_usd": 0.05}
# Feedback:
# <feedback text>
```

## Requirements

- OpenAI Codex CLI v0.70+ with skills feature enabled
- `sg` binary installed (superego v0.4.5+)
- `.superego/` initialized in your project
