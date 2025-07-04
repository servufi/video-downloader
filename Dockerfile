# Stage 1: Builder
FROM rust:1.87-alpine AS builder

WORKDIR /build

# Install dependencies
RUN apk add --no-cache wget tar xz musl-dev

# Download static yt-dlp binary
RUN wget -O yt-dlp https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux \
    && chmod +x yt-dlp

# Download static ffmpeg build
# RUN wget -O ffmpeg.tar.xz https://johnvansickle.com/ffmpeg/builds/ffmpeg-git-amd64-static.tar.xz \
#     && mkdir ffmpeg-extract \
#     && tar -xf ffmpeg.tar.xz -C ffmpeg-extract --strip-components=1
COPY ffmpeg.tar.xz ./
RUN mkdir ffmpeg-extract && \
    tar -xf ffmpeg.tar.xz -C ffmpeg-extract --strip-components=1

# Copy Cargo project
COPY Cargo.toml .
COPY .cargo ./.cargo
COPY src ./src

# Compile Rust binary statically
RUN apk add --no-cache musl-dev \
    && cargo build --release --target x86_64-unknown-linux-musl

# Prepare required folders for scratch stage
RUN mkdir -p /tmp-root /tmp-dl

# Stage 2: Minimal image
FROM scratch

# Copy created folders to scratch for yt-dlp + Rust to work
COPY --from=builder /tmp-root /tmp
COPY --from=builder /tmp-dl /dl

# Copy compiled tools
COPY --from=builder /build/yt-dlp /yt-dlp
COPY --from=builder /build/ffmpeg-extract/ffmpeg /ffmpeg
COPY --from=builder /build/ffmpeg-extract/ffprobe /ffprobe
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/video-downloader /main

# Set binary PATH and run
ENV PATH="/:/"

ENTRYPOINT ["/main"]
