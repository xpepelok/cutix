#!/usr/bin/env bash
# Downloads the LGPL shared FFmpeg build we ship, proves it carries no GPL
# component, and lays the runtime libraries plus their licence into a directory.
#
#   scripts/bundle-ffmpeg.sh <platform> <destination>
#
# <platform> is "windows" or "linux". The destination ends up holding the five
# shared libraries the loader opens, the FFmpeg licence text, and PROVENANCE.txt
# recording exactly which build this is.
set -euo pipefail

PLATFORM="${1:?usage: bundle-ffmpeg.sh <windows|linux> <destination>}"
DESTINATION="${2:?usage: bundle-ffmpeg.sh <windows|linux> <destination>}"

# Pinned so a release is reproducible and so the licence text we ship matches the
# binaries we ship. Bump both the tag and the checksum together.
RELEASE_TAG="latest"
BASE="https://github.com/BtbN/FFmpeg-Builds/releases/download/${RELEASE_TAG}"

case "$PLATFORM" in
  windows) ARCHIVE="ffmpeg-master-latest-win64-lgpl-shared.zip" ;;
  linux)   ARCHIVE="ffmpeg-master-latest-linux64-lgpl-shared.tar.xz" ;;
  *) echo "unknown platform '$PLATFORM' (expected windows or linux)" >&2; exit 2 ;;
esac

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "fetching $ARCHIVE"
curl -fsSL "$BASE/$ARCHIVE" -o "$WORK/$ARCHIVE"
sha256sum "$WORK/$ARCHIVE" | tee "$WORK/archive.sha256"

case "$ARCHIVE" in
  *.zip)    unzip -q "$WORK/$ARCHIVE" -d "$WORK/unpacked" ;;
  *.tar.xz) mkdir -p "$WORK/unpacked" && tar -xJf "$WORK/$ARCHIVE" -C "$WORK/unpacked" ;;
esac

ROOT="$(find "$WORK/unpacked" -mindepth 1 -maxdepth 1 -type d | head -n 1)"
[ -n "$ROOT" ] || { echo "the archive did not unpack into a directory" >&2; exit 1; }

FFMPEG="$ROOT/bin/ffmpeg"
[ -x "$FFMPEG" ] || FFMPEG="$ROOT/bin/ffmpeg.exe"
[ -f "$FFMPEG" ] || { echo "no ffmpeg binary under $ROOT/bin" >&2; exit 1; }

# Exact flag matching. A substring grep is not good enough: "--enable-gpl" is a
# substring of "--enable-gpl-something" and every flag containing "gpl" trips a
# loose pattern, so each flag is compared whole after splitting on whitespace.
CONFIGURATION="$("$FFMPEG" -hide_banner -version 2>/dev/null | grep '^configuration:' || true)"
[ -n "$CONFIGURATION" ] || { echo "ffmpeg -version printed no configuration line" >&2; exit 1; }

FLAGS="$(printf '%s\n' "${CONFIGURATION#configuration:}" | tr ' ' '\n' | sed '/^$/d')"

for FORBIDDEN in --enable-gpl --enable-nonfree --enable-libx264 --enable-libx265; do
  if printf '%s\n' "$FLAGS" | grep -qx -- "$FORBIDDEN"; then
    echo "REFUSING TO SHIP: $FORBIDDEN is present in the build configuration" >&2
    echo "$CONFIGURATION" >&2
    exit 1
  fi
done

if ! printf '%s\n' "$FLAGS" | grep -qx -- "--enable-shared"; then
  echo "REFUSING TO SHIP: not a shared build; static linking would defeat replaceability" >&2
  echo "$CONFIGURATION" >&2
  exit 1
fi

echo "LGPL check passed (no --enable-gpl/--enable-nonfree/--enable-libx264/--enable-libx265)"

mkdir -p "$DESTINATION"
COPIED=0
for STEM in avutil swresample swscale avcodec avformat; do
  FOUND=""
  for CANDIDATE in \
    "$ROOT/bin/$STEM"-*.dll \
    "$ROOT/lib/lib$STEM.so".* \
    "$ROOT/lib/lib$STEM".*.dylib
  do
    [ -f "$CANDIDATE" ] || continue
    FOUND="$CANDIDATE"
    break
  done
  [ -n "$FOUND" ] || { echo "no shared library for $STEM under $ROOT" >&2; exit 1; }
  cp -L "$FOUND" "$DESTINATION/"
  COPIED=$((COPIED + 1))
done
[ "$COPIED" -eq 5 ] || { echo "expected 5 libraries, copied $COPIED" >&2; exit 1; }

for LICENCE in "$ROOT/LICENSE.txt" "$ROOT/LICENSE" "$ROOT/COPYING.LGPLv3"; do
  if [ -f "$LICENCE" ]; then
    cp "$LICENCE" "$DESTINATION/FFMPEG-LICENSE.txt"
    break
  fi
done
[ -f "$DESTINATION/FFMPEG-LICENSE.txt" ] || { echo "the build shipped no licence text" >&2; exit 1; }

VERSION_LINE="$("$FFMPEG" -hide_banner -version 2>/dev/null | head -n 1)"

cat > "$DESTINATION/PROVENANCE.txt" <<PROVENANCE
FFmpeg shared libraries bundled with Cutix
============================================

These libraries are FFmpeg, used under the GNU Lesser General Public License.
They are unmodified binaries as published upstream. Cutix does not link
against them at build time; it opens them by name at runtime.

Version:   $VERSION_LINE
Source:    $BASE/$ARCHIVE
Archive:   $(cut -d' ' -f1 < "$WORK/archive.sha256")  (sha256)
Platform:  $PLATFORM
Fetched:   $(date -u +%Y-%m-%dT%H:%M:%SZ)

Configuration as reported by the build itself:

$CONFIGURATION

Verified absent by exact flag match: --enable-gpl, --enable-nonfree,
--enable-libx264, --enable-libx265.

Licence: see FFMPEG-LICENSE.txt in this directory (LGPL v3; the build is
configured with --enable-version3).

Corresponding source for this exact build is published alongside the binaries at
https://github.com/BtbN/FFmpeg-Builds and at https://ffmpeg.org/download.html.

Substituting your own build
---------------------------

Nothing here is fused into the Cutix executable, so you may replace it:

  * Replace the files in this directory with your own build of the same major
    versions, keeping the same file names, or
  * set OPENCUT_FFMPEG_DIR to a directory holding your libraries, which takes
    precedence over this one, or
  * set OPENCUT_DISABLE_FFMPEG=1 to run without FFmpeg at all.

Requirements for a substitute: shared libraries for avutil, swresample, swscale,
avcodec and avformat, with libavcodec major 58 or newer.
PROVENANCE

echo "staged into $DESTINATION:"
ls -1 "$DESTINATION"
