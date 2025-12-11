# Superego Architecture

> Metacognitive advisor for Claude Code - watches conversations, gates actions by phase.

## Core Principle

**Phase transitions are user-gated.** Claude's actions cannot change the phase - only user messages can. This enables:
- One LLM evaluation per user turn (not per tool call)
- Instant tool gating against cached phase
- Clean separation of evaluation vs enforcement

## System Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     User sends message                           │
└───────────────────────────┬─────────────────────────────────────┘
                            │
                            ▼
                  UserPromptSubmit hook
                            │
                            ▼
              sg evaluate --transcript-path X     ← LLM call (once per user msg)
                            │
                            ▼
              .superego/state.json updated
              {phase: "ready", scope: "JWT auth"}
                            │
                            ▼
┌─────────────────────────────────────────────────────────────────┐
│              Claude responds, makes tool calls                   │
└───────────────────────────┬─────────────────────────────────────┘
                            │
              ┌─────────────┼─────────────┐
              ▼             ▼             ▼
           Read          Glob         Write
              │             │             │
              ▼             ▼             ▼
         (no hook)    (no hook)    PreToolUse hook
                                        │
                                        ▼
                          sg check --tool-name Write  ← NO LLM, instant
                                        │
                                        ▼
                               Read state.json
                               phase == READY?
                                        │
                                  ┌─────┴─────┐
                                 YES         NO
                                  │           │
                               ALLOW       BLOCK
