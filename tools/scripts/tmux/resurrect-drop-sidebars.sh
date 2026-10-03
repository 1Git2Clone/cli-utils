#!/bin/bash
# @resurrect-hook-post-restore-all. resurrect saves workmux's sidebar pane
# like any other and restores it as an empty shell, while workmux's own tmux
# hooks open a fresh sidebar in every restored window: one ghost pane per
# window. Drop the pane restored from each saved "workmux" pane, then
# rescale its window's remaining panes to fill the space the same way
# workmux's own sidebar-off does (tmux-layout-after-sidebar-remove.py) --
# a bare kill-pane instead hands all of it to whichever pane is the killed
# pane's tree sibling, which is not necessarily what you want enlarged.
# resurrect's own default, not XDG: it never looks there unless told to.
dir=$(tmux show -gqv @resurrect-dir)
last=${dir:-$HOME/.tmux/resurrect}/last
[ -f "$last" ] || exit 0
# Saved pane lines: pane, session, window, ..., pane_index ($6), ..., command ($10).
awk -F'\t' '$1 == "pane" && $10 == "workmux" { print "pane-" $2 ":" $3 "." $6 }' "$last" |
  while read -r tag; do
    # Restored panes are started as `cat '.../<tag>'; exec ...`. Never kill a window's last pane.
    tmux list-panes -a -F '#{window_id} #{pane_id} #{window_panes} #{pane_start_command}' |
      awk -v t="$tag'" '$3 > 1 && index($0, t) { print $1, $2 }' |
      while read -r win pane; do
        layout=$(tmux-layout-after-sidebar-remove.py "$win" "$pane" 2>/dev/null || true)
        tmux kill-pane -t "$pane" 2>/dev/null || true
        [ -z "$layout" ] || tmux select-layout -t "$win" "$layout" 2>/dev/null || true
      done
  done
