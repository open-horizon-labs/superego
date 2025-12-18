#!/bin/bash
# Superego evaluation hook
# Used by: Stop (after response), PreCompact (before context truncation)
#
# AIDEV-NOTE: For Stop hooks, if concerns found and not already blocked once,
# returns {"decision":"block","reason":"..."} so Claude sees feedback and continues.

# Check for sg binary
if ! command -v sg &> /dev/null; then
    echo "sg binary not found. Install: cargo install superego" >&2
    exit 0
fi

# Use CLAUDE_PROJECT_DIR if available, otherwise current directory
PROJECT_DIR="${CLAUDE_PROJECT_DIR:-.}"

# Debug log function
log() {
    echo "[$(date '+%H:%M:%S')] [evaluate] $1" >> "$PROJECT_DIR/.superego/hook.log" 2>/dev/null
}

# Read hook input from stdin
INPUT=$(cat)

# Skip if superego is disabled
if [ "$SUPEREGO_DISABLED" = "1" ]; then
    log "SKIP: SUPEREGO_DISABLED=1"
    exit 0
fi

# Check if superego is initialized
if [ ! -d "$PROJECT_DIR/.superego" ]; then
    exit 0  # No log - .superego doesn't exist
fi

log "Hook fired"

# Check if stop hook already active (prevent infinite loop)
STOP_HOOK_ACTIVE=$(echo "$INPUT" | jq -r '.stop_hook_active // false')
if [ "$STOP_HOOK_ACTIVE" = "true" ]; then
    log "SKIP: stop_hook_active=true (already blocked once)"
    exit 0
fi

# Extract transcript path and session ID from hook input
TRANSCRIPT_PATH=$(echo "$INPUT" | jq -r '.transcript_path // .transcriptPath // ""')
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // ""')

# Skip if no transcript path
if [ -z "$TRANSCRIPT_PATH" ] || [ "$TRANSCRIPT_PATH" = "null" ]; then
    log "SKIP: No transcript path"
    exit 0
fi

# Build session-namespaced paths
if [ -n "$SESSION_ID" ] && [ "$SESSION_ID" != "null" ]; then
    SESSION_DIR="$PROJECT_DIR/.superego/sessions/$SESSION_ID"
    mkdir -p "$SESSION_DIR"
    FEEDBACK_PATH="$SESSION_DIR/feedback"
else
    FEEDBACK_PATH="$PROJECT_DIR/.superego/feedback"
fi

# Skip if this is superego's own transcript (recursion prevention)
if [[ "$TRANSCRIPT_PATH" == *"/.superego/"* ]] || [[ "$TRANSCRIPT_PATH" == ".superego/"* ]]; then
    log "SKIP: Superego transcript (recursion prevention)"
    exit 0
fi

# Run LLM evaluation (redirect all output to log)
# Atomic lock to prevent duplicate evaluations
if [ -n "$SESSION_ID" ] && [ "$SESSION_ID" != "null" ]; then
    LOCK_FILE="$SESSION_DIR/eval.lock"
else
    LOCK_FILE="$PROJECT_DIR/.superego/eval.lock"
fi

if mkdir "$LOCK_FILE" 2>/dev/null; then
    # Got lock, run evaluation
    trap 'rmdir "$LOCK_FILE" 2>/dev/null' EXIT
    if [ -n "$SESSION_ID" ] && [ "$SESSION_ID" != "null" ]; then
        log "Running: sg evaluate-llm --session-id $SESSION_ID"
        sg evaluate-llm --transcript-path "$TRANSCRIPT_PATH" --session-id "$SESSION_ID" >> "$PROJECT_DIR/.superego/hook.log" 2>&1
    else
        log "Running: sg evaluate-llm (no session_id)"
        sg evaluate-llm --transcript-path "$TRANSCRIPT_PATH" >> "$PROJECT_DIR/.superego/hook.log" 2>&1
    fi
    EXIT_CODE=$?
    rmdir "$LOCK_FILE" 2>/dev/null
else
    log "Eval already in progress, skipping"
    exit 0
fi

if [ $EXIT_CODE -ne 0 ]; then
    log "ERROR: sg evaluate-llm failed with code $EXIT_CODE"
    exit 0
fi

log "Evaluation complete"

# Check if there's feedback to deliver (file exists and non-empty)
# Use atomic move to prevent race conditions with concurrent hooks
if [ -s "$FEEDBACK_PATH" ]; then
    TEMP_FEEDBACK="$FEEDBACK_PATH.$$"
    if ! mv "$FEEDBACK_PATH" "$TEMP_FEEDBACK" 2>/dev/null; then
        log "Feedback already claimed by another hook"
        exit 0
    fi
    FEEDBACK=$(cat "$TEMP_FEEDBACK")
    log "Blocking with feedback: ${FEEDBACK:0:100}..."
    rm -f "$TEMP_FEEDBACK"

    # Build properly escaped JSON using jq
    REASON="SUPEREGO FEEDBACK: Please critically evaluate this feedback. If you agree, incorporate it. If you disagree on non-trivial points, escalate to the user.

$FEEDBACK"

    # Output block decision - Claude will see the reason and continue
    OUTPUT=$(jq -n --arg reason "$REASON" '{"decision":"block","reason":$reason}')
    log "Outputting: $OUTPUT"
    echo "$OUTPUT"
    exit 1
fi

# No concerns, allow
exit 0
