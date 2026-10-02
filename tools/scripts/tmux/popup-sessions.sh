#!/bin/bash
# Launcher for bind s: the sessions-only twin of the bind f switcher.
primary=$(caelestia-color.sh primary)
tmux display-popup -E -w 90% -h 80% -S "fg=$primary" \
  "$(command -v tmux-popup-sessions-picker.sh)"
