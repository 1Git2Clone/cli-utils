#!/bin/bash
# @resurrect-hook-post-restore-all. resurrect saves workmux's sidebar pane
# like any other and restores it as an empty shell, while workmux's own tmux
# hooks open a fresh sidebar in every restored window: one ghost pane per
# window. Kill the pane restored from each saved "workmux" pane.
last=${XDG_DATA_HOME:-$HOME/.local/share}/tmux/resurrect/last
[ -f "$last" ] || exit 0
# Saved pane lines: pane, session, window, ..., pane_index ($6), ..., command ($10).
awk -F'\t' '$1 == "pane" && $10 == "workmux" { print "pane-" $2 ":" $3 "." $6 }' "$last" |
  while read -r tag; do
    # Restored panes are started as `cat '.../<tag>'; exec ...`. Never kill a window's last pane.
    tmux list-panes -a -F '#{pane_id} #{window_panes} #{pane_start_command}' |
      awk -v t="$tag'" '$2 > 1 && index($0, t) { print $1 }' |
      xargs -r -n1 tmux kill-pane -t
  done
