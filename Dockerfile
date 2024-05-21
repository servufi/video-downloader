FROM alpine:latest
WORKDIR /app
COPY --chown=1000:1000 ./dl.py .
ENV TERM=xterm \
    SAVEDIR=/dl
RUN mkdir -p /.cache && \
    chown 1000:1000 /.cache && \
    apk add --no-cache python3 py3-pip ca-certificates wget ffmpeg && \
    wget -O /usr/bin/yt-dlp https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp && \
    chown 1000:1000 /usr/bin/yt-dlp && \
    chmod u+rwx /usr/bin/yt-dlp && \
    ln -s /usr/bin/yt-dlp /usr/bin/youtube-dl && \
    python3 -m venv /app/venv && \
    /app/venv/bin/pip install yt-dlp && \
    rm -rf /var/cache/apk/*
ENV PATH="/app/venv/bin:$PATH"
USER 1000
ENTRYPOINT ["python3", "dl.py"]
