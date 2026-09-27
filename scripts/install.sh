#!/usr/bin/env bash
# Build, package and install Tela, then refresh the desktop databases so it
# shows up in your launcher.
#
#   scripts/install.sh          build and install
#   scripts/install.sh --run    …and launch it afterwards
#
# Safe to re-run: pkgver() bumps on every build, so pacman treats each one as
# an upgrade rather than refusing a reinstall.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$here/packaging"

echo "==> Building package"
# -f overwrites a previous build, -s pulls missing deps, -i installs.
makepkg -fsi --noconfirm

echo "==> Refreshing desktop database"
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database ~/.local/share/applications 2>/dev/null || true
  sudo update-desktop-database /usr/share/applications 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  sudo gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
fi

echo "==> Installed: $(pacman -Q tela 2>/dev/null || echo 'not found')"
echo "    binary:  $(command -v tela || echo 'not on PATH')"
echo "    desktop: /usr/share/applications/tela.desktop"

# Offer to make it the default browser only when asked, never silently.
if [ "${1:-}" = "--default-browser" ]; then
  xdg-settings set default-web-browser tela.desktop && echo "    set as default browser"
fi

if [ "${1:-}" = "--run" ]; then
  echo "==> Launching"
  exec tela
fi
