use clap::Parser;
use curl::easy::{Easy, List};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::sync::{Arc, Mutex};

#[derive(Parser, Debug)]
#[command(author, version, about = "Virtual host brute-forcer", long_about = None)]
struct Args {
    /// Target IP address
    #[arg(short, long)]
    ip: String,

    /// Main domain name
    #[arg(short, long)]
    domain: String,

    /// Wordlist path
    #[arg(short, long)]
    wordlist: String,

    /// HTTP codes to match, comma separated (used when --no-filter-baseline is set)
    #[arg(short, long, default_value_t = String::from("200,301,302,403"))]
    code: String,

    /// Disable baseline filtering and match on --code instead
    #[arg(long, action = clap::ArgAction::SetTrue)]
    no_filter_baseline: bool,

    /// Target port
    #[arg(short = 'P', long, default_value_t = 80)]
    port: u16,

    /// Use HTTPS instead of HTTP
    #[arg(long, action = clap::ArgAction::SetTrue)]
    https: bool,

    /// Number of parallel threads
    #[arg(short = 't', long, default_value_t = 10)]
    threads: usize,

    /// Save results to output file
    #[arg(short, long)]
    output: Option<String>,

    /// Use wordlist as-is, without sanitizing or deduplicating entries
    #[arg(long, action = clap::ArgAction::SetTrue)]
    raw_wordlist: bool,
}

struct Response {
    code: u32,
    size: usize,
}

/// Strip scheme, port, and path from a wordlist entry, returning a clean hostname label.
/// Returns None if the result is empty or contains characters invalid in a hostname.
/// Examples:
///   "https://admin"     → Some("admin")
///   "http://dev:8080/"  → Some("dev")
///   "staging"           → Some("staging")
///   "http://"           → None
fn sanitize_vhost(raw: &str) -> Option<String> {
    let s = raw.trim();
    // Strip scheme
    let s = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);
    // Drop path (everything after the first '/')
    let s = s.split('/').next().unwrap_or("");
    // Drop port
    let s = s.split(':').next().unwrap_or("");
    // Reject empty strings or entries with characters invalid in a hostname label
    if s.is_empty() || s.contains(|c: char| !c.is_alphanumeric() && c != '-' && c != '.') {
        return None;
    }
    Some(s.to_string())
}

fn probe(vhost: &str, domain: &str, ip: &str, port: u16, https: bool) -> Result<Response, curl::Error> {
    let mut easy = Easy::new();
    let scheme = if https { "https" } else { "http" };
    easy.url(&format!("{}://{}:{}", scheme, ip, port))?;
    easy.timeout(std::time::Duration::from_secs(5))?;
    easy.follow_location(false)?;
    if https {
        // Disable cert verification since we target by IP
        easy.ssl_verify_peer(false)?;
        easy.ssl_verify_host(false)?;
    }

    let mut list = List::new();
    list.append(&format!("Host: {}.{}", vhost, domain))?;
    easy.http_headers(list)?;

    let mut body = Vec::new();
    {
        let mut transfer = easy.transfer();
        transfer.write_function(|data| {
            body.extend_from_slice(data);
            Ok(data.len())
        })?;
        transfer.perform()?;
    }

    let code = easy.response_code()?;
    Ok(Response { code, size: body.len() })
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    let filter_baseline = !args.no_filter_baseline;

    // Parse valid codes once upfront as u32
    let codes: Vec<u32> = args.code
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    // Configure rayon thread pool
    rayon::ThreadPoolBuilder::new()
        .num_threads(args.threads)
        .build_global()
        .expect("Failed to build thread pool");

    // Establish baseline — exit immediately on failure
    println!("[*] Probing baseline...");
    let baseline = match probe("__baseline_bruthost__", &args.domain, &args.ip, args.port, args.https) {
        Ok(r) => {
            println!("[*] Baseline: code={} size={}", r.code, r.size);
            r
        }
        Err(e) => {
            eprintln!(
                "[!] Could not reach {}:{} — {} — check IP/port and connectivity.",
                args.ip, args.port, e
            );
            std::process::exit(1);
        }
    };

    // Optional thread-safe output file
    let out_writer: Option<Arc<Mutex<BufWriter<File>>>> = args
        .output
        .as_ref()
        .map(|path| {
            let f = File::create(path).expect("Cannot create output file");
            Arc::new(Mutex::new(BufWriter::new(f)))
        });

    // Load wordlist into memory
    let file = File::open(&args.wordlist)?;
    let lines: Vec<String> = if args.raw_wordlist {
        BufReader::new(file)
            .lines()
            .filter_map(|l| l.ok())
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    } else {
        let raw_count = std::cell::Cell::new(0usize);
        let cleaned: Vec<String> = BufReader::new(file)
            .lines()
            .filter_map(|l| l.ok())
            .inspect(|_| raw_count.set(raw_count.get() + 1))
            .filter_map(|l| sanitize_vhost(&l))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let skipped = raw_count.get().saturating_sub(cleaned.len());
        if skipped > 0 {
            println!("[*] Skipped {} invalid/duplicate entries from wordlist", skipped);
        }
        cleaned
    };

    println!(
        "[*] Loaded {} words — starting scan ({} threads)\n",
        lines.len(),
        args.threads
    );

    // Progress bar
    let pb = ProgressBar::new(lines.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            " {spinner:.cyan} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) · {msg}",
        )
        .unwrap()
        .progress_chars("=>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(80));

    // Parallel scan
    lines.par_iter().for_each(|vhost| {
        pb.set_message(vhost.clone());

        match probe(vhost, &args.domain, &args.ip, args.port, args.https) {
            Ok(resp) => {
                let show = if filter_baseline {
                    // Hide responses identical to baseline (code + size)
                    resp.code != baseline.code || resp.size != baseline.size
                } else {
                    // Classic mode: only show listed status codes
                    codes.contains(&resp.code)
                };

                if show {
                    let msg = format!(
                        "[+] Found: {}.{} → code={} size={}",
                        vhost, args.domain, resp.code, resp.size
                    );
                    // pb.println() prints above the bar without breaking it
                    pb.println(&msg);
                    if let Some(ref w) = out_writer {
                        let mut writer = w.lock().unwrap();
                        writeln!(writer, "{}", msg).ok();
                    }
                }
            }
            Err(e) => pb.println(format!("[-] Error probing {}: {}", vhost, e)),
        }

        pb.inc(1);
    });

    pb.finish_and_clear();

    // Flush output file
    if let Some(ref w) = out_writer {
        w.lock().unwrap().flush().ok();
    }

    println!("[*] Scan complete.");
    Ok(())
}
