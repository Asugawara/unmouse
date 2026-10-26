#!/bin/sh
# Build the universal macOS binary and pack unmouse-<version>-macos-universal.tar.gz + SHA256SUMS into <outdir>.
# Usage: sh scripts/build-release.sh <version> <outdir>
# Needs the targets aarch64-apple-darwin and x86_64-apple-darwin (rustup target add ...).
set -eu

usage() {
  echo "usage: sh scripts/build-release.sh <version> <outdir>" >&2
  exit 2
}

main() {
  [ $# -eq 2 ] || usage
  [ "$(uname -s)" = Darwin ] || { echo "build-release: macOS only (lipo and codesign are needed)" >&2; exit 1; }
  V=${1#v}
  mkdir -p "$2"
  OUT=$(cd "$2" && pwd)
  ROOT=$(cd "$(dirname "$0")/.." && pwd)
  cd "$ROOT"

  # a version without a pre-release suffix is a real release: it must be the one in Cargo.toml
  case $V in
    *-*) ;;
    *)
      cargo_v=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
      [ "$V" = "$cargo_v" ] || { echo "build-release: version $V does not match Cargo.toml ($cargo_v)" >&2; exit 1; }
      ;;
  esac

  # stamps `unmouse --version`
  UNMOUSE_BUILD_VERSION=$V
  export UNMOUSE_BUILD_VERSION
  for target in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release --locked --target "$target"
  done

  name=unmouse-$V
  stage=$OUT/stage/$name
  rm -rf "$OUT/stage"
  mkdir -p "$stage"
  lipo -create -output "$stage/unmouse" \
    target/aarch64-apple-darwin/release/unmouse \
    target/x86_64-apple-darwin/release/unmouse
  codesign --force --sign - "$stage/unmouse"
  chmod 755 "$stage/unmouse"

  files="unmouse LICENSE README.md config.example.toml"
  [ ! -f README.ja.md ] || files="$files README.ja.md"
  for f in $files; do
    [ "$f" = unmouse ] || cp "$f" "$stage/$f"
  done

  epoch=${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct 2>/dev/null || true)}
  if [ -n "$epoch" ]; then
    # BSD touch: -t [[CC]YY]MMDDhhmm[.SS], in UTC so the result does not depend on the machine's time zone
    stamp=$(TZ=UTC date -r "$epoch" +%Y%m%d%H%M.%S)
    for f in $files; do TZ=UTC touch -t "$stamp" "$stage/$f"; done
    TZ=UTC touch -t "$stamp" "$stage"
  fi

  tarball=$OUT/$name-macos-universal.tar.gz
  rm -f "$tarball"
  # an explicit member list in a fixed order and gzip -n (no name or time in the header) keep the archive reproducible
  members=$OUT/stage/members.txt
  { echo "$name"; for f in $files; do echo "$name/$f"; done | sort; } > "$members"
  (cd "$OUT/stage" && COPYFILE_DISABLE=1 tar --uid 0 --gid 0 --no-mac-metadata --no-recursion -cf - -T members.txt | gzip -n > "$tarball")
  rm -f "$members"

  (cd "$OUT" && shasum -a 256 "$name-macos-universal.tar.gz" > SHA256SUMS)
  echo "$tarball"
  echo "$OUT/SHA256SUMS"
}

main "$@"
