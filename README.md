# Superego

Metacognitive advisor for Claude Code. Evaluates conversations and provides feedback to keep Claude on track.

## Installation

### From source
```bash
cargo build --release
sudo cp target/release/sg /usr/local/bin/
```

### Via cargo (when published)
```bash
cargo install sg
```

### Via Homebrew (when tap created)
```bash
brew tap OWNER/higher-peak
brew install sg
```

## Usage

Initialize in your project:
```bash
cd /path/to/project
sg init
```

This creates `.superego/` with the system prompt and configures Claude Code hooks in `.claude/settings.json`.

Superego will automatically:
1. Evaluate conversations after each Claude response
2. Write feedback to a queue if concerns are found
3. Inject feedback as context on your next prompt

## Customization

Edit `.superego/prompt.md` to customize the evaluation criteria.

## Commands

```bash
sg init              # Initialize superego in current project
sg evaluate-llm      # Run LLM evaluation (called by hooks)
sg has-feedback      # Check for pending feedback
sg get-feedback      # Get and clear pending feedback
sg history           # View decision history
```

## License

MIT
