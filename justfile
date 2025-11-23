# Format the codebase
fmt:
    cargo fmt --all

# Check formatting without modifying files
fmt-check:
    cargo fmt --all -- --check

# Run clippy lints
clippy:
    cargo clippy --all-targets --all-features -- -D warnings

# Build the project
build:
    cargo build --all-targets --all-features

# Run tests
test:
    cargo test --all-features

# Run all checks (formatting + clippy + build + tests)
check: fmt-check clippy build test

# Validate git conventions (commit messages and branch names)
validate:
    @echo "🔍 Validating git conventions..."
    @./scripts/validate-branch-name.sh
    @echo "✅ Git validation passed!"

# Format and run all checks including git validation
all: fmt check validate

# Install git hooks for commit message and branch name validation
install-hooks:
    @echo "Installing git hooks..."
    @mkdir -p .git/hooks
    @cp .githooks/commit-msg .git/hooks/commit-msg
    @cp .githooks/pre-push .git/hooks/pre-push
    @cp .githooks/pre-commit .git/hooks/pre-commit
    @chmod +x .git/hooks/commit-msg
    @chmod +x .git/hooks/pre-push
    @chmod +x .git/hooks/pre-commit
    @chmod +x scripts/*.sh
    @echo "✅ Git hooks installed successfully!"

# Default recipe (run when just called without arguments)
default: check

