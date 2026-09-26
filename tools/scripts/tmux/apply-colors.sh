#!/bin/bash
# Pushes the live Caelestia colours (from colors.sh) into tmux as real
# set-option calls with already-resolved hex — not tmux's own #(...),
# which validates status-style/pane-border-style eagerly at set-time and
# rejects a shell-command placeholder outright ("invalid style").
# Run again (or just `tmux source-file ~/.tmux.conf`) any time to re-sync
# after a scheme switch.
# shellcheck source=/dev/null
source "$HOME/.profile.d/colors.sh"

# workmux gives each window it manages a local copy of the window formats with
# its status appended, and that copy would keep the old colours. Whatever was
# appended to the old global carries over onto the new one.
formats=(window-status-format window-status-current-format)
declare -A before
for o in "${formats[@]}"; do before[$o]=$(tmux show -gv "$o"); done

tmux set-option -g status-style "fg=$ON_SURFACE,bg=$BACKGROUND,overline"
tmux set-option -g status-left "#[fg=$ON_PRIMARY,bg=$PRIMARY,bold]  #S #[fg=$PRIMARY,bg=$BACKGROUND,nobold]"
tmux set-window-option -g window-status-format "#[fg=$SUBTEXT0,bg=$BACKGROUND] #I  #W "
tmux set-window-option -g window-status-current-format "#[fg=$PRIMARY,bg=$BACKGROUND] #[fg=$ON_PRIMARY,bg=$PRIMARY,bold] #I  #W #[fg=$PRIMARY,bg=$BACKGROUND,nobold] "
# continuum drives its autosave off a #(...) it prepends to status-right, so
# overwriting the option blind would stop the saves without saying anything.
autosave=$(tmux show -gv status-right | grep -o '#([^)]*continuum_save\.sh)' || true)
tmux set-option -g status-right "$autosave#[fg=$PRIMARY,bg=$BACKGROUND]#[fg=$ON_PRIMARY,bg=$PRIMARY]  %H:%M  %d • %b • %y "
tmux set-option -g pane-border-style "fg=$OUTLINE"
tmux set-option -g pane-active-border-style "fg=$PRIMARY"

for w in $(tmux list-windows -a -F '#{window_id}'); do
  for o in "${formats[@]}"; do
    local_fmt=$(tmux show -wv -t "$w" "$o")
    [[ -n $local_fmt && $local_fmt == "${before[$o]}"* ]] || continue
    tmux set-window-option -t "$w" "$o" "$(tmux show -gv "$o")${local_fmt#"${before[$o]}"}"
  done
done
