#!/bin/bash
# SessionStart hook for superego
# Establishes the superego contract at the beginning of each session
#
# AIDEV-NOTE: Uses additionalContext to inject contract into Claude's context

# Skip if superego is disabled
if [ "$SUPEREGO_DISABLED" = "1" ]; then
    exit 0
fi

# Check if superego is initialized
if [ ! -d ".superego" ]; then
    exit 0
fi

# Auto-update hooks if outdated (silent)
sg check >/dev/null 2>&1 || true

# Log
echo "[$(date '+%H:%M:%S')] [session] Session started" >> .superego/hook.log 2>/dev/null

# Output JSON with additionalContext
cat << 'EOF'
{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": "SUPEREGO ACTIVE: This project uses superego, a metacognitive advisor that monitors your work. When you receive SUPEREGO FEEDBACK, critically evaluate it: if you agree, incorporate it into your approach; if you disagree on non-trivial feedback, escalate to the user explaining both perspectives. Superego feedback reflects concerns about your reasoning, approach, or alignment with the user's goals - it deserves serious consideration, not just acknowledgment."
  }
}
EOF
