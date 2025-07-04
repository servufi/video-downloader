use rayon::prelude::*;
use rustyline::{DefaultEditor, Result as RLResult};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const DEBUG: bool = false;

static ENCODE_LOCK: once_cell::sync::Lazy<Mutex<()>> =
    once_cell::sync::Lazy::new(|| Mutex::new(()));

/// Configuration for single download task
struct DownloadTask {
    url: String,
    size: Option<String>,
    twofa: Option<String>,
}

fn usage() {
    println!(
        r#"
                                  _____.__
  ______ ______________  ____ ___/ ____\__|
 /  ___// __ \_  __ \  \/ /  |  \   __\|  |
 \___ \\  ___/|  | \/\   /|  |  /|  |  |  |
/____  >\___  >__|    \_/ |____/ |__|  |__|
     \/     \/
      servufi/video-downloader
  Powered by: yt-dlp, ffmpeg, docker.. :)

Tip:
  - docker run --rm -v $(pwd):/dl servufi/video-downloader url1 url2 9.5M url3
  - or place /dl/urls.txt with lines like:
    https://example.com/video 5M 123456

Size formats: 5000K / 5.6M / 1G
Input format: <URL> (size) (2FA)
"#
    );
}

fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

fn parse_size_to_bits(input: &str) -> Option<u64> {
    let s = input.trim().to_ascii_lowercase();

    let (value, unit) = s
        .chars()
        .position(|c| !c.is_numeric() && c != '.')
        .map(|idx| (&s[..idx], &s[idx..]))
        .unwrap_or((s.as_str(), "")); // no suffix

    let val = value.parse::<f64>().ok()?; // If no number, bail

    let bits = match unit {
        "" => {
            eprintln!("[WARN] No unit, assuming bytes.");
            val * 8.0
        }
        "k" | "kb" => val * 1_000.0 * 8.0,
        "kbit" | "kbps" => val * 1_000.0,
        "m" | "mb" => val * 1_000_000.0 * 8.0,
        "mbit" | "mbps" => val * 1_000_000.0,
        "g" | "gb" => val * 1_000_000_000.0 * 8.0,
        "gbit" | "gbps" => val * 1_000_000_000.0,
        _ => {
            eprintln!("[WARN] Unknown size unit '{}', assuming bytes.", unit);
            val * 8.0
        }
    };

    Some(bits as u64)
}

