# Decision Record: Superego for OpenAI Codex

**Date:** 2025-12-21
**Status:** Accepted
**Context:** Implementing superego metacognitive oversight for OpenAI Codex CLI

## Background

Superego provides metacognitive oversight for AI coding assistants. It currently supports:
- **Claude Code** via plugin hooks (SessionStart, Stop, PreToolUse)
- **OpenCode** via TypeScript plugin SDK (`@opencode-ai/plugin`)

Both implementations use automatic hooks to:
1. Inject the superego contract on session start
2. Evaluate conversations when the model finishes (or before large edits)
3. Block and provide feedback if concerns are found

## Research: Codex Extension Model

We investigated four potential extension mechanisms:

### 1. Plugin/Hook System
**Finding:** Codex does NOT have a plugin system like OpenCode.

The Codex TypeScript SDK (`@openai/codex-sdk`) is for **driving Codex externally**, not for plugins that run inside Codex. Event hooks are an [open feature request (#2109)](https://github.com/openai/codex/issues/2109) with 150+ upvotes but not yet implemented.

> "Let us define event hooks with pattern matching, to trigger scripts/commands before/after codex behaviors."

Related PRs (#2904, #4522) have been opened and closed, indicating ongoing but incomplete work.

### 2. Skills System
**Finding:** Skills are Codex's native extension mechanism.

Skills are directories with `SKILL.md` files containing:
- YAML frontmatter (name, description)
- Markdown body with instructions

Skills are:
- Stored in `~/.codex/skills/**/SKILL.md` (user) or `.codex/skills/` (repo)
- Enabled via `[features] skills = true` in `~/.codex/config.toml`
- Invoked explicitly via `$skill-name` or implicitly when task matches description
- **NOT automatic** - Codex chooses whether to use them

References:
- [Skills Documentation](https://github.com/openai/codex/blob/main/docs/skills.md)
- [Skills Developer Reference](https://developers.openai.com/codex/skills/)

### 3. AGENTS.md
**Finding:** Hierarchical instruction injection, similar to CLAUDE.md.

AGENTS.md files are loaded from:
- `~/.codex/AGENTS.md` (global)
- Repository root and parent directories

This provides system prompt injection but no enforcement mechanism.

### 4. MCP Servers
**Finding:** Codex supports MCP for external tools.

MCP servers can provide tools that Codex calls, but:
- Codex must choose to call the tool
- No automatic triggering on events
- No blocking capability

## Comparison: Extension Capabilities

| Capability | Claude Code | OpenCode | Codex |
|------------|-------------|----------|-------|
| Automatic session start hook | Yes | Yes | No |
| Automatic evaluation on completion | Yes | Yes | No |
| Block before tool execution | Yes | Partial | No |
| System prompt injection | Yes (hook) | Yes (transform) | Yes (AGENTS.md) |
| Custom tools | Yes (MCP) | Yes (tool API) | Yes (MCP, Skills) |
| Plugin SDK | Shell scripts | TypeScript | None |

## Decision

**Approach:** Implement superego for Codex as a **Skill** with AGENTS.md guidance.

### What We Can Achieve

1. **`$superego` skill** - Codex can invoke to run evaluation
2. **AGENTS.md injection** - Remind Codex to use superego before large changes
3. **Manual evaluation** - Users can ask Codex to "use $superego"

### What We Cannot Achieve (Without Hooks)

1. **Automatic evaluation** - Cannot trigger on session idle/completion
2. **Blocking** - Cannot prevent Codex from proceeding
3. **PreToolUse gates** - Cannot intercept before large edits

### Future Path

If/when Codex implements event hooks (Issue #2109), we can:
1. Add automatic evaluation on `turn.completed`
2. Add blocking capability
3. Potentially create a TypeScript plugin similar to OpenCode

### Alternative Considered: SDK Wrapper

We could wrap Codex with a script using `@openai/codex-sdk`:

```typescript
for await (const event of runStreamed(thread)) {
  if (event.type === "turn.completed") {
    // Run superego evaluation
  }
}
```

**Rejected because:**

The SDK "spawns the CLI and exchanges JSONL events over stdin/stdout" - meaning it's designed for **headless/programmatic use**. A wrapper approach would require either:

1. **Headless-only mode** - Users lose the Codex TUI experience entirely
2. **Pass-through TUI** - We'd need to reimplement displaying Codex output to users

Neither is "minimal hooks" - both represent significant UX degradation or maintenance burden.

Additional concerns:
- Changes how users run Codex (must use wrapper instead of `codex` directly)
- Higher friction for adoption
- Ongoing compatibility maintenance as Codex evolves

## Implementation Plan

1. Create `codex-skill/` directory with:
   - `SKILL.md` - Skill definition with evaluation instructions
   - `scripts/evaluate.sh` - Shell script to run `sg evaluate-llm`
   - `README.md` - Installation and usage instructions

2. Provide AGENTS.md snippet for users to add superego guidance

3. Update `sg init` to optionally set up Codex skill

## References

- [Codex Skills Docs](https://github.com/openai/codex/blob/main/docs/skills.md)
- [Codex MCP Docs](https://developers.openai.com/codex/mcp/)
- [Event Hooks Feature Request](https://github.com/openai/codex/issues/2109)
- [OpenCode Plugin Implementation](../opencode-plugin/)
