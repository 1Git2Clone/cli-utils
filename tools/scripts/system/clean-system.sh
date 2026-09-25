#!/bin/bash
###############################################################################
# A script to remove some cache, unused packages and duplicated files inside  #
#                          your home directory.                               #
###############################################################################

###############################################################################
# Remove store paths nothing references. Old generations are kept, so rollback
# still works; `nix-collect-garbage -d` is the one that drops them.
###############################################################################
sudo nix-collect-garbage

###############################################################################
# Remove the cache from your home directory.
###############################################################################
rm -rf ~/.cache/*

###############################################################################
# Run an app that checks for duplicate files in the home directory.
###############################################################################
rmlint ~/
chmod +x rmlint.sh
./rmlint.sh # By default this script gets deleted afterwards.