```

## Phases

| Phase | Description | Write Tools |
|-------|-------------|-------------|
| **EXPLORING** | Gathering context, reading files, understanding codebase | Blocked |
| **DISCUSSING** | Clarifying approach, resolving unknowns, planning | Blocked |
| **READY** | User confirmed approach, clear to implement | Allowed |

### Phase Signals (for LLM inference)

**→ EXPLORING:**
- Session just started
- Reading files without clear goal

**→ DISCUSSING:**
- Questions asked but not answered
- Multiple approaches being considered
- User said "what about...", "actually...", "wait"

**→ READY:**
- User confirmed approach: "ok", "let's do it", "sounds good", "go ahead"
- Clear scope established
- No unresolved unknowns

## Tool Classification

**Read tools** (always allowed, no superego check):
- `Read`, `Glob`, `Grep`, `LS`, `WebFetch`, `WebSearch`

**Write tools** (gated by phase):
- `Edit`, `Write`, `Bash`, `Task`, `NotebookEdit`

**Design decisions:**
- **Bash**: All Bash commands treated as write (even `git status`). Phase checks are instant (cached), so safe to over-gate.
- **Task (subagents)**: Trust parent's phase. Subagents inherit the current phase, no separate gating.

## CLI Commands

### sg init
Initialize superego for a project.

```bash
sg init
```

- Creates `.superego/` directory structure
- Writes default `prompt.md`
- Creates empty `state.json` (phase: exploring)
- Creates `decisions/` folder

### sg evaluate
Called by `UserPromptSubmit` hook. Infers phase from conversation.

```bash
sg evaluate --transcript-path /path/to/session.jsonl
```

- Reads transcript + decision journal
- Calls superego's Claude session with context
- Writes phase to `.superego/state.json`
- Returns (doesn't block user prompt)

### sg check
Called by `PreToolUse` hook (write tools only). Fast, no LLM.

```bash
sg check --tool-name Write
```

- Reads `.superego/state.json`
- Returns JSON: `{"decision": "allow"}` or `{"decision": "block", "reason": "..."}`

### sg acknowledge
Claude calls this to accept feedback.

```bash
sg acknowledge
```

- Writes `feedback_accepted` decision to journal
- Clears pending feedback

### sg override
Claude calls this after user approves override.

```bash
sg override "user approved JWT approach"
```

- Writes `override_granted` decision to journal
- **Scope: Single action only** - allows the next blocked tool call, then re-evaluates
- Does NOT change phase or persist beyond one action
- Forces explicit re-confirmation for safety

### sg history
Query past decisions for context recovery.

```bash
sg history --limit 10
```

- Reads `.superego/decisions/*`
- Returns chronological list

### sg context-inject
Called by `SessionStart` hook. Injects context into Claude.

```bash
sg context-inject
```

- Outputs behavioral contract (how Claude should handle blocks)
- Includes summary of recent decisions from journal
- Returns as `additionalContext` for Claude's session

### sg reset
Recovery from corrupted state.

```bash
sg reset
```

- Clears `state.json` (resets to EXPLORING)
- Optionally clears `session/` (superego's Claude session)
- Preserves `decisions/` journal (audit trail)

### sg precompact
Called by `PreCompact` hook. Snapshots state before context compaction.

```bash
sg precompact --transcript-path /path/to/session.jsonl
```

- Reads current transcript before it's compacted
- Writes summary decision to journal capturing current understanding
- Ensures superego retains context even after Claude's context is compressed

### sg disable / sg enable
Escape hatch to bypass superego.

```bash
sg disable   # Sets disabled flag
sg enable    # Clears disabled flag
```

- When disabled, hooks skip all checks
- Also respects `SUPEREGO_DISABLED=1` env var

## Directory Structure

```
.superego/
├── config.yaml             ← Configuration (model, timeouts, etc.)
├── state.json              ← Current phase (cached between user messages)
├── prompt.md               ← Superego system prompt (customizable, defaults to embedded)
├── audit.log               ← Detailed log of all sg commands for debugging
├── session/                ← Superego's own Claude session (persistent)
└── decisions/              ← Decision journal (project-scoped)
    ├── 2024-01-15T10-30-00Z.yaml
    └── 2024-01-15T10-35-00Z.yaml
```

### state.json

```json
{
  "phase": "discussing",
  "since": "2024-01-15T10:30:00Z",
  "approved_scope": null,
  "last_evaluated": "2024-01-15T10:35:00Z",
  "pending_override": null,
  "disabled": false
}
```

**Override flow:**
1. `sg override "reason"` → sets `pending_override: {reason: "...", timestamp: "..."}`
2. Next `sg check` for write tool → sees `pending_override` → allows action → clears `pending_override`
3. Subsequent `sg check` calls → no override pending → normal phase gating resumes

### Decision Record

```yaml
timestamp: 2024-01-15T10:30:00Z
session_id: abc123              # Which main Claude session
type: phase_transition          # | override_granted | feedback_accepted
from_state: discussing
to_state: ready
context: |
  User confirmed JWT approach for authentication.
  All unknowns about auth flow resolved.
trigger: "user said 'ok let's implement it'"
approved_scope: "Implement JWT authentication in auth.ts"
```

## Hooks Configuration

```json
{
  "hooks": {
    "UserPromptSubmit": [
      {
        "type": "command",
        "command": "sg evaluate --transcript-path $TRANSCRIPT_PATH"
      }
    ],
    "PreToolUse": [
      {
        "matcher": "Edit|Write|Bash|Task|NotebookEdit",
        "type": "command",
        "command": "sg check --tool-name $TOOL_NAME"
      }
    ],
    "PreCompact": [
      {
        "type": "command",
        "command": "sg precompact --transcript-path $TRANSCRIPT_PATH"
      }
    ],
    "SessionStart": [
      {
        "type": "command",
        "command": "sg context-inject"
      }
    ]
  }
}
```

## LLM Evaluation

Superego maintains its own Claude session per project. When `sg evaluate` runs:

1. Load superego prompt from `.superego/prompt.md`
2. Read recent transcript (what main Claude is doing)
3. Read decision journal (project context)
4. Call: `claude --print --continue .superego/session "Evaluate: <context>"`
5. Parse structured JSON response
6. Update `state.json`

### Superego Prompt

```markdown
You are Superego, a metacognitive advisor monitoring a Claude Code session.

Analyze the conversation and determine the current phase:

**EXPLORING**: Still gathering context, reading files, no clear goal yet
**DISCUSSING**: Clarifying approach, unresolved questions exist, planning
**READY**: User confirmed approach, clear scope, ok to implement

Phase signals:
- User questions without answers → DISCUSSING
- "Let's do X" / "Go ahead" / "Sounds good" → READY
- "Wait" / "Actually..." / "What about..." → DISCUSSING
- New unknowns mid-implementation → Back to DISCUSSING

Respond with JSON only:
{
  "phase": "exploring|discussing|ready",
  "confidence": 0.0-1.0,
  "approved_scope": "description if ready, null otherwise",
  "reason": "brief explanation"
}
```

## Claude Behavioral Contract

When Claude is blocked by superego:

1. **Surface the conflict** - explain what superego said
2. **Present options** - override (user approves) or adapt (change approach)
3. **Respect the decision** - call `sg override` or `sg acknowledge`

Example blocked response:
```
Superego blocked this action: "Phase is DISCUSSING - there are
unresolved questions about the authentication approach."

Options:
1. **Override** - Proceed anyway (I'll use JWT as default)
2. **Discuss first** - Let's clarify the auth approach

Which would you prefer?
```

## Failure Modes

| Scenario | Behavior |
|----------|----------|
| `sg evaluate` fails/times out | Log warning, assume EXPLORING (safe default) |
| `sg check` can't read state.json | Block with "superego state unavailable" |
| Superego session corrupted | `sg reset` to clear and restart |
| Transcript path doesn't exist | Skip superego entirely (no transcript = no gating) |
| Hook timeout (60s default) | Future enhancement: consider async evaluation |

## Edge Cases

| Scenario | Behavior |
|----------|----------|
| First message is ambiguous ("fix the bug") | Superego indicates DISCUSSING, suggests Claude clarify scope |
| User sends short confirmation ("ok") | Superego reads full transcript to understand context of "ok" |
| Rapid successive messages | Possible race on state.json - noted as potential issue |
| Context compaction | PreCompact hook snapshots current understanding to decisions journal |

## Audit Trail

Superego maintains detailed logs for debugging:

```
.superego/
├── audit.log              ← Timestamped log of all sg commands
└── ...
```

Each entry includes: timestamp, command, inputs, decision, reason.

## Escape Hatch

```bash
# Disable for current session
export SUPEREGO_DISABLED=1

# Or via sg command
sg disable

# Re-enable
sg enable
```

## Recursion Prevention

Superego's own Claude session must not trigger hooks. Achieved by:
- Superego session stored in `.superego/session/` (different path pattern)
- Hooks check `$TRANSCRIPT_PATH` - skip if contains `.superego/`

## Tech Stack

- **Language**: Rust
- **CLI**: clap
- **Serialization**: serde + serde_json + serde_yaml
- **Claude invocation**: `std::process::Command` calling `claude` CLI
