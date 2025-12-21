# Superego Skill for OpenAI Codex

A Codex skill that provides metacognitive oversight via superego.

## Status: In Development

See [DECISION.md](./DECISION.md) for architectural context and constraints.

## Limitations

Unlike the Claude Code and OpenCode plugins, the Codex skill is **advisory-only**:

- No automatic evaluation on session completion
- No blocking capability
- Requires explicit invocation via `$superego`

This is due to Codex not having a plugin/hook system. See [GitHub Issue #2109](https://github.com/openai/codex/issues/2109) for the feature request.

## Installation

```bash
# Enable skills in Codex config
echo '[features]
skills = true' >> ~/.codex/config.toml

# Install the skill
mkdir -p ~/.codex/skills/superego
cp SKILL.md ~/.codex/skills/superego/

# Initialize superego in your project
sg init
```

## Usage

In Codex, invoke the skill:
- Explicitly: Type `$superego` in your message
- Via `/skills` command to browse and select

## Requirements

- OpenAI Codex CLI with skills feature enabled
- `sg` binary installed (`cargo install superego` or via Homebrew)
- `.superego/` initialized in your project
