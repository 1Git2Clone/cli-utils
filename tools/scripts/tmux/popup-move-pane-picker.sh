#!/bin/bash
# fzf body for the move-pane popup (bind M): pick a window by name and move
# pane $1 into it. Colours are re-read from the live Caelestia scheme.
CC=caelestia-color.sh
pane="$1"
win=$(tmux display -p -t "$pane" '#{window_id}')
target=$(tmux list-windows -a -f "#{!=:#{window_id},$win}" \
  -F '#{session_name}:#{window_index} #{window_name} #{pane_current_path}' |
  fzf --ansi \
    --preview 'tmux capture-pane -ep -t {1} | head -100' \
    --preview-window 'right:60%,border-rounded' \
    --color "fg:$($CC primary),border:$($CC primary),pointer:$($CC secondary),hl:$($CC secondary),hl+:$($CC secondary),header:$($CC primaryFixedDim),prompt:$($CC primary),info:$($CC primary)" \
    --border 'rounded' \
    --prompt '🔍 Move pane to: ' \
    --header ' Sessions / Windows ' \
    --delimiter ' ' |
  cut -d' ' -f1)
[ -n "$target" ] || exit 0
# switch-client too: move-pane only follows the pane within its own session.
tmux move-pane -s "$pane" -t "$target" \; select-layout -E -t "$target" \; switch-client -t "$target"
