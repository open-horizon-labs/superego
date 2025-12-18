#!/bin/bash
# Release script for sg (superego)
#
# Usage: ./scripts/release.sh [version]
# Examples:
#   ./scripts/release.sh        # Auto-increment patch (0.3.1 → 0.3.2)
#   ./scripts/release.sh 0.4.0  # Explicit version
#
# This script:
# 1. Updates version in Cargo.toml
# 2. Runs tests
# 3. Commits the version bump
# 4. Creates and pushes a git tag
# 5. Waits for the tarball to be available
# 6. Updates the Homebrew formula with new sha256
# 7. Publishes to crates.io
# 8. Commits the formula update
# 9. Updates the tap repository

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log() { echo -e "${GREEN}[release]${NC} $1"; }
warn() { echo -e "${YELLOW}[release]${NC} $1"; }
error() { echo -e "${RED}[release]${NC} $1" >&2; exit 1; }

# Get version - either from argument or auto-increment patch
if [ -z "$1" ]; then
    # Auto-increment patch version
    CURRENT=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
    if [ -z "$CURRENT" ]; then
        error "Could not read current version from Cargo.toml"
    fi
    MAJOR=$(echo "$CURRENT" | cut -d. -f1)
    MINOR=$(echo "$CURRENT" | cut -d. -f2)
    PATCH=$(echo "$CURRENT" | cut -d. -f3)
    VERSION="$MAJOR.$MINOR.$((PATCH + 1))"
    log "No version specified, auto-incrementing: $CURRENT → $VERSION"
else
    VERSION="$1"
    # Validate version format (semver)
    if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
        error "Invalid version format. Use semver (e.g., 0.2.0)"
    fi
fi

TAG="v$VERSION"

# Check we're in repo root
if [ ! -f "Cargo.toml" ]; then
    error "Must run from repository root (Cargo.toml not found)"
fi

# Check for clean working directory
if [ -n "$(git status --porcelain)" ]; then
    error "Working directory not clean. Commit or stash changes first."
fi

# Check we're on main branch
BRANCH=$(git branch --show-current)
if [ "$BRANCH" != "main" ]; then
    warn "Not on main branch (on: $BRANCH). Continue? [y/N]"
    read -r response
    if [ "$response" != "y" ]; then
        exit 1
    fi
fi

# Check tag doesn't already exist
if git rev-parse "$TAG" >/dev/null 2>&1; then
    error "Tag $TAG already exists"
fi

# Get repo URL from Cargo.toml
REPO_URL=$(grep '^repository' Cargo.toml | sed 's/.*= "//' | sed 's/"//')
if [ -z "$REPO_URL" ]; then
    error "Could not find repository URL in Cargo.toml"
fi

log "Releasing $TAG"
log "Repository: $REPO_URL"

# Step 1: Update Cargo.toml version
log "Updating Cargo.toml version to $VERSION..."
sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml

# Verify the change
if ! grep -q "version = \"$VERSION\"" Cargo.toml; then
    error "Failed to update version in Cargo.toml"
fi

# Update plugin version
PLUGIN_JSON="plugin/.claude-plugin/plugin.json"
log "Updating plugin version in $PLUGIN_JSON..."
sed -i '' "s/\"version\": \"[^\"]*\"/\"version\": \"$VERSION\"/" "$PLUGIN_JSON"

if ! grep -q "\"version\": \"$VERSION\"" "$PLUGIN_JSON"; then
    error "Failed to update version in $PLUGIN_JSON"
fi

# Update marketplace plugin version (requires jq)
MARKETPLACE_JSON=".claude-plugin/marketplace.json"
log "Updating marketplace plugin version in $MARKETPLACE_JSON..."
if ! command -v jq &> /dev/null; then
    error "jq is required for marketplace.json update. Install with: brew install jq"
