#!/usr/bin/env bash
# scripts/setup-claude-symlinks.sh
#
# Wire Claude Code's `.claude/` loader up to the canonical
# `.agents/` resource tree, so anything Claude reads from
# `.claude/{rules,skills,agents}` is the same content the rest
# of the toolchain sees under `.agents/`.
#
# Idempotent — re-running just refreshes the links.
#
# Layout produced:
#
#   .claude/
#     settings.json          (kept; project-local Claude config)
#     rules    -> ../.agents/         (if .agents/ exists)
#     skills   -> ../.agents/skills/  (if .agents/skills/ exists)
#     agents   -> ../.agents/agents/  (if .agents/agents/ exists)

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
AGENTS="$ROOT/.agents"
CLAUDE="$ROOT/.claude"

if [ ! -d "$AGENTS" ]; then
    echo "error: $AGENTS does not exist" >&2
    exit 1
fi

if [ ! -d "$CLAUDE" ]; then
    echo "error: $CLAUDE does not exist" >&2
    exit 1
fi

link_pair() {
    # link_pair <name> <target>
    local name="$1"
    local target="$2"
    local link_path="$CLAUDE/$name"

    if [ ! -e "$target" ] && [ ! -L "$target" ]; then
        echo "skip   .claude/$name   (target $target missing)"
        return 0
    fi

    # Remove any existing entry (file, dir, or symlink).
    if [ -e "$link_path" ] || [ -L "$link_path" ]; then
        rm -rf "$link_path"
    fi

    # Relative symlink so the repo is portable across clones.
    # macOS BSD `realpath` lacks GNU `--relative-to`; compute it
    # with Python for portability.
    local rel
    rel="$(python3 -c "import os,sys; print(os.path.relpath(sys.argv[1], sys.argv[2]))" "$target" "$CLAUDE")"
    ln -s "$rel" "$link_path"
    echo "linked .claude/$name -> $rel"
}

link_pair rules  "$AGENTS"
link_pair skills "$AGENTS/skills"
link_pair agents "$AGENTS/agents"

echo
echo "done. Claude Code will now load:"
echo "  .claude/rules    -> .agents/             (.md files loaded as rules)"
echo "  .claude/skills   -> .agents/skills/     (if present)"
echo "  .claude/agents   -> .agents/agents/     (if present)"