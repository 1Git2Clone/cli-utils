#!/bin/bash
# fzf body for the session/window switcher popup (bind f). Colours are
# re-read from the live Caelestia scheme on every launch.
CC=caelestia-color.sh
{ read -r c1; read -r c2; read -r c3; } < <($CC primary secondary primaryFixedDim)
tmux list-windows -a -F '#{session_name}:#{window_index} #{window_name} #{pane_current_path}' |
  fzf --ansi \
    --preview 'tmux capture-pane -ep -t {1} | head -100' \
    --preview-window 'right:60%,border-rounded' \
    --color "fg:$c1,border:$c1,pointer:$c2,hl:$c2,hl+:$c2,header:$c3,prompt:$c1,info:$c1" \
    --border 'rounded' \
    --prompt '🔍 Switch to: ' \
    --header ' Sessions / Windows ' \
    --delimiter ' ' |
  cut -d' ' -f1 |
  xargs tmux switch-client -t
