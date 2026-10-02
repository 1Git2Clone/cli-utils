#!/bin/bash
# fzf body for the move-window popup (bind M-m): move window $1 to another
# session. The first row is always "new session: <query>", defaulting to the
# window's own name, so Enter on a fresh query creates that session;
# ctrl-j/k/n/p walk down to an existing one. A name that matches an existing
# session moves the window there instead.
# `--list Q` is the fzf reload source (typing re-lists instead of filtering,
# which is what keeps the "new" row pinned on top).
CC=caelestia-color.sh
export CUR_SESSION WIN_NAME
list() {
  printf 'NEW\t+ new session: %s\n' "${1:-$WIN_NAME}"
  # grep exits 1 when nothing matches, which pipefail would turn into a failure.
  tmux list-sessions -F '#{session_name}' | grep -vxF "$CUR_SESSION" | grep -iF -- "$1" | awk '{print "S\t" $0}' || true
}
[ "$1" = --list ] && { list "$2"; exit; }
{ read -r c1; read -r c2; read -r c3; } < <($CC primary secondary primaryFixedDim)

win="$1"
CUR_SESSION=$(tmux display -p -t "$win" '#{session_name}')
WIN_NAME=$(tmux display -p -t "$win" '#{window_name}')
out=$(list "" | fzf --disabled --print-query --no-sort --delimiter '\t' --with-nth 2 \
    --bind "change:reload($0 --list {q})+first" \
    --preview '[ {1} = S ] && tmux capture-pane -ep -t {2} | head -100' \
    --preview-window 'right:60%,border-rounded' \
    --color "fg:$c1,border:$c1,pointer:$c2,hl:$c2,hl+:$c2,header:$c3,prompt:$c1,info:$c1" \
    --border 'rounded' \
    --prompt '🔍 Move window to: ' \
    --header ' Type a name, Enter = new session ') || exit 0
query=$(sed -n 1p <<<"$out")
sel=$(sed -n 2p <<<"$out")
[ "${sel%%$'\t'*}" = NEW ] && query=${query:-$WIN_NAME}

ph=
if [ "${sel%%$'\t'*}" = NEW ] && ! tmux has-session -t "=$query" 2>/dev/null; then
  # A session can't exist without a window: create it with a placeholder,
  # move ours in, then drop the placeholder.
  read -r target ph < <(tmux new-session -dP -F '#{session_id} #{window_id}' -s "$query")
else
  name=$query
  [ "${sel%%$'\t'*}" = S ] && name=${sel#*$'\t'}
  target="=$name"
fi
tmux move-window -a -s "$win" -t "$target:" && [ -n "$ph" ] && tmux kill-window -t "$ph"
# Close the gap the window left behind (no-op if the source session is gone).
tmux move-window -r -t "=$CUR_SESSION:" 2>/dev/null
tmux switch-client -t "$target"
