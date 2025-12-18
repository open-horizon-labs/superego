# ADR-001: Plugin-Based Distribution with Separate Binary Install

## Status

Accepted

## Context

Superego is a metacognitive advisor for Claude Code that monitors conversations and provides feedback via hooks. Prior to v0.4.0, superego was distributed as a single binary that embedded hook scripts and installed them directly into `.claude/hooks/` during `sg init`.

**Problems with the embedded approach:**
- Hook updates required reinstalling the binary and re-running `sg init`
- Tight coupling between binary releases and hook behavior
- No separation between "plugin logic" (hooks) and "evaluation logic" (binary)
- Limited discoverability - users had to know about superego to find it

**Goal:** Improve reach and availability by distributing superego as a Claude Code plugin.

**Constraint discovered:** Claude Code and Opencode have architecturally incompatible plugin systems (shell scripts vs JS/TS modules). This ADR focuses on Claude Code; Opencode support is deferred.

## Decision

Distribute superego as two separate components:

1. **Plugin** (`/plugin marketplace add` / `/plugin install`)
   - Contains hook definitions and shell scripts
   - Lives in `plugin/` directory with `.claude-plugin/plugin.json` manifest
   - Installed globally per-user via Claude Code's plugin system
   - Activates only in projects with `.superego/` directory

2. **Binary** (`cargo install superego`)
   - Contains evaluation logic, LLM integration, state management
   - Installed separately via cargo
   - Called by plugin hook scripts

### Bootstrap Flow (Binary Missing)

When the plugin detects the binary is missing:

1. SessionStart hook checks for `sg` binary
2. If missing, injects `additionalContext` informing Claude of the situation
3. Claude asks user if they want to install via `cargo install superego`
4. User approves or declines
5. If approved, Claude runs the install command
6. If declined, session continues with superego features disabled

This "Claude-mediated approval" pattern keeps the user in control while providing a smooth onboarding experience.

**Note:** After successful binary installation, evaluation hooks (Stop, PreToolUse, etc.) will work immediately within the same session. However, the full superego contract message is only injected at SessionStart, so a new session will see the complete "SUPEREGO ACTIVE" context.

### Install Paths

**From GitHub (primary):**
```bash
/plugin marketplace add cloud-atlas-ai/superego
/plugin install superego@superego
cargo install superego  # Or let Claude install on first session
```

**From local clone (development):**
```bash
/plugin marketplace add /path/to/superego
/plugin install superego@superego
cargo install --path /path/to/superego
```

## Alternatives Considered

### Alternative 1: Bundled Binary in Plugin

Package pre-built binaries within the plugin itself.

**Rejected because:**
- Platform detection complexity (macOS arm64/x86, Linux, Windows)
- Security concerns with distributing binaries
- Plugin size bloat
- Still requires Rust for building releases

### Alternative 2: Auto-Install Without Approval

Plugin automatically runs `cargo install` without asking.

**Rejected because:**
- Violates user consent (running commands without approval)
- May fail silently if Rust not installed
- Unexpected long install during session start

### Alternative 3: Block Session Until Binary Installed

SessionStart hook returns `{"continue": false}` if binary missing.

**Rejected because:**
- Poor UX - session won't start at all
- User may not understand why
- No graceful degradation

### Alternative 4: Homebrew / Package Manager Distribution

Publish to Homebrew, apt, etc.

**Deferred (not rejected):** Can be added later as supplementary install method. Not pursuing now due to maintenance overhead and platform limitations (Homebrew = macOS primarily).

## Consequences

### Positive

- **Cleaner separation**: Hooks evolve independently of evaluation logic
- **Easier updates**: Plugin updates don't require cargo reinstall
- **Better discoverability**: Plugin can be listed in marketplaces
- **User control**: Explicit approval before binary install
- **Graceful degradation**: Session works even without binary

### Negative

- **Two-step install**: Users must install both plugin and binary
- **Rust requirement**: Binary install requires Rust toolchain (unless pre-built binaries added later)
- **Breaking change**: Users upgrading from <v0.4.0 must run `sg migrate`
- **Coordination**: Two release artifacts (plugin version, binary version) to keep compatible

### Neutral

- **Migration path**: `sg migrate` command removes legacy hooks
- **Per-project activation**: Plugin is global, but only activates with `.superego/`

## Related

- PR #1: Port superego to Claude Code plugin
- Future: ADR for Opencode plugin support (different architecture needed)
- Future: ADR for pre-built binary distribution (if Homebrew/releases added)
