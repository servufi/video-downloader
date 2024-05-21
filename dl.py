import os
import re
import subprocess
import sys
from typing import List, Tuple, Dict

def usage() -> str:
    return r"""
                                  _____.__
  ______ ______________  ____ ___/ ____\__|
 /  ___// __ \_  __ \  \/ /  |  \   __\|  |
 \___ \\  ___/|  | \/\   /|  |  /|  |  |  |
/____  >\___  >__|    \_/ |____/ |__|  |__|
     \/     \/
      servufi/video-downloader
  Powered by: yt-dlp, ffmpeg, docker.. :)

Tip:
  - docker run --rm -v /home/user/dl:/dl servufi/video-downloader url1 url2 9.5M url3

Size formats: 5000K / 5.6M / 1G
Input format: <URL> (size) ...

"""

class Downloader:
    def __init__(self, save_dir: str = '/dl'):
        self.save_dir = save_dir
        if not os.access('/dl', os.W_OK):
            print(usage())
            print("Error: Output folder not writable:")
            print("Did you bind it with '-v ${pwd}:/dl' ?")
            print("Permissions ?")
            print("Windows users use backslashes C:\dl:/dl ?")
            sys.exit(1)

    def validate_url(self, url: str) -> bool:
        url_regex = re.compile(r'^https?://.*$')
        return re.match(url_regex, url) is not None

    def convert_to_bits(self, size: str) -> int:
        size = size.upper()
        unit = ''.join(filter(str.isalpha, size))
        value = float(''.join(filter(lambda x: x.isdigit() or x == '.', size)))

        if unit in ['M', 'MB']:
            return int(value * 1000 * 1000 * 8)
        elif unit in ['G', 'GB']:
            return int(value * 1000 * 1000 * 1000 * 8)
        elif unit in ['K', 'KB']:
            return int(value * 1000 * 8)
        else:
            return 0

    def download_file(self, url: str, filesize: str = "None", twofactor: str = "None") -> None:
        if not self.validate_url(url):
            print(f"Invalid URL: {url}")
            return

        ytcmd = ["yt-dlp", "--remux", "mp4", "--merge", "mp4"]

        if filesize != "None":
            ytcmd.extend(["--format-sort", f"filesize:{filesize}"])

        if os.path.isfile(f"{self.save_dir}/.netrc"):
            ytcmd.extend(["--netrc-location", f"{self.save_dir}/.netrc"])
            if twofactor != "None":
                ytcmd.extend(["--twofactor", twofactor])

        filename = subprocess.check_output(
            ["yt-dlp", "--skip-download", "--print", "filename", "-o", "%(title)s", url, "--restrict-filenames"]
        ).decode().strip()

        if not filename or ' ' in filename:
            print("403 Blocked?")
            return

        filename = filename[:120]
        outputpath = f"{self.save_dir}/{filename}.mp4"
        ytcmd.extend(["--output", outputpath, url])

        print("Downloading file...")
        subprocess.run(ytcmd)

        if os.path.isfile(outputpath):
            print(f"Download completed. [{url}]")

        if filesize != "None" and os.path.isfile(outputpath):
            self.reencode_file(outputpath, filesize)

    def reencode_file(self, input_file: str, filesize: str) -> None:
        output_file = f"{self.save_dir}/{os.path.basename(input_file).split('.')[0]}.{filesize}.mp4"

        video_size = os.path.getsize(input_file)
        video_size_bits = video_size * 8
        audio_size = subprocess.check_output(
            ["ffprobe", "-v", "error", "-select_streams", "a:0", "-show_entries", "stream=bit_rate", "-of", "default=noprint_wrappers=1:nokey=1", input_file]
        ).decode().strip()
        audio_bitrates = [320, 256, 192]
        minimum_bitrate = 192000
        if audio_size == "N/A":
            audio_size = minimum_bitrate

        nearest_audio_bitrate = min(audio_bitrates, key=lambda x: abs(x - int(audio_size) // 1000))
        target_size = self.convert_to_bits(filesize)

        if target_size != 0:
            length = float(subprocess.check_output(
                ["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1", input_file]
            ).decode().strip())
            length_round_up = int(length) + 1
            total_bitrate = target_size // length_round_up
            audio_bitrate = nearest_audio_bitrate * 1000
            video_bitrate = total_bitrate - audio_bitrate
            if video_bitrate < 1000:
                video_bitrate = 1000

            if video_size_bits <= target_size:
                print("Input file is already within target size.")
            else:
                subprocess.run(["ffmpeg", "-y", "-i", input_file, "-b:v", str(video_bitrate), "-maxrate:v", str(video_bitrate), "-bufsize:v", str(target_size // 20), "-b:a", str(audio_bitrate), output_file])
                input_size = os.path.getsize(input_file)
                output_size = os.path.getsize(output_file)

                if output_size > input_size or output_size == 0:
                    print("Conversion failed to reach the target file size. Keeping original file.")
                    os.remove(output_file)
                else:
                    print("Conversion successful. Deleting original file.")
                    os.remove(input_file)
                    os.rename(output_file, input_file)
        else:
            print("Encoding skipped. Valid filesize formats are <size>k/m/g ex. 13M")

    def validate_size(self, size: str) -> bool:
        # regex pattern for size validation
        pattern = re.compile(r'^\d+(\.\d+)?[MmGgKk]$')
        return bool(pattern.match(size))

    def parse_inputs(self, inputs: List[str]) -> Tuple[List[str], Dict[str, str], Dict[str, str]]:
        urls = []  # urls
        sizes = {}  # size wishes
        factors = {}  # optional 2fa codes (untested)
        i = 0

        while i < len(inputs):
            if self.validate_url(inputs[i]):  # if valid url
                url = inputs[i]
                size = "None"  # default size
                factor = "None"  # default 2fa code (untested)

                # if next input is not url and valid size, its size
                if i + 1 < len(inputs) and not self.validate_url(inputs[i + 1]) and self.validate_size(inputs[i + 1]):
                    size = inputs[i + 1]
                    i += 1  # move to next input

                # if next input after size or size wasn't size and isn't url and .netrc exists, its probably 2fa code (untested)
                if i + 1 < len(inputs) and os.path.isfile(f"{self.save_dir}/.netrc") and not self.validate_url(inputs[i + 1]):
                    factor = inputs[i + 1]
                    i += 1  # move to next input

                # add entry
                urls.append(url)
                sizes[url] = size
                factors[url] = factor
            else:
                print(usage())
                print(f"Invalid URL: {inputs[i]}")
                sys.exit(1)
            i += 1  # move to next input
        return urls, sizes, factors

def main() -> None:
    downloader = Downloader()

    if len(sys.argv) > 1:
        inputs = sys.argv[1:]
        urls, sizes, factors = downloader.parse_inputs(inputs)
        for url in urls:
            downloader.download_file(url, sizes[url], factors[url])
    else:
        print(usage())
        print("Enter URLs:")
        while True:
            try:
                input_urls = input("> ").strip()
                if input_urls.lower() in ["exit", "quit"]:
                    break
                inputs = input_urls.split()
                urls, sizes, factors = downloader.parse_inputs(inputs)
                for url in urls:
                    downloader.download_file(url, sizes[url], factors[url])
            except (KeyboardInterrupt, EOFError):
                print("\nBye!")
                break

if __name__ == "__main__":
    main()
