#!/bin/sh
# unmouse installer: downloads a release tarball, verifies it and installs the binary.
# Usage: curl -fsSL https://raw.githubusercontent.com/Asugawara/unmouse/main/install.sh | sh
# It then runs "unmouse install", which registers the login item (this triggers the Accessibility prompt).
# env: UNMOUSE_VERSION        version to install (default: the latest release)
#      UNMOUSE_BIN_DIR        where to put the binary (default: $HOME/.local/bin)
#      UNMOUSE_BASE_URL       releases URL (default: https://github.com/Asugawara/unmouse/releases)
#      UNMOUSE_NO_SERVICE=1   only install the binary; skip "unmouse install"
set -eu

BASE=${UNMOUSE_BASE_URL:-https://github.com/Asugawara/unmouse/releases}
BIN_DIR=${UNMOUSE_BIN_DIR:-$HOME/.local/bin}
V=${UNMOUSE_VERSION:-}
TMP=""
BIN_TMP=""

say() { echo "unmouse-install: $*" >&2; }
err() { echo "unmouse-install: error: $*" >&2; }

cleanup() {
  [ -z "$TMP" ] || rm -rf "$TMP"
  [ -z "$BIN_TMP" ] || rm -f "$BIN_TMP"
}

need() {
  for c in "$@"; do
    command -v "$c" >/dev/null 2>&1 || { err "required command not found: $c"; exit 1; }
  done
}

fetch() {
  url=$1 out=$2
  case $url in
    https://*) curl -fsSL --proto '=https' --proto-redir '=https' --connect-timeout 15 --retry 3 -o "$out" "$url" ;;
    *) curl -fsSL --connect-timeout 15 --retry 3 -o "$out" "$url" ;;
  esac
}

resolve_version() {
  V=${V#v}
  [ -z "$V" ] || return 0
  fetch "$BASE/latest/download/SHA256SUMS" "$TMP/latest-sums" 2>/dev/null \
    || { err "no published release found at $BASE (or no network)"; exit 1; }
  V=$(sed -n 's/.*unmouse-\([0-9][0-9A-Za-z.+-]*\)-macos-universal\.tar\.gz.*/\1/p' "$TMP/latest-sums" | head -n 1)
  [ -n "$V" ] || { err "no published release found at $BASE (or no network)"; exit 1; }
}

installed_version() {
  # "unmouse 0.1.0" -> "0.1.0"
  v=$("$BIN_DIR/unmouse" --version 2>/dev/null || true)
  echo "${v#unmouse }"
}

download() {
  tarball=unmouse-$V-macos-universal.tar.gz
  say "downloading unmouse $V"
  fetch "$BASE/download/v$V/$tarball" "$TMP/$tarball" \
    || { err "download failed: $BASE/download/v$V/$tarball"; exit 1; }
  fetch "$BASE/download/v$V/SHA256SUMS" "$TMP/SHA256SUMS" \
    || { err "download failed: $BASE/download/v$V/SHA256SUMS"; exit 1; }
  want=$(awk -v f="$tarball" '$2 == f || $2 == "*" f { print $1; exit }' "$TMP/SHA256SUMS")
  got=$(shasum -a 256 "$TMP/$tarball" | cut -d ' ' -f 1)
  if [ -z "$want" ] || [ "$want" != "$got" ]; then
    err "checksum mismatch for $tarball; aborting"
    exit 1
  fi
  mkdir "$TMP/extract"
  tar -xzf "$TMP/$tarball" -C "$TMP/extract"
  [ -f "$TMP/extract/unmouse-$V/unmouse" ] || { err "$tarball has no unmouse-$V/unmouse"; exit 1; }
}

# copy to a temp name in the bin dir, then rename: a running or half-written binary is never visible at the final path
install_binary() {
  mkdir -p "$BIN_DIR"
  BIN_TMP=$BIN_DIR/.unmouse.$$
  cp "$TMP/extract/unmouse-$V/unmouse" "$BIN_TMP"
  chmod 755 "$BIN_TMP"
  mv -f "$BIN_TMP" "$BIN_DIR/unmouse"
  BIN_TMP=""
  say "unmouse $V installed: $BIN_DIR/unmouse"
}

path_hint() {
  case ":$PATH:" in
    *":$BIN_DIR:"*) return 0 ;;
  esac
  say "$BIN_DIR is not on your PATH. Add it:"
  echo "  bash/zsh: export PATH=\"$BIN_DIR:\$PATH\"" >&2
  echo "  fish:     fish_add_path $BIN_DIR" >&2
}

main() {
  [ "$(uname -s)" = Darwin ] || { err "unmouse runs on macOS only"; exit 1; }
  need curl tar mkdir cp mv rm chmod mktemp shasum awk sed
  TMP=$(mktemp -d)
  trap cleanup EXIT
  trap 'exit 1' INT TERM
  resolve_version
  if [ "$(installed_version)" = "$V" ]; then
    say "unmouse $V is already installed: $BIN_DIR/unmouse"
  else
    download
    install_binary
  fi
  if [ "${UNMOUSE_NO_SERVICE:-}" != 1 ]; then
    "$BIN_DIR/unmouse" install
  fi
  path_hint
}

main "$@"
