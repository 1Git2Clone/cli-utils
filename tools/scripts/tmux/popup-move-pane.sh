#!/bin/bash
# Launcher for bind M. $1 is the invoking pane's id, passed in from
# tmux.conf via #{pane_id}: inside the popup there is no "current pane".
pane="$1"
primary=$(caelestia-color.sh primary)
tmux display-popup -E -w 90% -h 80% -S "fg=$primary" \
  "$(command -v tmux-popup-move-pane-picker.sh) '$pane'"
