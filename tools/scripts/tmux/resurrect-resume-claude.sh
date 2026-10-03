#!/bin/bash
# resurrect's restore command for claude panes: reopen the conversation this
# pane held at the last save (resurrect-save-claude.sh), or the picker if it
# held none. --continue is no substitute: it takes the directory's newest
# conversation, so every claude pane in one directory reopens the same one.
state=${XDG_STATE_HOME:-$HOME/.local/state}/tmux-claude-sessions
# The saved position, from the `cat '.../pane-<position>'` resurrect starts the
# pane with: the current one shifts once drop-sidebars kills the pane before it.
pane=$(tmux display -p -t "$TMUX_PANE" '#{pane_start_command}' | sed -n "s|.*/pane-\([^']*\)'.*|\1|p")
[ -n "$pane" ] || pane=$(tmux display -p -t "$TMUX_PANE" '#{session_name}:#{window_index}.#{pane_index}')
id=$(awk -F'\t' -v p="$pane" '$1 == p { print $2; exit }' "$state" 2>/dev/null || true)
exec claude --resume ${id:+"$id"}
