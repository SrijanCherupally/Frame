#!/usr/bin/env bash
# Build the offline Windows media tools from pinned upstream sources.
# Requires Git, make, NASM, pkg-config and an x86_64-w64-mingw32 GCC/binutils toolchain.
set -euo pipefail
MEDIA_ROOT="${1:-/tmp/frame-media-build}"
MEDIA_JOBS="${FRAME_MEDIA_JOBS:-6}"
CROSS_PREFIX="${FRAME_CROSS_PREFIX:-x86_64-w64-mingw32-}"
mkdir -p "$MEDIA_ROOT"
MEDIA_ROOT="$(cd "$MEDIA_ROOT" && pwd)"
MEDIA_PREFIX="$MEDIA_ROOT/prefix"
mkdir -p "$MEDIA_PREFIX"
checkout_source() {
    local directory="$1" repository="$2" revision="$3"
    if [[ ! -d "$directory/.git" && -f "$directory/.frame-source-revision" ]]; then
        [[ "$(cat "$directory/.frame-source-revision")" == "$revision" ]] || { echo "Wrong offline source revision: $directory" >&2; exit 1; }
        return
    fi
    if [[ ! -d "$directory/.git" ]]; then
        git init "$directory"
        git -C "$directory" remote add origin "$repository"
        git -C "$directory" fetch --depth 1 origin "$revision"
        git -C "$directory" checkout --detach FETCH_HEAD
    fi
    [[ "$(git -C "$directory" rev-parse HEAD)" == "$revision" ]] || {
        echo "Wrong source revision in $directory; use a fresh build folder." >&2; exit 1;
    }
}
checkout_source "$MEDIA_ROOT/ffmpeg" https://github.com/FFmpeg/FFmpeg.git 3a0867c2bfda4a4d4309ca1a8cbdc6175e67f587
checkout_source "$MEDIA_ROOT/x264" https://github.com/mirror/x264.git c24e06c2e184345ceb33eb20a15d1024d9fd3497
checkout_source "$MEDIA_ROOT/SDL" https://github.com/libsdl-org/SDL.git 8acf7cfc0f88717e5c036f60f0314bfcafe39189
(
    cd "$MEDIA_ROOT/x264"
    ./configure --host=x86_64-w64-mingw32 --cross-prefix="$CROSS_PREFIX" --prefix="$MEDIA_PREFIX" --enable-static --disable-cli --enable-win32thread
    make -j"$MEDIA_JOBS"
    make install
)
mkdir -p "$MEDIA_ROOT/build-sdl"
(
    cd "$MEDIA_ROOT/build-sdl"
    ../SDL/configure --host=x86_64-w64-mingw32 --prefix="$MEDIA_PREFIX" --enable-static --disable-shared --disable-hidapi --disable-video-vulkan --disable-video-opengl
    make -j"$MEDIA_JOBS"
    make install
)
(
    cd "$MEDIA_ROOT/ffmpeg"
    PKG_CONFIG_LIBDIR="$MEDIA_PREFIX/lib/pkgconfig" ./configure \
        --target-os=mingw32 --arch=x86_64 --cross-prefix="$CROSS_PREFIX" \
        --prefix="$MEDIA_PREFIX" --pkg-config=pkg-config --pkg-config-flags=--static \
        --enable-gpl --enable-libx264 --enable-sdl2 --enable-static --disable-shared \
        --disable-debug --disable-doc --disable-autodetect --disable-network \
        --disable-hwaccels --disable-devices --enable-indev=lavfi \
        --enable-w32threads --disable-pthreads --extra-ldflags=-static
    make -j"$MEDIA_JOBS"
    make install
)
printf 'Media tools built in %s/bin\n' "$MEDIA_PREFIX"
