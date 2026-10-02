#!/bin/bash
# Launcher for bind M-m. $1 is the invoking window's id, passed in from
# tmux.conf via #{window_id}: inside the popup there is no "current window".
win="$1"
primary=$(caelestia-color.sh primary)
tmux display-popup -E -w 60% -h 50% -S "fg=$primary" \
  "$(command -v tmux-popup-move-window-picker.sh) '$win'"
