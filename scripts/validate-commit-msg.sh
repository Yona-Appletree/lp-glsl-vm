#!/bin/sh
# Validate semantic commit messages

commit_msg_file="$1"
commit_msg=$(cat "$commit_msg_file")

# Semantic commit message regex
# Format: <type>(<scope>): <description>
# Types: feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert
semantic_regex='^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(\([a-zA-Z0-9_-]+\))?: .{1,}'

# Check if commit message matches semantic format
if ! echo "$commit_msg" | grep -qE "$semantic_regex"; then
    echo "❌ Error: Commit message does not follow semantic commit format."
    echo ""
    echo "Format: <type>[optional scope]: <description>"
    echo ""
    echo "Types: feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert"
    echo ""
    echo "Examples:"
    echo "  feat(vm): add support for GLSL functions"
    echo "  fix(parser): handle edge case in expression parsing"
    echo "  docs: update README with installation instructions"
    echo "  refactor: simplify error handling logic"
    echo ""
    echo "Your message:"
    echo "  $commit_msg"
    exit 1
fi

exit 0






