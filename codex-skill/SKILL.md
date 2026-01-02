---
name: superego
description: Metacognitive oversight. Invoke with "$superego" to evaluate, "$superego init" to set up, "$superego remove" to uninstall.
---

# Superego - Metacognitive Oversight

## $superego (evaluate)

Evaluate the current conversation for alignment, focus, and proportionality.

**When to use:**
- Before committing to a plan or approach
- When choosing between alternatives
- Before non-trivial implementations
- When the task feels complex or uncertain
- Before claiming work is "done"

**Run:**
```bash
sg evaluate-codex
```

**Handle results:**
- `has_concerns: true` → STOP, show feedback to user verbatim, ask before proceeding
- `has_concerns: false` → Tell user "Superego: No concerns", continue
- `skipped: true` → Proceed normally (not an error)

## $superego init

Set up superego for this project. Run these steps:

**Step 1:** Install sg binary if not present:
```bash
command -v sg || cargo install superego
```

**Step 2:** Initialize .superego/ directory:
```bash
sg init
```

**Step 3:** Add this section to the project's AGENTS.md (create the file if needed):

```markdown
## Superego Metacognitive Oversight

This project uses superego. You have the `$superego` skill available.

**Use $superego:**
- Before committing to a plan or approach
- When choosing between alternatives
- Before non-trivial implementations
- When the task feels complex or uncertain
- Before claiming work is "done"

Superego catches strategic mistakes. Use it at **decision points**.

**Results:** `has_concerns: true` = STOP and show user; `skipped: true` = proceed normally.
```

**Step 4:** Confirm to user: "Superego initialized. I'll use $superego at decision points."

## $superego prompt list

List available evaluation prompts and show which is currently active.

**Run:**
```bash
sg prompt list
```

**Output shows:**
- `code` - Metacognitive advisor for software development (default)
- `writing` - Co-author reviewer for content creation
- `learning` - Learning coach for teaching approaches
- The active prompt is marked with `*`

## $superego prompt switch <name>

Switch to a different evaluation prompt type.

**Run:**
```bash
sg prompt switch <name>  # name = code, writing, or learning
```

**Examples:**
```bash
sg prompt switch writing   # Use writing prompt for blog posts/docs
sg prompt switch learning  # Use learning prompt for tutorials
sg prompt switch code      # Back to code prompt
```

**Behavior:**
- Backs up customizations before switching (saved to `.superego/prompt.<type>.md.bak`)
- Restores previous customizations if you've used this prompt before
- Updates `.superego/config.yaml` with new base_prompt

**Tell user:** "Switched to [name] prompt. Superego will now evaluate using [description]."

## $superego prompt show

Show current prompt info and available backups.

**Run:**
```bash
sg prompt show
```

**Output shows:**
- Current base prompt
- Whether you have local modifications
- Which backups are available

## $superego review [target]

Get on-demand review of changes. Advisory feedback (non-blocking).

**Run:**
```bash
sg review            # Review staged changes (git diff --cached)
sg review staged     # Same as above
sg review pr         # Review PR diff vs base branch
sg review <file>     # Review specific file
```

**Examples:**
```bash
# Before committing
git add .
sg review

# Before creating PR
sg review pr

# Review specific file
sg review src/main.rs
```

**Uses:** Current active prompt (code/writing/learning)

**Tell user:** Show the review feedback and explain it uses the current prompt type.

## $superego remove

Remove superego from this project. Run these steps:

**Step 1:** Remove .superego/ directory:
```bash
rm -rf .superego/
```

**Step 2:** Remove the "Superego Metacognitive Oversight" section from AGENTS.md (if present).

**Step 3:** Confirm to user: "Superego removed from this project."
