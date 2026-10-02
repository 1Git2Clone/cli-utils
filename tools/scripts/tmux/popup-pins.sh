#!/bin/bash
# Launcher for bind j. $1 is the invoking session's id, passed in from
# tmux.conf via #{session_id}.
primary=$(caelestia-color.sh primary)
tmux display-popup -E -w 50 -h 8 -S "fg=$primary" \
  "$(command -v tmux-popup-pins-picker.sh) '$1'"
