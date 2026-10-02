#!/bin/bash
# fzf body for the move-window popup (bind M-m): move window $1 to another
# session. The current session is never a candidate -- not as a "new"
# session (its name already exists), not relabelled "existing" either:
# its row just isn't there, the same way it's already excluded below from
# the list of other sessions. The top row otherwise defaults to the
# window's own name and relabels itself "existing session: <name>" once
# that name already belongs to some other session.
# `--list Q` is the fzf reload source (typing re-lists instead of filtering,
# which is what keeps a real top row pinned).
CC=caelestia-color.sh
export CUR_SESSION WIN_NAME
list() {
  local name=${1:-$WIN_NAME}
  if [ "$name" != "$CUR_SESSION" ]; then
    if tmux has-session -t "=$name" 2>/dev/null; then
      printf 'NEW\t\xe2\x86\xa6 existing session: %s\n' "$name"
    else
      printf 'NEW\t+ new session: %s\n' "$name"
    fi
  fi
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

name=$query
[ "${sel%%$'\t'*}" = S ] && name=${sel#*$'\t'}

# The current session can only reach here if nothing in the (empty) list
# got selected and the typed/default query itself is it -- still a no-op.
[ -n "$name" ] && [ "$name" != "$CUR_SESSION" ] || exit 0

ph=
if ! tmux has-session -t "=$name" 2>/dev/null; then
  # A session can't exist without a window: create it with a placeholder,
  # move ours in, then drop the placeholder.
  read -r target ph < <(tmux new-session -dP -F '#{session_id} #{window_id}' -s "$name")
else
  target="=$name"
fi
tmux move-window -a -s "$win" -t "$target:" && [ -n "$ph" ] && tmux kill-window -t "$ph"
# Close the gap the window left behind (no-op if the source session is gone).
tmux move-window -r -t "=$CUR_SESSION:" 2>/dev/null
tmux switch-client -t "$target"
