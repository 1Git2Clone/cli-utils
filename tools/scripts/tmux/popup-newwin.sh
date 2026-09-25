#!/bin/bash
# Launcher for bind c: computes the popup border colour synchronously
# (tmux's own #() is async and would resolve empty on a fresh command),
# then opens the popup.
primary=$(caelestia-color.sh primary)
tmux display-popup -E -w 60% -h 50% -S "fg=$primary" \
  "$(command -v tmux-popup-newwin-picker.sh)"
