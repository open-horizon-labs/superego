#!/bin/bash
# Superego evaluation hook
# Used by: Stop (after response), PreCompact (before context truncation)
#
# AIDEV-NOTE: Single script for all evaluation triggers. Evaluates
# everything since last_evaluated timestamp.

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

# Run LLM evaluation
log "Running: sg evaluate-llm"
sg evaluate-llm --transcript-path "$TRANSCRIPT_PATH" >> .superego/hook.log 2>&1 || log "ERROR: sg evaluate-llm failed"
log "Done"

# Always exit 0 - evaluation shouldn't block Claude from stopping
exit 0
