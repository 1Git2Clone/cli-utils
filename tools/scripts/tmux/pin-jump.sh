#!/bin/bash
# Alt+1..9,0 (root-table binds): jump to the pin in slot $1. A pin is a line
# "<slot> <session><TAB><window name><TAB><pane index>" in the pins file;
# popup-pins-picker.sh edits it. A pin with no window or pane (the older
# session-only kind) just switches session.
file=${XDG_STATE_HOME:-$HOME/.local/state}/tmux-pins
sess= win= pane=
IFS=$'\t' read -r sess win pane < <(awk -v n="$1" '
  { s = index($0, " ") }
  substr($0, 1, s - 1) == n { print substr($0, s + 1) }' "$file" 2>/dev/null || true) || true
[ -n "$sess" ] || { tmux display-message "pin $1: empty"; exit 0; }
tmux has-session -t "=$sess" 2>/dev/null || { tmux display-message "pin $1: no session '$sess'"; exit 0; }
if [ -n "$win" ]; then
  idx=$(tmux list-windows -t "=$sess" -F '#{window_index}'$'\t''#{window_name}' |
    awk -F'\t' -v w="$win" '$2 == w { print $1; exit }')
  if [ -n "$idx" ]; then
    tmux select-window -t "=$sess:$idx"
    [ -z "$pane" ] || tmux select-pane -t "=$sess:$idx.$pane" 2>/dev/null || true
  fi
fi
tmux switch-client -t "=$sess"
