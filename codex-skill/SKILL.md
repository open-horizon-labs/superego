---
name: superego
description: Metacognitive oversight. Invoke with "$superego" to evaluate, "$superego init" to set up, "$superego update" to update, "$superego remove" to uninstall.
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

## $superego update

Download and install the latest superego skill files.

**Run:**
```bash
SKILL_DIR="$HOME/.codex/skills/superego"

# Backup current skill
if [ -f "$SKILL_DIR/SKILL.md" ]; then
  cp "$SKILL_DIR/SKILL.md" "$SKILL_DIR/SKILL.md.bak"
fi

# Download latest files
echo "Downloading latest skill files..."
for file in SKILL.md agents/code.md agents/writing.md agents/learning.md; do
  mkdir -p "$(dirname "$SKILL_DIR/$file")"
  curl -fsSL -o "$SKILL_DIR/$file" \
    "https://raw.githubusercontent.com/cloud-atlas-ai/superego/main/codex-skill/$file"
done

# Update binary if installed
if command -v sg >/dev/null; then
  echo "Updating superego binary..."
  if command -v brew >/dev/null; then
    brew upgrade superego 2>/dev/null || true
  elif command -v cargo >/dev/null; then
    cargo install superego --force
  fi
  echo "Binary version: $(sg --version)"
fi

echo "✓ Skill files updated. Restart Codex to apply changes."
```

**Tell user:** "Updated superego skill to latest version. Restart Codex to reload."

**If errors occur:** Tell user to check their internet connection or try again later.

## $superego remove

Remove superego from this project. Run these steps:

**Step 1:** Remove .superego/ directory:
```bash
rm -rf .superego/
```

**Step 2:** Remove the "Superego Metacognitive Oversight" section from AGENTS.md (if present).

**Step 3:** Confirm to user: "Superego removed from this project."
