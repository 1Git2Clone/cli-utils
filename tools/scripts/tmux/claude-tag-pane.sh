#!/bin/bash
# Claude's SessionStart hook: tag this pane with the conversation it now holds
# (@claude-session, from the hook's JSON on stdin), for resurrect-save-claude.sh.
[ -n "${TMUX_PANE:-}" ] || exit 0
tmux set -p -t "$TMUX_PANE" @claude-session "$(jq -r .session_id)"
