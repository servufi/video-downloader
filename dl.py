import os
import re
import subprocess
import sys
import readline
from concurrent.futures import ThreadPoolExecutor, as_completed
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
        self.inputs = Tuple
        self.prev_flush_len = 1
        if not os.access(self.save_dir, os.W_OK):
            print(usage())
            print(f"Error: Output folder not writable: {self.save_dir}")
            print(r"Did you bind it with '-v ${pwd}:/dl' ?")
            print(r"Permissions ?")
            print(r"On Windows paths use backslashes C:\dl:/dl ?")
            sys.exit(1)


    def parse_inputs(self, inputs: List[str]) -> bool:
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
                return 0
            i += 1  # move to next input

        self.inputs = urls, sizes, factors
        return 1


    def validate_url(self, url: str) -> bool:
        url_regex = re.compile(r'^https?://.*$')
        return re.match(url_regex, url) is not None


    def validate_size(self, size: str) -> bool:
        # regex pattern for size validation
        pattern = re.compile(r'^\d+(\.\d+)?[MmGgKk]$')
        return bool(pattern.match(size))


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


    def start(self):
        urls, sizes, factors = self.inputs
        with ThreadPoolExecutor() as executor:
            futures = {executor.submit(self.download_file, url, sizes[url], factors[url]): url for url in urls}
            for future in as_completed(futures):
                url = futures[future]
                try:
                    future.result()
                except Exception as exc:
                    print(f'{url} generated exception: {exc}')


    def printrow(self, msg: str):
        sys.stdout.write(f"\r{' ' * self.prev_flush_len}\r")
        sys.stdout.write(f"{msg}")
        sys.stdout.flush()
        self.prev_flush_len = len(msg)


    def download_file(self, url: str, filesize: str = "None", twofactor: str = "None"):
        ytcmd = ["yt-dlp", "--remux", "mp4", "--merge", "mp4"]

        if os.path.isfile(f"{self.save_dir}/.netrc"):
            ytcmd.extend(["--netrc-location", f"{self.save_dir}/.netrc"])
            if twofactor != "None":
                ytcmd.extend(["--twofactor", twofactor])

        # Download metadata for filename
        filename_process = subprocess.Popen(["yt-dlp", "--skip-download", "--print", "filename", "-o", "%(title)s", url, "--restrict-filenames"], stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        while True:
            output = filename_process.stdout.readline()
            if filename_process.poll() is not None and output == b'':
                break
            if output:
                filename = output.strip().decode('utf-8').strip()

        if not filename or ' ' in filename:
            print(f"[ISP blocked?] {url}", flush=True)
            return

        filename = filename[:120]
        outputpath = f"{self.save_dir}/{filename}.mp4"
        ytcmd.extend(["--output", outputpath, url])

        self.printrow(f"[Downloading] {url} -> {outputpath}")
        download_process = subprocess.Popen(ytcmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        while True:
            output = download_process.stdout.readline()
            if download_process.poll() is not None and output == b'':
                break

        if filesize != "None" and os.path.isfile(outputpath):
            self.reencode_file(outputpath, filesize)


    def reencode_file(self, input_file: str, filesize: str) -> None:
        output_file = f"{self.save_dir}/{os.path.basename(input_file).split('.')[0]}.{filesize}.mp4"
        target_size_bits = self.convert_to_bits(filesize)

        # Get video duration in seconds
        duration = float(subprocess.check_output(
            ["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1", input_file]
        ).decode().strip())
        if duration == "N/A":
            self.printrow("Done. Duration unknown, encoding skipped.")
            return

        # Get current audio bitrate
        audio_bitrate = subprocess.check_output(
            ["ffprobe", "-v", "error", "-select_streams", "a:0", "-show_entries", "stream=bit_rate", "-of", "default=noprint_wrappers=1:nokey=1", input_file]
        ).decode().strip()
        audio_bitrate = int(audio_bitrate) if audio_bitrate != "N/A" else 192000

        # Check current file size
        video_size_bits = os.path.getsize(input_file) * 8
        if video_size_bits <= target_size_bits:
            self.printrow("Done. File already meets the target size.")
            return

        # Calculate total bitrate needed
        container_overhead = 0.05
        container_bits = target_size_bits * container_overhead
        total_bitrate = (target_size_bits-container_bits) // duration

        # Set minimum video bitrate
        minimum_video_bitrate = 1000
        # Define possible audio bitrates
        audio_bitrates = [320000, 256000, 192000]

        selected_audio_bitrate = audio_bitrate
        selected_video_bitrate = total_bitrate - audio_bitrate

        for audio_br in [audio_bitrate] + audio_bitrates:
            video_bitrate = total_bitrate - audio_br

            if video_bitrate >= minimum_video_bitrate:
                selected_audio_bitrate = audio_br
                selected_video_bitrate = video_bitrate
                break

        # If selected video bitrate is below minimum, use real minimum video bitrate
        if selected_video_bitrate < minimum_video_bitrate:
            selected_video_bitrate = minimum_video_bitrate

        # Expected output size
        expected_output_size_bits = (selected_video_bitrate + selected_audio_bitrate) * duration
        if expected_output_size_bits > video_size_bits:
            self.printrow(f"Done. Expected encoded size exceeds input file size, skip.")
            return

        process = subprocess.Popen([
            "ffmpeg", "-y", "-i", input_file,
            "-b:v", str(selected_video_bitrate),
            "-maxrate:v", str(selected_video_bitrate),
            "-bufsize:v", str(target_size_bits // 20),
            "-b:a", str(selected_audio_bitrate),
            "-progress", "pipe:1",
            output_file
        ], stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

        while True:
            output = process.stdout.readline()
            if process.poll() is not None and output == b'':
                break
            if output:
                progress_info = output.strip().decode('utf-8').strip()
                if "frame=" in progress_info:
                    self.printrow(f"{progress_info}")

        # Verify new file size
        if os.path.exists(output_file):
            output_size_bits = os.path.getsize(output_file) * 8
            if output_size_bits <= target_size_bits:
                self.printrow("Done. Output size meets the target size.")
            else:
                self.printrow(f"Done. Output size not fully met. ({int((output_size_bits-target_size_bits)/8)} bytes over target)")

            if output_size_bits < video_size_bits:
                os.remove(input_file)
                os.rename(output_file, input_file)
            else:
                os.remove(output_file)
            return



def main() -> None:
    downloader = Downloader()

    if len(sys.argv) > 1:
        inputs = sys.argv[1:]
        downloader.parse_inputs(inputs)
        downloader.start()
    else:
        print(usage())
        print("Enter URLs:")
        while True:
            try:
                input_urls = input("\n> ").strip()
                if input_urls.lower() in ["exit", "quit"]:
                    print("\nBye!")
                    break
                inputs = [re.sub(r"^['\"]|['\"]$", '', part) for part in input_urls.split()]
                if downloader.parse_inputs(inputs):
                    downloader.start()
            except (KeyboardInterrupt, EOFError):
                print("\nBye!")
                break

if __name__ == "__main__":
    main()
