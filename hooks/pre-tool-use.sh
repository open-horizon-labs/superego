#!/bin/bash
# PreToolUse hook for superego
# Evaluates large edits in context of the full session
#
# AIDEV-NOTE: Only triggers on Edit/Write over threshold. Passes the
# proposed change to sg evaluate-llm along with transcript context.

# Debug log function
log() {
    echo "[$(date '+%H:%M:%S')] [pre-tool] $1" >> .superego/hook.log 2>/dev/null
}

# Read hook input from stdin
INPUT=$(cat)

# Skip if superego is disabled
if [ "$SUPEREGO_DISABLED" = "1" ]; then
    exit 0
fi

# Check if superego is initialized
if [ ! -d ".superego" ]; then
    exit 0
fi

# Get tool info
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // ""')

# Only evaluate Edit and Write
if [ "$TOOL_NAME" != "Edit" ] && [ "$TOOL_NAME" != "Write" ]; then
    exit 0
fi

# Extract change details
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // ""')

if [ "$TOOL_NAME" = "Edit" ]; then
    OLD_STRING=$(echo "$INPUT" | jq -r '.tool_input.old_string // ""')
    NEW_STRING=$(echo "$INPUT" | jq -r '.tool_input.new_string // ""')
    OLD_LINES=$(echo "$OLD_STRING" | wc -l | tr -d ' ')
    NEW_LINES=$(echo "$NEW_STRING" | wc -l | tr -d ' ')
    CHANGE_SIZE=$((NEW_LINES > OLD_LINES ? NEW_LINES : OLD_LINES))

    CHANGE_CONTEXT="PROPOSED EDIT to $FILE_PATH:
--- OLD ($OLD_LINES lines) ---
$OLD_STRING
--- NEW ($NEW_LINES lines) ---
$NEW_STRING"
else
    CONTENT=$(echo "$INPUT" | jq -r '.tool_input.content // ""')
    CHANGE_SIZE=$(echo "$CONTENT" | wc -l | tr -d ' ')

    # Truncate large writes
    if [ "$CHANGE_SIZE" -gt 100 ]; then
        CONTENT_PREVIEW=$(echo "$CONTENT" | head -100)
        CHANGE_CONTEXT="PROPOSED WRITE to $FILE_PATH ($CHANGE_SIZE lines, first 100 shown):
$CONTENT_PREVIEW
..."
    else
        CHANGE_CONTEXT="PROPOSED WRITE to $FILE_PATH:
$CONTENT"
    fi
fi

log "Tool: $TOOL_NAME, File: $FILE_PATH, Size: $CHANGE_SIZE lines"

# Only evaluate changes over threshold
THRESHOLD=${SUPEREGO_CHANGE_THRESHOLD:-20}
if [ "$CHANGE_SIZE" -lt "$THRESHOLD" ]; then
    log "Small change ($CHANGE_SIZE < $THRESHOLD), allowing"
    exit 0
fi

log "Large change ($CHANGE_SIZE >= $THRESHOLD), evaluating in context..."

# Get transcript path
TRANSCRIPT_PATH=$(echo "$INPUT" | jq -r '.transcript_path // .transcriptPath // ""')
if [ -z "$TRANSCRIPT_PATH" ] || [ "$TRANSCRIPT_PATH" = "null" ]; then
    log "No transcript path, allowing"
    exit 0
fi

# Write pending change context to file for sg evaluate-llm to include
echo "$CHANGE_CONTEXT" > .superego/pending_change.txt

# Run evaluation with transcript context
log "Running: sg evaluate-llm with pending change"
RESULT=$(sg evaluate-llm --transcript-path "$TRANSCRIPT_PATH" 2>> .superego/hook.log)
EXIT_CODE=$?

rm -f .superego/pending_change.txt

if [ $EXIT_CODE -ne 0 ]; then
    log "ERROR: sg evaluate-llm failed with code $EXIT_CODE"
    exit 0
fi

log "Evaluation complete"

# Check if there's feedback
if [ -s ".superego/feedback" ]; then
    FEEDBACK=$(cat .superego/feedback)
    log "Blocking with feedback: ${FEEDBACK:0:100}..."

    rm -f .superego/feedback

    REASON="SUPEREGO FEEDBACK on proposed $TOOL_NAME to $FILE_PATH:

$FEEDBACK

Please reconsider the change or explain why it's appropriate."

    OUTPUT=$(jq -n --arg reason "$REASON" '{"decision":"block","reason":$reason,"suppressOutput":true}')
    log "Outputting: $OUTPUT"
    echo "$OUTPUT"
else
    log "No concerns, allowing"
fi

exit 0
