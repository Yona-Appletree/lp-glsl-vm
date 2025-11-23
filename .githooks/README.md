# Git Hooks

This directory contains git hooks that call validation scripts from `scripts/`.

The actual validation logic is in `scripts/` - hooks are just thin wrappers that call those scripts.

## Installation

Run once after cloning the repository:

```bash
just install-hooks
```

This will install:

- **commit-msg**: Validates semantic commit messages (calls `scripts/validate-commit-msg.sh`)
- **pre-push**: Validates branch naming conventions (calls `scripts/validate-branch-name.sh`)
- **pre-commit**: Runs basic formatting check (full validation via `just all`)

## Bypassing Hooks

If you need to bypass hooks (not recommended), use:

```bash
git commit --no-verify  # Skip pre-commit and commit-msg hooks
git push --no-verify     # Skip pre-push hook
```
