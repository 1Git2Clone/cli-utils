#!/usr/bin/env bash
# End to end, the resurrect hooks: build layouts, save, kill the server,
# restore, and check every pane came back as what it was. Runs against a
# private socket with a throwaway HOME and XDG dirs, so it never sees a real
# tmux server. Needs $RESURRECT (the plugin's directory) and the cli-utils
# tmux scripts on PATH; tests.nix provides both.
set -euo pipefail

work=$(mktemp -d)
sock=$work/sock1
T() { tmux -S "$sock" "$@"; }
trap 'T kill-server 2>/dev/null || true; rm -rf "$work"' EXIT
unset TMUX TMUX_PANE
export HOME=$work/home XDG_STATE_HOME=$work/state XDG_DATA_HOME=$work/data \
  XDG_CONFIG_HOME=$work/config PATH=$work/bin:$PATH
mkdir -p "$HOME" "$work/bin"
bash=$(command -v bash)

# Stand-ins that keep running like the real programs. tmux and resurrect name
# a pane by its process's argv[0], which for a script is the interpreter, so
# each ends by exec'ing a bash that carries the program's name and idles on
# the terminal. (Not sleep: a multicall coreutils dispatches on argv[0].)
# claude takes its conversation from --resume/--session-id (none is the
# picker) and fires the real SessionStart hook first.
cat >"$work/bin/claude" <<EOF
#!$bash
id=PICKER
case \${1:-} in --resume | --session-id) id=\${2:-PICKER} ;; esac
printf '{"session_id":"%s"}' "\$id" | tmux-claude-tag-pane.sh
exec -a claude $bash -c 'read -r _'
EOF
# workmux draws something, as the real sidebar does: resurrect saves no
# contents for a blank pane, so it would come back without the
# `cat .../pane-<position>` start command both hooks identify panes by.
printf '#!%s\necho agents\nexec -a workmux %s -c "read -r _"\n' "$bash" "$bash" >"$work/bin/workmux"
chmod +x "$work/bin/claude" "$work/bin/workmux"

# The resurrect half of dot-tmux.conf in nixos-dotfiles, except that claude
# panes restore a second late: a real login shell is slow enough to start that
# drop-sidebars has already renumbered the panes by then, and this one is not.
cat >"$work/tmux.conf" <<EOF
set -g default-shell $bash
set -g @resurrect-dir '$work/resurrect'
set -g @resurrect-capture-pane-contents 'on'
set -g @resurrect-processes '"~claude->sleep 1; tmux-resurrect-resume-claude.sh"'
set -g @resurrect-hook-post-save-all 'tmux-resurrect-save-claude.sh'
set -g @resurrect-hook-post-restore-all 'tmux-resurrect-drop-sidebars.sh'
run-shell $RESURRECT/resurrect.tmux
EOF

# A window as its panes in index order: the conversation a claude pane holds,
# otherwise the running command.
sig() { T list-panes -t "$1" -F '#{?#{@claude-session},#{@claude-session},#{pane_current_command}}' | xargs; }

failed=0
expect() { # <window> <signature> <what>
  local got=''
  for _ in $(seq 100); do
    got=$(sig "$1" 2>&1) || true
    [ "$got" = "$2" ] && { echo "ok   $3"; return; }
    sleep 0.1
  done
  echo "FAIL $3: $1 is '$got', want '$2'" >&2
  failed=1
}
run() { T send-keys -t "$1" "$2" Enter; }

T -f "$work/tmux.conf" new-session -d -s plain -x 200 -y 50

# Panes moved after claude started: the tag follows the pane.
T split-window -h -t plain:0
T split-window -h -t plain:0
run plain:0.1 'claude --session-id A'
run plain:0.2 'claude --session-id B'
T new-window -t plain:1
run plain:1.0 'claude --session-id C'
expect plain:0 'bash A B' 'setup: plain'
T swap-pane -s plain:0.1 -t plain:0.2
T join-pane -h -s plain:1.0 -t plain:0.0
expect plain:0 'bash C B A' 'setup: plain, moved'

# workmux's sidebar at .0: its ghost is dropped after the restore, renumbering
# the claude panes behind it.
T new-session -d -s sidebar -x 200 -y 50 workmux
T split-window -h -t sidebar:0
T split-window -h -t sidebar:0
run sidebar:0.1 'claude --session-id D'
run sidebar:0.2 'claude --session-id E'
expect sidebar:0 'workmux D E' 'setup: sidebar'
T swap-pane -s sidebar:0.1 -t sidebar:0.2
expect sidebar:0 'workmux E D' 'setup: sidebar, moved'

# No tag (a claude started before the hook existed), and a second
# SessionStart in the same pane (/clear) replacing the first tag.
T new-session -d -s misc -x 200 -y 50
T split-window -h -t misc:0
run misc:0.0 'claude --session-id F'
run misc:0.1 'claude --session-id G'
expect misc:0 'F G' 'setup: misc'
T set -p -u -t misc:0.0 @claude-session
printf '{"session_id":"G2"}' |
  TMUX="$sock,0,0" TMUX_PANE=$(T display -p -t misc:0.1 '#{pane_id}') tmux-claude-tag-pane.sh
expect misc:0 'claude G2' 'setup: misc, retagged'

T run-shell "$RESURRECT/scripts/save.sh"
T kill-server
# A fresh socket: the old server may still be shutting down on its own.
sock=$work/sock2
T -f "$work/tmux.conf" new-session -d -s boot -x 200 -y 50
T run-shell "$RESURRECT/scripts/restore.sh"

expect plain:0 'bash C B A' 'panes moved after start reopen their own conversation'
expect sidebar:0 'E D' 'sidebar ghost dropped, claude panes behind it unaffected'
expect misc:0 'PICKER G2' 'untagged pane gets the picker, retagged pane the newer conversation'

exit "$failed"
