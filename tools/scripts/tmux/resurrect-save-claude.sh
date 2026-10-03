#!/bin/bash
# @resurrect-hook-post-save-all. Claude's SessionStart hook (claude-tag-pane.sh)
# tags its pane with the conversation it holds (@claude-session, a pane option,
# so it follows the pane wherever it moves). Record each tag under the pane's
# position as of this save, which is where resurrect will put the pane back.
state=${XDG_STATE_HOME:-$HOME/.local/state}/tmux-claude-sessions
mkdir -p "$(dirname "$state")"
tmux list-panes -a -F '#{session_name}:#{window_index}.#{pane_index}'$'\t''#{@claude-session}' |
  awk -F'\t' '$2 != ""' >"$state.tmp"
mv "$state.tmp" "$state"
