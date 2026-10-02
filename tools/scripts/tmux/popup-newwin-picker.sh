#!/bin/bash
# fzf body for the "new window" popup (bind c). Colours are re-read from
# the live Caelestia scheme on every launch.
CC=caelestia-color.sh
{ read -r c1; read -r c2; read -r c3; } < <($CC primary secondary primaryFixedDim)
{
  echo "$HOME"
  zoxide query -l
} |
  fzf \
    --color "fg:$c1,pointer:$c2,hl:$c2,hl+:$c2,header:$c3,prompt:$c1,info:$c1" \
    --prompt '  dir: ' \
    --header ' New Window ' |
  xargs -r tmux-new-named-window.sh
