#!/bin/bash
# Print hex colours (with leading #) from the *live* Caelestia scheme, one per
# line, read fresh from disk on every call so callers stay in sync with
# whatever scheme is currently active (no baked-in hex, no reload hooks).
# Several keys in one call cost one process, not one each (popups call it hot).
# Usage: caelestia-color.sh <key>...   e.g. primary secondary primaryFixedDim
scheme="$HOME/.local/state/caelestia/scheme.json"
jq -r '.colours as $c | $ARGS.positional[] | "#" + $c[.]' "$scheme" --args "$@"
