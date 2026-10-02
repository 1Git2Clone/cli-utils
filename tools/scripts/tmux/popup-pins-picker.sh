#!/bin/bash
# Body of the pins popup (bind j): shows the four pin slots; a digit pins the
# current session ($1 is its id) there, "-" then a digit unpins that slot.
# One keypress each, no Enter. Any other key just closes the popup.
file=${XDG_STATE_HOME:-$HOME/.local/state}/tmux-pins
mkdir -p "$(dirname "$file")"
[ -f "$file" ] || : >"$file"
cur=$(tmux display -p -t "$1" '#{session_name}')
for n in 1 2 3 4; do
  name=$(awk -v n="$n" '$1 == n { sub(/^[^ ]+ /, ""); print }' "$file")
  printf ' %s  %s\n' "$n" "${name:--}"
done
printf '\n n pins "%s" · -n unpins\n' "$cur"
op=pin
read -rsn1 k || exit 0
if [ "$k" = - ]; then op=unpin; read -rsn1 k || exit 0; fi
case $k in [1-4]) ;; *) exit 0 ;; esac
# Rewrite the file without slot $k (and, when pinning, without $cur's old slot).
awk -v n="$k" -v c="$cur" -v op="$op" '
  { r = $0; sub(/^[^ ]+ /, "", r) }
  $1 != n && !(op == "pin" && r == c) { print }
  END { if (op == "pin") print n " " c }' "$file" | sort -n >"$file.tmp"
mv "$file.tmp" "$file"
