#!/bin/bash

set -euo pipefail

SESSION="${SESSION:-${1:-"code"}}"

tmux new-session -d -s "$SESSION"

# Every command targets $SESSION: untargeted, they hit whichever session tmux
# considers current, which is not the new one when others exist.
# Three even columns...
tmux split-window -h -t "$SESSION"
tmux split-window -h -t "$SESSION"
tmux select-layout -t "$SESSION" even-horizontal

# ...the left one halved...
tmux split-window -v -t "$SESSION:.{top-left}"

# ...and the right one in thirds. Sized per split, since select-layout even-v
# would restack the whole window rather than this column.
tmux split-window -v -l 66% -t "$SESSION:.{top-right}"
tmux split-window -v -l 50% -t "$SESSION:.{bottom-right}"

# Attach to the session
tmux attach-session -t "$SESSION"
