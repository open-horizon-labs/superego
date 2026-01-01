#!/bin/bash
# PreToolUse hook for superego
#
# TRIGGERS EVALUATION ON:
# 1. LARGE EDIT/WRITE - Edit/Write >= threshold lines
# 2. INTERVAL - Any tool, if eval_interval_minutes passed (sg should-eval)
#
# Uses PreToolUse as a convenient tick for periodic drift detection.
#
# NOTE: No `set -e` because sg should-eval uses exit codes (0=yes, 1=no)

# Check for sg binary
if ! command -v sg &> /dev/null; then
    echo "sg binary not found. Install: cargo install superego" >&2
    exit 0
fi

# Use CLAUDE_PROJECT_DIR if available, otherwise current directory
PROJECT_DIR="${CLAUDE_PROJECT_DIR:-.}"

# Log function
log() {
    echo "[$(date '+%H:%M:%S')] [pre-tool] $1" >> "$PROJECT_DIR/.superego/hook.log" 2>/dev/null
}

# Read hook input from stdin
INPUT=$(cat)

# Skip if superego is disabled
if [ "$SUPEREGO_DISABLED" = "1" ]; then
    exit 0
fi

# Check if superego is initialized
if [ ! -d "$PROJECT_DIR/.superego" ]; then
    exit 0
fi

# Extract common fields
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // ""')
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // ""')
TRANSCRIPT_PATH=$(echo "$INPUT" | jq -r '.transcript_path // .transcriptPath // ""')

# Build session-namespaced paths
if [ -n "$SESSION_ID" ] && [ "$SESSION_ID" != "null" ]; then
    SESSION_DIR="$PROJECT_DIR/.superego/sessions/$SESSION_ID"
    mkdir -p "$SESSION_DIR"
else
    SESSION_DIR="$PROJECT_DIR/.superego"
    SESSION_ID=""
fi
FEEDBACK_PATH="$SESSION_DIR/feedback"
PENDING_CHANGE_PATH="$SESSION_DIR/pending_change.txt"
LOCK_FILE="$SESSION_DIR/eval.lock"

# Skip if no transcript
if [ -z "$TRANSCRIPT_PATH" ] || [ "$TRANSCRIPT_PATH" = "null" ]; then
    exit 0
fi

# ===========================================================================
# HELPER: Run evaluation and handle feedback
# ===========================================================================
run_eval() {
    local trigger_reason="$1"

    # Atomic lock to prevent duplicate evaluations
    if ! mkdir "$LOCK_FILE" 2>/dev/null; then
        log "Eval already in progress, skipping"
        exit 0
    fi
    trap 'rmdir "$LOCK_FILE" 2>/dev/null' EXIT

    log "Running eval (trigger: $trigger_reason)"
    if [ -n "$SESSION_ID" ]; then
        sg evaluate-llm --transcript-path "$TRANSCRIPT_PATH" --session-id "$SESSION_ID" >> "$PROJECT_DIR/.superego/hook.log" 2>&1
    else
        sg evaluate-llm --transcript-path "$TRANSCRIPT_PATH" >> "$PROJECT_DIR/.superego/hook.log" 2>&1
    fi
    local exit_code=$?
    rmdir "$LOCK_FILE" 2>/dev/null
    trap - EXIT

    # Cleanup pending change
    rm -f "$PENDING_CHANGE_PATH"

    if [ $exit_code -ne 0 ]; then
        log "ERROR: sg evaluate-llm failed with code $exit_code"
        exit 0
    fi

    log "Evaluation complete"

    # Trigger wm extraction in background (graceful if wm not installed)
    # AIDEV-NOTE: wm extract captures tacit knowledge from the transcript
    if command -v wm >/dev/null 2>&1; then
        log "Triggering wm extract in background"
        wm extract --transcript "$TRANSCRIPT_PATH" --background 2>/dev/null &
    fi

    # Check for feedback (atomic move)
    if [ -s "$FEEDBACK_PATH" ]; then
        local temp_feedback="$FEEDBACK_PATH.$$"
        if mv "$FEEDBACK_PATH" "$temp_feedback" 2>/dev/null; then
            local feedback
            feedback=$(cat "$temp_feedback")
            rm -f "$temp_feedback"
            log "Blocking with feedback: ${feedback:0:100}..."

            local reason="SUPEREGO FEEDBACK ($trigger_reason):

$feedback

Please reconsider or explain why it's appropriate."

            jq -n --arg reason "$reason" '{"decision":"block","reason":$reason}'
            exit 1
        fi
    fi

    # No concerns - allow
    exit 0
}