fn reencode_video(input_path: &str, target_size_str: &str) {
    println!(
        "[Re-encoding] {} to target size {}",
        input_path, target_size_str
    );

    // Step 1: Parse target size string
    let target_bits = match parse_size_to_bits(target_size_str) {
        Some(bits) => bits,
        None => {
            eprintln!("[WARN] Could not parse target size '{}'", target_size_str);
            return;
        }
    };
    if DEBUG {
        println!("[DEBUG] Target bits: {}", target_bits);
    }

    // Step 2: Get video duration
    let duration_out = Command::new("/ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            input_path,
        ])
        .output();

    let duration: f64 = match duration_out {
        Ok(out) if out.status.success() => {
            let dur_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if DEBUG {
                println!("[DEBUG] Duration string: {}", dur_str);
            }
            dur_str.parse().unwrap_or(0.0)
        }
        Ok(out) => {
            eprintln!(
                "[WARN] ffprobe duration command failed with status: {:?}",
                out.status
            );
            return;
        }
        Err(e) => {
            eprintln!("[WARN] ffprobe duration command error: {}", e);
            return;
        }
    };

    if duration <= 0.0 {
        eprintln!("[WARN] Duration is zero or invalid.");
        return;
    }

    if DEBUG {
        println!("[DEBUG] Duration seconds: {:.2}", duration);
    }

    // Step 3: Get current audio bitrate
    let audio_out = Command::new("/ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=bit_rate",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            input_path,
        ])
        .output();

    let original_audio = match audio_out {
        Ok(out) if out.status.success() => {
            let ab_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if DEBUG {
                println!("[DEBUG] Raw audio bitrate string: '{}'", ab_str);
            }
            ab_str.parse::<u64>().unwrap_or(320_000)
        }
        _ => {
            eprintln!("[WARN] Failed to read audio bitrate, using fallback.");
            320_000
        }
    };

    if DEBUG {
        println!("[DEBUG] Audio bitrate: {}", original_audio);
    }

    // Step 4: Get current file size
    let original_bits = match fs::metadata(input_path) {
        Ok(meta) => meta.len() * 8,
        Err(e) => {
            eprintln!("[WARN] Cannot stat file: {}", e);
            return;
        }
    };

    if DEBUG {
        println!("[DEBUG] Original size in bits: {}", original_bits);
    }

    if original_bits <= target_bits {
        println!("[SKIP] File already within target size {}", input_path);
        return;
    }

    // Step 5: Bitrate budgeting
    let container_overhead = (target_bits as f64 * 0.01) as u64;
    let total_budget = target_bits - container_overhead;
    let total_bitrate = (total_budget as f64 / duration) as u64;

    if DEBUG {
        println!(
            "[DEBUG] Budget bits: {}, Total bitrate: {} bps",
            total_budget, total_bitrate
        );
    }

    let mut selected_audio = original_audio;
    let mut selected_video = 1000;

    for &audio_try in &[original_audio, 320_000, 256_000, 192_000] {
        let video_try = total_bitrate.saturating_sub(audio_try);

        if DEBUG {
            println!("[DEBUG] Trying audio {} → video {}", audio_try, video_try);
        }

        if video_try >= 1000 {
            selected_audio = audio_try;
            selected_video = video_try;
            break;
        }
    }

    if selected_video < 1000 {
        if DEBUG {
            println!("[DEBUG] Capping video bitrate to minimum (1000)");
        }
        selected_video = 1000;
    }

    let expected_bits = ((selected_audio + selected_video) as f64 * duration) as u64;
    if DEBUG {
        println!(
            "[DEBUG] Expected output bits: {} (original: {})",
            expected_bits, original_bits
        );
    }

    if expected_bits >= original_bits {
        println!(
            "[SKIP] Re-encoding would make file bigger, skipping {}",
            input_path
        );
        return;
    }

    println!(
        "[ENCODE] video: {} kbps, audio: {} kbps",
        selected_video / 1000,
        selected_audio / 1000
    );

    // Step 6: Encode
    let tmp_output = format!("{}.{}.tmp.mp4", input_path, target_size_str);

    let ffmpeg = Command::new("/ffmpeg")
        .args([
            "-loglevel",
            "error",
            "-y",
            "-i",
            input_path,
            "-c:v",
            "libx264",
            "-b:v",
            &selected_video.to_string(),
            "-maxrate:v",
            &selected_video.to_string(),
            "-bufsize:v",
            &(selected_video * 2).to_string(),
            "-c:a",
            "aac",
            "-b:a",
            &selected_audio.to_string(),
            &tmp_output,
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn();

    match ffmpeg {
        Ok(mut child) => {
            let result = child.wait();
            if let Ok(status) = result {
                if DEBUG {
                    println!("[DEBUG] ffmpeg exit: {}", status);
                }
            } else {
                eprintln!("[ERROR] ffmpeg wait failed.");
                return;
            }
        }
        Err(e) => {
            eprintln!("[ERROR] Failed to launch ffmpeg: {}", e);
            return;
        }
    }

    // Step 7: Validate output
    let final_bits = match fs::metadata(&tmp_output) {
        Ok(meta) => meta.len() * 8,
        Err(e) => {
            eprintln!("[WARN] Output file missing: {}", e);
            return;
        }
    };

    if DEBUG {
        println!("[DEBUG] Encoded output bits: {}", final_bits);
    }

    if final_bits >= original_bits {
        println!("[SKIP] Encoded file not smaller than original, discarding.");
        let _ = fs::remove_file(&tmp_output);
        return;
    }

    let _ = fs::remove_file(input_path);
    let _ = fs::rename(&tmp_output, input_path);
    println!("[DONE] Re-encoded and replaced original {}", input_path);
}

fn run_download(task: &DownloadTask, log: &mut dyn Write) -> Option<String> {
    println!("[Downloading] {}", task.url);

    let cookies_path_str = "/dl/cookies.txt";
    let cookies_path = Path::new(cookies_path_str);
    let use_cookies = cookies_path.exists();

    // Step 1: Fetch intended output filename via metadata only
    let mut meta_args = vec![
        "--skip-download",
        "--print",
        "filename",
        "-o",
        "%(title)s",
        "--restrict-filenames",
    ];

    if use_cookies {
        meta_args.push("--cookies");
        meta_args.push(cookies_path_str);
    }

    if let Some(code) = &task.twofa {
        meta_args.push("--twofactor");
        meta_args.push(code);
    }

    meta_args.push(&task.url);

    let filename_output = Command::new("/yt-dlp").args(&meta_args).output();

    let filename = match filename_output {
        Ok(out) if out.status.success() => {
            let name = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            if name.is_empty() {
                let _ = writeln!(
                    log,
                    "{} FAILED: metadata lookup returned empty or invalid filename",
                    task.url
                );
                return None;
            }
            name
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            let _ = writeln!(
                log,
                "{} FAILED: metadata status {:?}\nSTDOUT:\n{}\nSTDERR:\n{}",
                task.url, out.status, stdout, stderr
            );
            return None;
        }
        Err(e) => {
            let _ = writeln!(log, "{} FAILED: metadata fetch error: {}", task.url, e);
            return None;
        }
    };

    let output_path = format!("/dl/{}.mp4", filename);

    if Path::new(&output_path).exists() {
        println!("[SKIP] {} already exists, skipping download.", output_path);
        return Some(output_path);
    }

    // Step 2: Perform actual download
    let mut dl_args = vec![
        "--remux",
        "mp4",
        "--merge-output-format",
        "mp4",
        "-o",
        &output_path,
    ];

    if use_cookies {
        dl_args.push("--cookies");
        dl_args.push(cookies_path_str);
    }

    if let Some(code) = &task.twofa {
        dl_args.push("--twofactor");
        dl_args.push(code);
    }

    dl_args.push(&task.url);

    let status = Command::new("/yt-dlp").args(&dl_args).status();

    match status {
        Ok(code) if code.success() => Some(output_path),
        Ok(code) => {
            let _ = writeln!(
                log,
                "{} FAILED: yt-dlp exited with status {}",
                task.url, code
            );
            None
        }
        Err(e) => {
            let _ = writeln!(log, "{} FAILED: download error: {}", task.url, e);
            None
        }
    }
}

fn parse_line_to_task(line: &str) -> Option<DownloadTask> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() || !is_url(parts[0]) {
        return None;
    }

    let url = parts[0].to_string();
    let size = parts
        .get(1)
        .filter(|s| parse_size_to_bits(s).is_some())
        .map(|s| s.to_string());
    let twofa = parts.get(2).map(|s| s.to_string());

    Some(DownloadTask { url, size, twofa })
}

