#!/bin/sh
# Validate branch naming conventions

# Get the current branch name
branch_name=$(git rev-parse --abbrev-ref HEAD)

# Allowed branches: main, renovate/*, feature/*
# Branch naming regex
branch_regex='^(main|renovate/[a-z0-9._-]+|feature/[a-z0-9._-]+)$'

# Check if branch name matches convention
if ! echo "$branch_name" | grep -qE "$branch_regex"; then
    echo "❌ Error: Branch name '$branch_name' does not follow the naming convention."
    echo ""
    echo "Allowed branch names:"
    echo "  - main"
    echo "  - renovate/<description>"
    echo "  - feature/<description>"
    echo ""
    echo "Examples:"
    echo "  main"
    echo "  renovate/dependencies"
    echo "  feature/add-vm-execution"
    echo ""
    exit 1
fi

exit 0








