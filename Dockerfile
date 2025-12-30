# Stage 1: ffmpeg-builder
FROM alpine:3.22 AS ffmpeg-builder
WORKDIR /build

# Install build dependencies
RUN apk add --no-cache \
  build-base \
  cmake \
  perl \
  diffutils \
  tar xz \
  git \
  yasm nasm \
  musl-dev \
  pkgconf \
  meson ninja-build cargo python3 \
  zlib-dev zlib-static \
  openssl-dev openssl-libs-static \
  lame-dev \
  x264-dev \
  dav1d-dev \
  libogg-dev libogg-static \
  libvorbis-dev libvorbis-static\
  zlib

# Build dav1d
RUN git clone --depth=1 https://code.videolan.org/videolan/dav1d.git && \
    cd dav1d && \
    meson setup build --default-library=static --buildtype=release --prefix=/usr/local && \
    ninja -C build | tee /build/david_build.log && \
    ninja -C build install

# Build libvpx
RUN git clone --depth=1 https://chromium.googlesource.com/webm/libvpx.git && \
    cd libvpx && \
    ./configure --prefix=/usr/local --disable-shared --enable-vp8 --enable-vp9 --enable-static \
     | tee /build/libvpx_configure.log && \
    make -j$(nproc) | tee /build/libvpx_make.log && \
    make install

# Build x265
RUN git clone --branch stable --depth=1 https://bitbucket.org/multicoreware/x265_git && \
    cd x265_git/build/linux && \
    cmake -G "Unix Makefiles" -DCMAKE_INSTALL_PREFIX=/usr/local -DENABLE_SHARED=OFF ../../source \
     | tee /build/x265_cmake.log && \
    make -j$(nproc) | tee /build/x265_make.log && \
    make install && \
    sed -i 's/-lssp_nonshared//g; s/-lgcc_s//g; s/-lgcc//g' /usr/local/lib/pkgconfig/x265.pc

# Clone FFmpeg and build static ffmpeg/ffprobe
RUN git clone --branch release/7.1 --depth=1 https://github.com/FFmpeg/FFmpeg.git && \
  cd FFmpeg && \
  ./configure \
    --prefix=/usr/local \
    --pkg-config-flags="--static" \
    --extra-cflags="-static" \
    --extra-ldflags="-static" \
    --enable-static \
    --disable-shared \
    --disable-debug \
    --disable-doc \
    --enable-gpl \
    --enable-libdav1d \
    --enable-libx264 \
    --enable-libx265 \
    --enable-libvpx \
    --enable-libvorbis \
    --enable-libmp3lame | tee /build/ffmpeg_configure.log && \
  make -j$(nproc) | tee /build/ffmpeg_make.log && \
  make install

# Stage 2: rust-builder
FROM rust:1.88-alpine AS rust-builder
WORKDIR /build

# Install build dependencies
RUN apk add --no-cache wget musl-dev

# Download yt-dlp binary
RUN wget -O yt-dlp https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_musllinux \
  && chmod +x yt-dlp

# Copy Cargo project
COPY Cargo.toml .
COPY .cargo ./.cargo
COPY src ./src

# Build static Rust binary
RUN cargo build --release --target x86_64-unknown-linux-musl | tee /build/main_build.log

# Prepare scratch dirs
RUN mkdir -p /tmp-root /tmp-dl

# Stage 3: Final stage
FROM scratch

# Copy built ffmpeg + ffprobe
COPY --from=ffmpeg-builder /build/FFmpeg/ffmpeg /ffmpeg
COPY --from=ffmpeg-builder /build/FFmpeg/ffprobe /ffprobe

# Copy musl runtime libs for yt-dlp_musllinux from ffmpeg-builder stage
COPY --from=ffmpeg-builder /lib/ld-musl-x86_64.so.1 /lib/
COPY --from=ffmpeg-builder /lib/libc.musl-x86_64.so.1 /lib/
COPY --from=ffmpeg-builder /usr/lib/libz.so.1 /lib/

# Copy dirs
COPY --from=rust-builder /tmp-root /tmp
COPY --from=rust-builder /tmp-dl /dl

# Copy yt-dlp
COPY --from=rust-builder /build/yt-dlp /yt-dlp

# Copy Rust binary
COPY --from=rust-builder /build/target/x86_64-unknown-linux-musl/release/video-downloader /main

# Dev
# COPY --from=ffmpeg-builder /build/*.log /
# COPY --from=rust-builder /build/*.log /

# Add binary path
ENV PATH="/:/"

ENTRYPOINT ["/main"]