fn process_txt_file(path: &Path) {
    let file = File::open(path).expect("Failed to open urls.txt");
    let reader = BufReader::new(file);

    let mut tasks = vec![];
    let mut errors = vec![];

    for line in reader.lines().map_while(Result::ok) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        if let Some(task) = parse_line_to_task(line) {
            tasks.push(task);
        } else {
            errors.push(format!("SKIPPED: {}", line));
        }
    }

    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let renamed = format!("/dl/urls_{}.txt", epoch);
    let _ = fs::rename(path, &renamed);

    execute_tasks_parallel(tasks, Some(Path::new(&renamed).to_path_buf()), errors);
}

fn interactive_prompt() -> RLResult<()> {
    usage();
    println!("Enter URLs (or 'quit'):");
    let mut rl = DefaultEditor::new()?;

    loop {
        let line = rl.readline("> ");
        match line {
            Ok(input) => {
                let trimmed = input.trim();
                if trimmed.is_empty() {
                    continue;
                }

                rl.add_history_entry(trimmed)?;

                if trimmed.eq_ignore_ascii_case("q")
                    || trimmed.eq_ignore_ascii_case("quit")
                    || trimmed.eq_ignore_ascii_case("exit")
                {
                    break;
                }

                let words: Vec<String> = trimmed.split_whitespace().map(str::to_string).collect();
                let tasks = parse_args_to_tasks(&words);

                if tasks.is_empty() {
                    println!("Invalid input.");
                    continue;
                }

                execute_tasks_parallel(tasks, None, Vec::new());
            }
            Err(_) => break,
        }
    }

    Ok(())
}

