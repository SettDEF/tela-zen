#!/usr/bin/env bash
# Build and install the checkout this script lives in.
#
#   scripts/install.sh                    build and install
#   scripts/install.sh --run              …and launch it
#   scripts/install.sh --default-browser  …and make it the default browser
#
# The work is in ../install.sh, which is also the entry point people curl. Run
# from inside a checkout it builds that checkout rather than cloning, so this is
# a shorthand and not a second implementation.
set -euo pipefail
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/install.sh" "$@"
