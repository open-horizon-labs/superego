# Initialize Superego

Initialize superego for this project.

## Step 1: Check current state
- Check if `.superego/` already exists - if so, tell user it's already initialized and show status
- Check if `sg` binary is available (`command -v sg`) - if yes, skip to Step 3

## Step 2: Install sg binary

**Detect available package managers:**
- Homebrew: `command -v brew`
- Cargo: `command -v cargo` OR `test -f ~/.cargo/bin/cargo`

**Offer installation based on what's available:**

If **Homebrew** available (preferred for macOS):
```bash
brew install cloud-atlas-ai/superego/superego
```

If **Cargo** available:
```bash
cargo install superego
# or if cargo not in PATH:
~/.cargo/bin/cargo install superego
```

If **neither available**, offer to install a package manager:
- **Install Homebrew** (recommended for macOS):
  ```bash
  /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
  ```
  Then: `brew install cloud-atlas-ai/superego/superego`

- **Install Rust** (cross-platform):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
  Then restart shell and: `cargo install superego`

**For local development** (if user specifies a path):
```bash
cargo install --path /path/to/superego
# or: ~/.cargo/bin/cargo install --path /path/to/superego
```

## Step 3: Initialize project
After `sg` binary is available, run:
```bash
sg init
```

## Step 4: Confirm
Tell user superego is now initialized and active for this project. It will monitor your work and provide feedback when needed (before large changes, at natural stopping points, etc.).

---
Be concise. Detect what's available, offer appropriate options, guide user through setup.
