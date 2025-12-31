# Superego Prompt Management

Manage superego prompts for this project. Available actions:

## Usage

- `/superego-prompt` or `/superego-prompt list` — List available prompts
- `/superego-prompt switch <name>` — Switch to a different prompt (code, writing, learning)
- `/superego-prompt show` — Show current prompt info

## Actions

### List (default)

Run `sg prompt list` to show available prompts with the current one marked.

### Switch

When the user specifies a prompt name (e.g., `/superego-prompt switch writing`):

1. Run `sg prompt switch <name>` to switch prompts
2. Report the result (backed up customizations, restored from backup, or fresh install)
3. Remind user that the new prompt will take effect on next evaluation

### Show

Run `sg prompt show` to display:
- Current prompt type and description
- Whether it has local modifications
- Available backups from other prompt types

## Notes

- The `code` prompt is for coding/development work (default)
- The `writing` prompt is for content creation, writing, and editing
- The `learning` prompt is for reviewing teaching/tutoring approaches - ensures learning is hands-on and verifiable
- Customizations are preserved: switching backs up your changes and restores them when you switch back
- If `.superego/` doesn't exist, suggest running `/superego-init` first
