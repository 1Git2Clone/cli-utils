#!/bin/bash
# Launcher for bind \ ($1 = pin) and bind BSpace ($1 = unpin). $2 is the
# invoking pane's id, passed in from tmux.conf via #{pane_id}.
primary=$(caelestia-color.sh primary)
tmux display-popup -E -w 70% -h 14 -S "fg=$primary" \
  "$(command -v tmux-popup-pins-picker.sh) '$1' '$2'"
