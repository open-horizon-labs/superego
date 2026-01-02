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
# Create directory structure
mkdir -p ~/.codex/skills/superego/agents

# Install skill definition
curl -o ~/.codex/skills/superego/SKILL.md \
  https://raw.githubusercontent.com/cloud-atlas-ai/superego/main/codex-skill/SKILL.md

# Install agent files (optional, for advisory roles)
curl -o ~/.codex/skills/superego/agents/code.md \
  https://raw.githubusercontent.com/cloud-atlas-ai/superego/main/codex-skill/agents/code.md
curl -o ~/.codex/skills/superego/agents/writing.md \
  https://raw.githubusercontent.com/cloud-atlas-ai/superego/main/codex-skill/agents/writing.md
curl -o ~/.codex/skills/superego/agents/learning.md \
  https://raw.githubusercontent.com/cloud-atlas-ai/superego/main/codex-skill/agents/learning.md
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

### Core Evaluation (Stable)

```bash
$superego               # Evaluate current conversation
sg evaluate-codex       # Manual evaluation

# Output:
# {"has_concerns": true, "tokens": 5000}
# Feedback: <feedback text>
```

### New Features (v0.8.0 - Pending Codex Testing)

The v0.8.0 binary adds these features. They work when running `sg` directly from the command line. Integration via `$superego` skill in Codex is pending testing.

**Multi-Prompt Support:**
```bash
sg prompt list              # List available prompts
sg prompt switch writing    # Switch to writing prompt
sg prompt switch learning   # Switch to learning prompt
sg prompt switch code       # Back to code prompt (default)
sg prompt show              # Show current prompt info
```

Available prompts: **code** (default), **writing**, **learning**

The evaluation respects the active prompt from `.superego/config.yaml`.

**On-Demand Review:**
```bash
sg review            # Review staged changes (git diff --cached)
sg review pr         # Review PR diff vs base branch
sg review <file>     # Review specific file
```

**Agent Files:**

Three agent files are available for copying into your project's `AGENTS.md`:
- `~/.codex/skills/superego/agents/code.md` - Conversational coding advisor
- `~/.codex/skills/superego/agents/writing.md` - Conversational writing reviewer
- `~/.codex/skills/superego/agents/learning.md` - Conversational learning coach

Copy the relevant agent content into your `AGENTS.md` to give Codex specialized advisory guidance.

See [SKILL.md](./SKILL.md) for the full skill definition including these new commands.

## Testing Status

**What's been tested:**
- ✅ Core evaluation via `$superego` skill
- ✅ Binary commands work when run directly
- ✅ Multi-prompt support in `sg evaluate-codex`
- ✅ bd/wm integration

**Pending verification in Codex sessions:**
- ❓ `$superego prompt ...` commands via skill
- ❓ `$superego review ...` commands via skill
- ❓ Agent file workflow in practice

If you test these features, please report results in a GitHub issue.

## Requirements

- OpenAI Codex CLI v0.70+ with skills feature enabled
- `sg` binary installed (superego v0.4.5+)
- `.superego/` initialized in your project
