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

# Build embive-program for riscv32imac-unknown-none-elf target
# Cargo automatically handles dependency tracking, so this will rebuild
# if embive-program or embive-runtime source files change
embive-program:
    cargo build --package embive-program --target riscv32imac-unknown-none-elf

# Inspect ELF binary layout (sections, addresses, sizes)
# Shows memory layout from linker script
elf-layout:
    @echo "📋 ELF Section Layout:"
    @rust-objdump -h target/riscv32imac-unknown-none-elf/debug/embive-program 2>/dev/null || \
     rust-objdump -h target/riscv32imc-unknown-none-elf/debug/embive-program 2>/dev/null || \
     echo "Binary not found. Run 'just embive-program' first."

# Show linker script symbols (stack_start, heap_start, etc.)
elf-symbols:
    @echo "🔍 Linker Script Symbols:"
    @nm target/riscv32imac-unknown-none-elf/debug/embive-program 2>/dev/null | \
     grep -E "(__stack_start|__heap_start|__heap_end|_end|_bss|__data|__bss)" || \
     nm target/riscv32imc-unknown-none-elf/debug/embive-program 2>/dev/null | \
     grep -E "(__stack_start|__heap_start|__heap_end|_end|_bss|__data|__bss)" || \
     echo "Binary not found. Run 'just embive-program' first."

# Show all symbols in the binary
elf-all-symbols:
    @nm target/riscv32imac-unknown-none-elf/debug/embive-program 2>/dev/null || \
     nm target/riscv32imc-unknown-none-elf/debug/embive-program 2>/dev/null || \
     echo "Binary not found. Run 'just embive-program' first."

# Disassemble code section
elf-disasm:
    @rust-objdump -d target/riscv32imac-unknown-none-elf/debug/embive-program 2>/dev/null | head -50 || \
     rust-objdump -d target/riscv32imc-unknown-none-elf/debug/embive-program 2>/dev/null | head -50 || \
     echo "Binary not found. Run 'just embive-program' first."

# Default recipe (run when just called without arguments)
default: check