# ===========================================================================
# HELPER: Build pending change context for Edit/Write
# ===========================================================================
build_pending_change() {
    local file_path
    file_path=$(echo "$INPUT" | jq -r '.tool_input.file_path // ""')

    if [ "$TOOL_NAME" = "Edit" ]; then
        local old_string new_string old_lines new_lines
        old_string=$(echo "$INPUT" | jq -r '.tool_input.old_string // ""')
        new_string=$(echo "$INPUT" | jq -r '.tool_input.new_string // ""')
        old_lines=$(echo "$old_string" | wc -l | tr -d ' ')
        new_lines=$(echo "$new_string" | wc -l | tr -d ' ')

        echo "PROPOSED EDIT to $file_path:
--- OLD ($old_lines lines) ---
$old_string
--- NEW ($new_lines lines) ---
$new_string"
    elif [ "$TOOL_NAME" = "Write" ]; then
        local content content_lines
        content=$(echo "$INPUT" | jq -r '.tool_input.content // ""')
        content_lines=$(echo "$content" | wc -l | tr -d ' ')

        if [ "$content_lines" -gt 100 ]; then
            local preview
            preview=$(echo "$content" | head -100)
            echo "PROPOSED WRITE to $file_path ($content_lines lines, first 100 shown):
$preview
..."
        else
            echo "PROPOSED WRITE to $file_path:
$content"
        fi
    fi
}

# ===========================================================================
# TRIGGER 1: LARGE EDIT/WRITE (size >= threshold)
# ===========================================================================
if [ "$TOOL_NAME" = "Edit" ] || [ "$TOOL_NAME" = "Write" ]; then
    # Calculate change size
    if [ "$TOOL_NAME" = "Edit" ]; then
        OLD_LINES=$(echo "$INPUT" | jq -r '.tool_input.old_string // ""' | wc -l | tr -d ' ')
        NEW_LINES=$(echo "$INPUT" | jq -r '.tool_input.new_string // ""' | wc -l | tr -d ' ')
        CHANGE_SIZE=$((NEW_LINES > OLD_LINES ? NEW_LINES : OLD_LINES))
    else
        CHANGE_SIZE=$(echo "$INPUT" | jq -r '.tool_input.content // ""' | wc -l | tr -d ' ')
    fi

    THRESHOLD=${SUPEREGO_CHANGE_THRESHOLD:-20}

    if [ "$CHANGE_SIZE" -ge "$THRESHOLD" ]; then
        log "Large $TOOL_NAME ($CHANGE_SIZE >= $THRESHOLD lines)"
        build_pending_change > "$PENDING_CHANGE_PATH"
        run_eval "large $TOOL_NAME"
        # run_eval exits, won't reach here
    fi
fi

# ===========================================================================
# TRIGGER 2: INTERVAL CHECK (any tool, periodic drift detection)
# ===========================================================================
# sg should-eval exits 0 if eval needed, 1 if not
INTERVAL_TRIGGERED=false
if [ -n "$SESSION_ID" ]; then
    sg should-eval --session-id "$SESSION_ID" >/dev/null 2>&1 && INTERVAL_TRIGGERED=true
else
    sg should-eval >/dev/null 2>&1 && INTERVAL_TRIGGERED=true
fi

if [ "$INTERVAL_TRIGGERED" = true ]; then
    log "Interval eval triggered on $TOOL_NAME"

    # Include pending change context if this is an Edit/Write
    if [ "$TOOL_NAME" = "Edit" ] || [ "$TOOL_NAME" = "Write" ]; then
        build_pending_change > "$PENDING_CHANGE_PATH"
    fi

    run_eval "interval on $TOOL_NAME"
    # run_eval exits, won't reach here
fi

# ===========================================================================
# NEITHER TRIGGERED - ALLOW
# ===========================================================================
exit 0
