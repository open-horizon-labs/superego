#!/bin/bash
# Superego evaluation hook
# Used by: Stop (after response), PreCompact (before context truncation)
#
# AIDEV-NOTE: For Stop hooks, if concerns found and not already blocked once,
# returns {"decision":"block","reason":"..."} so Claude sees feedback and continues.

# Debug log function
log() {
    echo "[$(date '+%H:%M:%S')] $1" >> .superego/hook.log 2>/dev/null
}

# Read hook input from stdin
INPUT=$(cat)

# Skip if superego is disabled
if [ "$SUPEREGO_DISABLED" = "1" ]; then
    log "SKIP: SUPEREGO_DISABLED=1"
    exit 0
fi

# Check if superego is initialized
if [ ! -d ".superego" ]; then
    exit 0  # No log - .superego doesn't exist
fi

log "Hook fired"

# Check if stop hook already active (prevent infinite loop)
STOP_HOOK_ACTIVE=$(echo "$INPUT" | jq -r '.stop_hook_active // false')
if [ "$STOP_HOOK_ACTIVE" = "true" ]; then
    log "SKIP: stop_hook_active=true (already blocked once)"
    exit 0
fi

# Extract transcript path from hook input
TRANSCRIPT_PATH=$(echo "$INPUT" | jq -r '.transcript_path // .transcriptPath // ""')

# Skip if no transcript path
if [ -z "$TRANSCRIPT_PATH" ] || [ "$TRANSCRIPT_PATH" = "null" ]; then
    log "SKIP: No transcript path"
    exit 0
fi

# Skip if this is superego's own transcript (recursion prevention)
if [[ "$TRANSCRIPT_PATH" == *".superego"* ]]; then
    log "SKIP: Superego transcript (recursion prevention)"
    exit 0
fi

# Run LLM evaluation and capture output
log "Running: sg evaluate-llm"
RESULT=$(sg evaluate-llm --transcript-path "$TRANSCRIPT_PATH" 2>> .superego/hook.log)
EXIT_CODE=$?

if [ $EXIT_CODE -ne 0 ]; then
    log "ERROR: sg evaluate-llm failed with code $EXIT_CODE"
    exit 0
fi

log "Evaluation complete"

# Check if there's feedback to deliver (file exists and non-empty)
if [ -s ".superego/feedback" ]; then
    FEEDBACK=$(cat .superego/feedback)
    log "Blocking with feedback: ${FEEDBACK:0:100}..."

    # Clear feedback file since we're delivering it now
    rm -f .superego/feedback

    # Build properly escaped JSON using jq
    REASON="SUPEREGO FEEDBACK: Please critically evaluate this feedback. If you agree, incorporate it. If you disagree on non-trivial points, escalate to the user.

$FEEDBACK"

    # Output block decision - Claude will see the reason and continue
    # suppressOutput hides from user display, reason still goes to Claude
    OUTPUT=$(jq -n --arg reason "$REASON" '{"decision":"block","reason":$reason,"suppressOutput":true}')
    log "Outputting: $OUTPUT"
    echo "$OUTPUT"
else
    log "No concerns, allowing stop"
fi

exit 0
