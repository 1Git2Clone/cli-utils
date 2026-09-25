#!/bin/bash

color_shift.py ./config/starship.toml ./config/starship.toml "$1"
color_shift.py ./config/HyprV/kitty/mocha.conf ./config/HyprV/kitty/mocha.conf "$1"
color_shift.py ./config/nvim/lua/plugins/colorscheme.lua ./config/nvim/lua/plugins/colorscheme.lua "$1"
