# video-downloader

Minimal mp4 downloader powered by [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) and [`ffmpeg`](https://github.com/FFmpeg/FFmpeg). Built in Rust, runs in Docker.

## Features

- Downloads video/audio using `yt-dlp` into `.mp4` format
- Optional size limit: automatically re-encodes oversized files using `ffmpeg`
  - Target size in KB/MB/GB (e.g. `5M`, `5000K`, `0.2G`)
  - Bitrate budgeted across audio/video (floors video to minimum 1000 bits, keeps decent audio) 
  - Re-encode skipped if original is already smaller
- Multi-URL batch support (via `rayon`)
- Interactive or CLI usage
- Auto-parses `/dl/urls.txt` if no arguments passed
- Auto-detects and converts `/dl/cookies.txt` if present (for authentication)
- Scratch-based container with statically built binaries (`main`, `yt-dlp`, `ffmpeg`, `ffprobe`)

## Usage

### CLI

Nushell's pwd = $"(pwd):/dl"

- Download directly:
```bash
docker run --rm -v $(pwd):/dl servufi/video-downloader https://example.com/video
```

- Download with target size:
```bash
docker run --rm -v $(pwd):/dl servufi/video-downloader https://example.com/video 9.5M
```

- Multiple URLs with sizes:

```bash
docker run --rm -v $(pwd):/dl servufi/video-downloader https://example.com/video https://example.com/videoB 9.5M https://example.com/videoC
```

### urls.txt Mode

1. Create a file in your mounted folder `/dl/urls.txt`:
```plain
https://a.com/vid1
https://b.com/vid2 4.5M
https://c.com/vid3
...
```

2. Run with no arguments, it automatically detects and processes this file:

```bash
docker run --rm -v $(pwd):/dl servufi/video-downloader
```

The file will be renamed to `/dl/urls_<epoch>.txt`.

### Interactive Mode

Launch with interactive prompt:

```bash
docker run --rm -it -v $(pwd):/dl servufi/video-downloader
```

You can type or paste:
```plain
> https://example.com/video 8M https://another.com/vid
> https://another.com
> q
```

### Cookie-Based Login Support

If the site requires login (e.g. X/Twitter), pass cookies using `/dl/cookies.txt`.

Supported input formats:

- Copy-paste from Chrome/Brave/Vivaldi DevTools (Application > Cookies > Ctrl+A > Ctrl+C)

- Copy-paste from Firefox's cookie viewer or use extension to extract cookie

- Already converted cookies.txt files like from yt-dlp CLI directly ([`More info`](https://github.com/yt-dlp/yt-dlp/wiki/FAQ#how-do-i-pass-cookies-to-yt-dlp))

At runtime, this `/dl/cookies.txt` -file should be auto-converted to Netscape format.

