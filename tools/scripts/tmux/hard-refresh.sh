#!/usr/bin/env bash
# Hard-refresh a live tmux server without killing it: unset every option
# at every scope (server, global, per-session, per-window), clear global
# hooks, then re-source ~/.tmux.conf so only what the config declares
# survives. This is the no-downtime equivalent of `tmux kill-server` for
# exorcising stale options a removed config line or plugin left behind
# (e.g. a deprecated `status-bg` that `status-style` can't override).
#
# Panes and their processes are untouched.

set -u

# Strip array indices ("status-format[0]" -> "status-format"), dedupe.
opt_names() {
  awk '{print $1}' | sed 's/\[.*//' | sort -u
}

# Server options.
tmux show-options -s | opt_names | while read -r o; do
  tmux set-option -su "$o" 2>/dev/null || true
done

# Global session + global window options (covers deprecated ones like
# status-bg/status-fg, plugin leftovers, @user options).
#
# default-shell/default-command are skipped: tmux seeds them from $SHELL
# at server start and the config doesn't set them, so unsetting would
# flip new panes to /bin/sh.
tmux show-options -g | opt_names | while read -r o; do
  case "$o" in
  default-shell | default-command) continue ;;
  esac
  tmux set-option -gu "$o" 2>/dev/null || true
done
tmux show-window-options -g | opt_names | while read -r o; do
  tmux set-window-option -gu "$o" 2>/dev/null || true
done

# Per-session and per-window overrides (these shadow globals and survive
# every source-file).
tmux list-sessions -F '#{session_name}' | while read -r s; do
  tmux show-options -t "$s" | opt_names | while read -r o; do
    tmux set-option -u -t "$s" "$o" 2>/dev/null || true
  done
  tmux list-windows -t "$s" -F '#{session_name}:#{window_index}' |
    while read -r w; do
      tmux show-window-options -t "$w" | opt_names | while read -r o; do
        tmux set-window-option -u -t "$w" "$o" 2>/dev/null || true
      done
    done
done

# Global hooks.
tmux show-hooks -g | awk '$2 != "" {print $1}' | sed 's/\[.*//' | sort -u |
  while read -r h; do
    tmux set-hook -gu "$h" 2>/dev/null || true
  done

# Rebuild the intended state.
tmux source-file ~/.tmux.conf
tmux display-message "tmux hard refresh done"
