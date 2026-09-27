#!/usr/bin/env bash
#
# Tela installer.
#
#   curl -fsSL https://raw.githubusercontent.com/SettDEF/tela-zen/main/install.sh | bash
#
# …or, to pass options through the pipe (note the `-s --`):
#
#   curl -fsSL https://raw.githubusercontent.com/SettDEF/tela-zen/main/install.sh \
#     | bash -s -- --yes --run
#
# Run from inside a checkout it builds that checkout instead of cloning, which
# is what `scripts/install.sh` does.
#
# Safe to re-run: the clone is updated, and pkgver() bumps on every build so
# pacman treats each one as an upgrade rather than refusing a reinstall.
set -euo pipefail

REPO_URL="https://github.com/SettDEF/tela-zen"
BRANCH="main"
SRC_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/tela-zen"

ASSUME_YES=0
SKIP_DEPS=0
SET_DEFAULT=0
LAUNCH=0

# ── output ──────────────────────────────────────────────────────────────────

if [ -t 1 ]; then
  B=$'\033[1m'; DIM=$'\033[2m'; RED=$'\033[31m'; GREEN=$'\033[32m'; R=$'\033[0m'
else
  B=''; DIM=''; RED=''; GREEN=''; R=''
fi

step() { printf '%s==>%s %s\n' "$B" "$R" "$*"; }
info() { printf '    %s\n' "$*"; }
note() { printf '    %s%s%s\n' "$DIM" "$*" "$R"; }
die()  { printf '%serror:%s %s\n' "$RED" "$R" "$*" >&2; exit 1; }

# Piped into bash, stdin IS the script — so a prompt must read the terminal
# directly, and must check it can actually be OPENED, not merely that it exists.
have_tty() { exec 3</dev/tty 2>/dev/null && exec 3<&-; }

ask() {
  local prompt="$1" default="$2" reply
  if [ "$ASSUME_YES" = 1 ] || ! have_tty; then
    printf '    %s %s(%s)%s\n' "$prompt" "$DIM" "$default" "$R"
    [ "$default" = y ]
    return
  fi
  read -r -p "    $prompt [$( [ "$default" = y ] && echo 'Y/n' || echo 'y/N' )] " reply </dev/tty
  reply="${reply:-$default}"
  [[ "$reply" =~ ^[Yy] ]]
}

usage() {
  cat <<'EOF'
Usage: install.sh [options]

  --yes                Take every default; never prompt.
  --dir PATH           Where to clone and build (default: ~/.local/share/tela-zen).
  --branch NAME        Branch to clone (default: main).
  --no-deps            Do not install dependencies with pacman.
  --default-browser    Make Tela the default browser afterwards.
  --run                Launch Tela afterwards.
  -h, --help           This.
EOF
}

# ── arguments, validated before anything is touched ─────────────────────────

while [ $# -gt 0 ]; do
  case "$1" in
    --yes|-y)          ASSUME_YES=1 ;;
    --dir)             [ $# -ge 2 ] || die "--dir needs a path"; SRC_DIR="$2"; shift ;;
    --branch)          [ $# -ge 2 ] || die "--branch needs a name"; BRANCH="$2"; shift ;;
    --no-deps)         SKIP_DEPS=1 ;;
    --default-browser) SET_DEFAULT=1 ;;
    --run)             LAUNCH=1 ;;
    -h|--help)         usage; exit 0 ;;
    --default)         die "unknown option: --default (did you mean --default-browser?)" ;;
    --deps)            die "unknown option: --deps (dependencies install by default; --no-deps skips them)" ;;
    *)                 usage >&2; die "unknown option: $1" ;;
  esac
  shift
done

# ── guards ──────────────────────────────────────────────────────────────────

[ "$(id -u)" != 0 ] || die "do not run this as root — makepkg refuses to build as root, and it calls sudo itself where it needs to."

command -v pacman >/dev/null 2>&1 || die "this installer builds an Arch package and needs pacman.
    On another distribution, install Qt 6 (base, declarative, webengine, webchannel)
    and a Rust toolchain, then: cargo build --release --bin tela"

command -v sudo >/dev/null 2>&1 || die "sudo is needed to install the package."

curl -fsS --head --max-time 10 https://github.com >/dev/null 2>&1 \
  || die "no route to github.com — check the network first."

# ── are we already in a checkout? ───────────────────────────────────────────

SELF_DIR=""
if [ -n "${BASH_SOURCE[0]:-}" ] && [ -f "${BASH_SOURCE[0]}" ]; then
  SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fi

