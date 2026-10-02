#!/bin/bash
# fzf body for the sessions popup (bind s): switch to a session, most recently
# active first. Once you type, a "+ new session: <query>" row
# is added as the last option and Enter on it creates the session. `--list Q`
# is the fzf reload source (typing re-lists instead of filtering, which is
# what keeps the "new" row there).
CC=caelestia-color.sh
# The session you are in goes last, so Enter on a fresh popup hops to the
# previous one. Exported so the fzf reload (a child process) keeps it.
: "${CUR:=$(tmux display -p '#{session_id}')}"
export CUR
list() {
  tmux list-sessions -F '#{session_activity} #{session_id}	#{session_name} (#{session_windows} windows)' |
    sort -rn |
    awk -v cur="$CUR" -v q="$1" '
      BEGIN { q = tolower(q) }
      { sub(/^[0-9]+ /, "") }
      index(tolower($0), q) { if ($1 == cur) last = $0; else print "S\t" $0 }
      END { if (last) print "S\t" last }' || true
  [ -z "$1" ] || printf 'NEW\t-\t+ new session: %s\n' "$1"
}
[ "${1:-}" = --list ] && { list "$2"; exit; }
{ read -r c1; read -r c2; read -r c3; } < <($CC primary secondary primaryFixedDim)

out=$(list "" | fzf --disabled --print-query --no-sort --delimiter '\t' --with-nth 3 \
    --bind "change:reload($0 --list {q})+first" \
    --preview '[ {1} = S ] && tmux capture-pane -ep -t {2} | head -100' \
    --preview-window 'right:60%,border-rounded' \
    --color "fg:$c1,border:$c1,pointer:$c2,hl:$c2,hl+:$c2,header:$c3,prompt:$c1,info:$c1" \
    --border 'rounded' \
    --prompt '🔍 Switch to session: ' \
    --header ' Type a name, the last row creates a new session ') || exit 0
query=$(sed -n 1p <<<"$out")
sel=$(sed -n 2p <<<"$out")
case $sel in
  NEW*) tmux has-session -t "=$query" 2>/dev/null || tmux new-session -ds "$query"
        tmux switch-client -t "=$query" ;;
  S*)   id=$(cut -f2 <<<"$sel"); tmux switch-client -t "$id" ;;
esac
