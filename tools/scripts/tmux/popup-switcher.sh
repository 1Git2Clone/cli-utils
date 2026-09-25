#!/bin/bash
# Launcher for bind f. $1 is the invoking pane's current path, passed in
# from tmux.conf via #{pane_current_path}.
dir="$1"
primary=$(caelestia-color.sh primary)
tmux display-popup -E -w 90% -h 80% -S "fg=$primary" -d "$dir" \
  "$(command -v tmux-popup-switcher-picker.sh)"
