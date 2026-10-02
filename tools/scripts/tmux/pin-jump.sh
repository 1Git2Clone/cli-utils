#!/bin/bash
# Alt+1..4 (root-table binds): jump to the session pinned in slot $1.
# Pins live in a plain "<slot> <session name>" file; popup-pins-picker.sh edits it.
file=${XDG_STATE_HOME:-$HOME/.local/state}/tmux-pins
name=$(awk -v n="$1" '$1 == n { sub(/^[^ ]+ /, ""); print }' "$file" 2>/dev/null || true)
[ -n "$name" ] || { tmux display-message "pin $1: empty"; exit 0; }
tmux switch-client -t "=$name" 2>/dev/null || tmux display-message "pin $1: no session '$name'"