fn parse_args_to_tasks(args: &[String]) -> Vec<DownloadTask> {
    let mut tasks = Vec::new();
    let mut i = 0;

    while i < args.len() {
        if is_url(&args[i]) {
            let url = args[i].clone();
            i += 1;

            let size = if i < args.len() && parse_size_to_bits(&args[i]).is_some() {
                let s = args[i].clone();
                i += 1;
                Some(s)
            } else {
                None
            };

            let twofa =
                if i < args.len() && !is_url(&args[i]) && parse_size_to_bits(&args[i]).is_none() {
                    let tf = args[i].clone();
                    i += 1;
                    Some(tf)
                } else {
                    None
                };

            tasks.push(DownloadTask { url, size, twofa });
        } else {
            i += 1;
        }
    }

    tasks
}

fn execute_tasks_parallel(
    tasks: Vec<DownloadTask>,
    log_path: Option<std::path::PathBuf>,
    errors: Vec<String>,
) {
    let log: Arc<Mutex<dyn Write + Send>> = match log_path {
        Some(path) => {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .expect("Failed to open log file");
            Arc::new(Mutex::new(file))
        }
        None => Arc::new(Mutex::new(io::stderr())),
    };
    let mut seen = std::collections::HashSet::new();
    let mut unique_tasks = Vec::new();

    for task in tasks {
        if seen.insert(task.url.clone()) {
            unique_tasks.push(task);
        } else {
            let mut log = log.lock().unwrap();
            let _ = writeln!(log, "[SKIP] Duplicate URL: {}", task.url);
        }
    }

    unique_tasks.into_par_iter().for_each(|task| {
        let mut local_buf = Vec::new();

        if let Some(path) = run_download(&task, &mut local_buf) {
            if let Some(sz) = &task.size {
                let _guard = ENCODE_LOCK.lock().unwrap(); // Exclusive encode
                reencode_video(&path, sz);
            }
        }

        let mut log = log.lock().unwrap();
        let _ = log.write_all(&local_buf);
    });

    for e in errors {
        let _ = writeln!(log.lock().unwrap(), "{}", e);
    }
}

fn normalize_cookies_in_place(path: &Path) -> std::io::Result<()> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut lines_out = vec![String::from("# Netscape HTTP Cookie File")];

    for line in reader.lines().map_while(Result::ok) {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 7 {
            continue;
        }

        let name = parts[0].trim();
        let value = parts[1].trim();
        let domain = parts[2].trim();

        let raw_path = parts[3].trim();
        let path_val = if raw_path.is_empty() { "/" } else { raw_path };

        let expires = parts[4].trim();
        let ts = match chrono::DateTime::parse_from_rfc3339(expires)
            .or_else(|_| chrono::DateTime::parse_from_str(expires, "%a, %d %b %Y %H:%M:%S GMT"))
        {
            Ok(dt) => dt.timestamp(),
            Err(_) => 0,
        };

        let domain_flag = if domain.starts_with('.') {
            "TRUE"
        } else {
            "FALSE"
        };
        let secure = if parts.get(7).map(|s| s.trim()) == Some("✓") {
            "TRUE"
        } else {
            "FALSE"
        };

        lines_out.push(format!(
            "{domain}\t{domain_flag}\t{path_val}\t{secure}\t{ts}\t{name}\t{value}"
        ));
    }

    // Overwrite the same file
    fs::write(path, lines_out.join("\n"))?;
    Ok(())
}

fn main() -> RLResult<()> {
    ctrlc::set_handler(|| {
        println!("\n[CTRL+C]");
        std::process::exit(1);
    })
    .expect("Error setting Ctrl+C handler");

    let cookies_path = Path::new("/dl/cookies.txt");
    if cookies_path.exists() {
        match normalize_cookies_in_place(cookies_path) {
            Ok(_) => println!("[INFO] Normalized cookies.txt in-place"),
            Err(e) => eprintln!("[WARN] Failed to normalize cookies.txt: {}", e),
        }
    }

    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        let txt = Path::new("/dl/urls.txt");
        if txt.exists() {
            process_txt_file(txt);
        } else {
            interactive_prompt()?;
        }
    } else {
        let tasks = parse_args_to_tasks(&args);
        execute_tasks_parallel(tasks, None, Vec::new());
    }

    Ok(())
}
