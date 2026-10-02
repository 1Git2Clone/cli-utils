#!/bin/bash
# Body of the pins popup. $1 is "pin" (bind \) or "unpin" (bind BSpace), $2 the
# pane you came from (its session, window name and pane index are what gets
# pinned). Shows the ten slots (1-9, then 0); one digit keypress picks the
# slot, any other key closes. No trailing newline on the last line: it would
# scroll the first slot out of the popup.
op=$1
file=${XDG_STATE_HOME:-$HOME/.local/state}/tmux-pins
mkdir -p "$(dirname "$file")"
[ -f "$file" ] || : >"$file"
tab=$'\t'
entry=$(tmux display -p -t "$2" "#{session_name}$tab#{window_name}$tab#{pane_index}")
for n in 1 2 3 4 5 6 7 8 9 0; do
  rest=$(awk -v n="$n" '{ s = index($0, " ") } substr($0, 1, s - 1) == n { print substr($0, s + 1) }' "$file")
  printf ' %s  %s\n' "$n" "${rest//$tab/ > }"
done
if [ "$op" = pin ]; then
  printf '\n pin %s to:' "${entry//$tab/ > }"
else
  printf '\n unpin:'
fi
read -rsn1 k || exit 0
case $k in [0-9]) ;; *) exit 0 ;; esac
# Rewrite the file without slot $k (and, when pinning, without this exact pin elsewhere).
awk -v n="$k" -v e="$entry" -v op="$op" '
  { s = index($0, " "); r = substr($0, s + 1) }
  substr($0, 1, s - 1) != n && !(op == "pin" && r == e) { print }
  END { if (op == "pin") print n " " e }' "$file" | sort -n >"$file.tmp"
mv "$file.tmp" "$file"