if [ -n "$SELF_DIR" ] && [ -f "$SELF_DIR/packaging/PKGBUILD" ]; then
  SRC_DIR="$SELF_DIR"
  CLONE=0
  step "Building the checkout at $SRC_DIR"
else
  CLONE=1
fi

# ── dependencies ────────────────────────────────────────────────────────────

# `cargo` as a package name resolves to rustup, which installs with NO toolchain
# — so `cargo build` then dies with "no default toolchain". Ask for `rust` by
# name and the problem cannot arise.
DEPS=(
  git base-devel
  rust clang
  qt6-base qt6-declarative qt6-webengine qt6-webchannel
  ttf-material-symbols-variable
)

if [ "$SKIP_DEPS" = 1 ]; then
  step "Skipping dependencies (--no-deps)"
else
  step "Dependencies"
  missing=()
  for p in "${DEPS[@]}"; do
    pacman -T "$p" >/dev/null 2>&1 || missing+=("$p")
  done
  if [ ${#missing[@]} -eq 0 ]; then
    info "already satisfied"
  else
    info "installing: ${missing[*]}"
    # Prompt for the password now rather than twenty minutes into a build.
    sudo -v
    sudo pacman -S --needed --noconfirm "${missing[@]}"
  fi
fi

# A pre-existing rustup with no toolchain selected builds nothing, and the error
# it gives says nothing about how to fix it.
if ! cargo --version >/dev/null 2>&1; then
  if command -v rustup >/dev/null 2>&1; then
    step "Rust toolchain"
    info "rustup has no default toolchain; installing stable"
    rustup default stable
  else
    die "cargo is not usable and rustup is not installed."
  fi
fi

# ── source ──────────────────────────────────────────────────────────────────

if [ "$CLONE" = 1 ]; then
  if [ -d "$SRC_DIR/.git" ]; then
    step "Updating $SRC_DIR"
    git -C "$SRC_DIR" fetch --quiet origin "$BRANCH"
    if [ -n "$(git -C "$SRC_DIR" status --porcelain)" ]; then
      note "local changes present — leaving them alone, building what is there"
    else
      git -C "$SRC_DIR" checkout --quiet "$BRANCH"
      git -C "$SRC_DIR" merge --quiet --ff-only "origin/$BRANCH"
      info "at $(git -C "$SRC_DIR" rev-parse --short HEAD)"
    fi
  else
    step "Cloning into $SRC_DIR"
    [ ! -e "$SRC_DIR" ] || die "$SRC_DIR exists and is not a git checkout. Move it, or pass --dir."
    mkdir -p "$(dirname "$SRC_DIR")"
    git clone --quiet --branch "$BRANCH" "$REPO_URL" "$SRC_DIR"
    info "at $(git -C "$SRC_DIR" rev-parse --short HEAD)"
  fi
fi

# ── build and install ───────────────────────────────────────────────────────

step "Building the package"
note "first build compiles Qt bindings and takes a while; later ones reuse the cache"
cd "$SRC_DIR/packaging"
# -f overwrite a previous build, -s pull any dep still missing, -i install.
makepkg -fsi --noconfirm

# ── verify, rather than assume ──────────────────────────────────────────────

step "Verifying"
pacman -Q tela >/dev/null 2>&1 || die "pacman does not report tela as installed — the build reported success but installed nothing."
command -v tela >/dev/null 2>&1 || die "tela is installed but not on PATH."
info "$(pacman -Q tela)"
info "binary: $(command -v tela)"

step "Refreshing the desktop database"
if command -v update-desktop-database >/dev/null 2>&1; then
  sudo update-desktop-database /usr/share/applications 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  sudo gtk-update-icon-cache -f -t /usr/share/icons/hicolor 2>/dev/null || true
fi
info "it will appear in your launcher as Tela"

# ── afterwards ──────────────────────────────────────────────────────────────

if [ "$SET_DEFAULT" = 1 ] || ask "Make Tela the default browser?" n; then
  if command -v xdg-settings >/dev/null 2>&1; then
    xdg-settings set default-web-browser tela.desktop && info "set as default browser"
  else
    note "xdg-settings not found; skipped"
  fi
fi

printf '\n%sTela is installed.%s  Source: %s\n' "$GREEN" "$R" "$SRC_DIR"
printf '    Re-run this script to update.  Ctrl+K opens the command palette.\n\n'

if [ "$LAUNCH" = 1 ] || ask "Launch it now?" n; then
  exec tela
fi