fi
jq --arg v "$VERSION" '.plugins[0].version = $v' "$MARKETPLACE_JSON" > "$MARKETPLACE_JSON.tmp" && mv "$MARKETPLACE_JSON.tmp" "$MARKETPLACE_JSON"

if ! jq -e --arg v "$VERSION" '.plugins[0].version == $v' "$MARKETPLACE_JSON" > /dev/null; then
    error "Failed to update plugin version in $MARKETPLACE_JSON"
fi

# Step 2: Run tests
log "Running tests..."
cargo test || error "Tests failed"

# Step 3: Build release
log "Building release binary..."
cargo build --release || error "Build failed"

# Step 4: Commit version bump (if needed)
log "Committing version bump..."
git add Cargo.toml "$PLUGIN_JSON" "$MARKETPLACE_JSON"
if git diff --cached --quiet; then
    log "Version already at $VERSION, skipping commit"
else
    git commit -m "Bump version to $VERSION"
fi

# Step 5: Create and push tag
log "Creating tag $TAG..."
git tag "$TAG"

log "Pushing to origin..."
git push origin "$BRANCH"
git push origin "$TAG"

# Step 6: Wait for tarball and get sha256
TARBALL_URL="$REPO_URL/archive/refs/tags/$TAG.tar.gz"
log "Waiting for tarball at: $TARBALL_URL"

# Wait up to 30 seconds for GitHub to make the tarball available
for i in {1..6}; do
    if curl -sfL "$TARBALL_URL" -o /tmp/sg-release.tar.gz 2>/dev/null; then
        break
    fi
    if [ $i -eq 6 ]; then
        error "Tarball not available after 30 seconds. Check GitHub."
    fi
    log "Waiting for tarball... (attempt $i/6)"
    sleep 5
done

SHA256=$(shasum -a 256 /tmp/sg-release.tar.gz | awk '{print $1}')
rm /tmp/sg-release.tar.gz
log "SHA256: $SHA256"

# Step 7: Update Homebrew formula
log "Updating Homebrew formula..."
FORMULA="Formula/superego.rb"

if [ ! -f "$FORMULA" ]; then
    error "Formula not found at $FORMULA"
fi

# Update URL version
sed -i '' "s|/v[0-9]*\.[0-9]*\.[0-9]*\.tar\.gz|/$TAG.tar.gz|" "$FORMULA"

# Update sha256
sed -i '' "s/sha256 \"[a-f0-9]*\"/sha256 \"$SHA256\"/" "$FORMULA"

# Verify changes
if ! grep -q "$TAG" "$FORMULA"; then
    error "Failed to update version in formula"
fi
if ! grep -q "$SHA256" "$FORMULA"; then
    error "Failed to update sha256 in formula"
fi

# Step 8: Publish to crates.io
log "Publishing to crates.io..."
cargo publish --allow-dirty || error "Failed to publish to crates.io"

# Step 9: Commit formula update
log "Committing formula update..."
git add "$FORMULA" Cargo.lock
git commit -m "Update Homebrew formula for $TAG"
git push origin "$BRANCH"

# Step 10: Update the tap repository
TAP_REPO="https://github.com/cloud-atlas-ai/homebrew-superego"
TAP_DIR=$(mktemp -d)

log "Cloning tap repository..."
git clone "$TAP_REPO" "$TAP_DIR" || error "Failed to clone tap repository"

log "Updating tap formula..."
cp "$FORMULA" "$TAP_DIR/Formula/superego.rb"

cd "$TAP_DIR"
git add Formula/superego.rb
git commit -m "Update sg to $TAG"
git push origin main || git push origin master

cd - > /dev/null
rm -rf "$TAP_DIR"

log ""
log "✓ Release $TAG complete!"
log ""
log "Next steps:"
log "  1. Create GitHub release at: $REPO_URL/releases/new?tag=$TAG"
log ""
log "Users can now install with:"
log "  cargo install superego"
log "  # or"
log "  brew tap cloud-atlas-ai/superego"
log "  brew install superego"
