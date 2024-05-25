# video-downloader

### Features

- Downloads audio/video URLs using [yt-dlp](https://github.com/yt-dlp/yt-dlp "https://github.com/yt-dlp/yt-dlp") (Stores files only in .mp4 format)
- Takes file size limit as argument and if it isn't already met by source URL it will attempt to shrink file with [ffmpeg](https://github.com/FFmpeg/FFmpeg "https://github.com/FFmpeg/FFmpeg"). (Video min. bitrate 1000, audio 192000-320000.)

#### Usage

- With menu:

`$ docker run --rm -it -v $(pwd):/dl servufi/video-downloader`

- Without menu:

`$ docker run --rm -v $(pwd):/dl servufi/video-downloader "<URL>"`

- Without menu and target size 13MB:

`$ docker run --rm -v $(pwd):/dl servufi/video-downloader "<URL>" 13M`

- Without menu, list URL's and some target sizes (downloads and encodes asynchronously):

`$ docker run --rm -v $(pwd):/dl servufi/video-downloader "<URL>" "<URL>" 13M "<URL>" 4M "<URL>"`

`$(pwd)` is save directory (from where command is executed). You can replace that with your own path, but `:/dl` (path within container) is must for script.
Windows users would use backslashes `-v C:\Users\user\Desktop\downloads:/dl`.

#### UNTESTED:

If URL requires login credentials, you can try your luck by creating `.netrc` -file in local bind folder `(C:\Users\user\Desktop\downloads\.netrc)` and add credentials there:

```
machine youtube login "user name" "pass word"
machine reddit login "user name" "pass word"
machine twitter login "user name" "pass word"
...
```

and if 2FA is required try to pass verification code for each URL:

`$ docker run --rm -v $(pwd):/dl servufi/video-downloader "<URL>" 13M ver!ficationc0d3`

`$ docker run --rm -v $(pwd):/dl servufi/video-downloader "<URL>" ver!ficationc0d3`
