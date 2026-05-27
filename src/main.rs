use clap::Parser;
use std::fs::File;
use std::io::{self, prelude::*, BufReader};
use curl::easy::{Easy, List};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Target ip
    #[arg(short, long)]
    ip: String,

    /// Main domain name
    #[arg(short, long)]
    domain: String,

    /// Wordlist path
    #[arg(short, long)]
    wordlist: String,

    /// HTTP code to match, comma separated (ignored if --filter-baseline is set)
    #[arg(short, long, default_value_t = String::from("200,301,302,403"))]
    code: String,

    /// Filter out responses identical to the baseline (recommended)
    #[arg(short, long, default_value_t = true)]
    filter_baseline: bool,
}

struct Response {
    code: u32,
    size: usize,
}

fn probe(vhost: &str, domain: &str, ip: &str) -> Option<Response> {
    let mut easy = Easy::new();
    let url = format!("http://{}", ip);
    easy.url(&url).unwrap();
    easy.timeout(std::time::Duration::from_secs(5)).unwrap();
    easy.follow_location(false).unwrap();

    let mut list = List::new();
    let host_header = format!("Host: {}.{}", vhost, domain);
    list.append(&host_header).unwrap();
    easy.http_headers(list).unwrap();

    let mut body = Vec::new();
    {
        let mut transfer = easy.transfer();
        transfer.write_function(|data| {
            body.extend_from_slice(data);
            Ok(data.len())
        }).unwrap();
        if let Err(e) = transfer.perform() {
            eprintln!("[-] curl error for {}: {}", vhost, e);
            return None;
        }
    }

    let code = easy.response_code().unwrap();
    Some(Response { code, size: body.len() })
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    let codes: Vec<&str> = args.code.split(',').collect();

    // Establish baseline with a random vhost
    let baseline = probe("__baseline_bruthost__", &args.domain, &args.ip);
    match &baseline {
        Some(r) => println!("[*] Baseline: code={} size={}", r.code, r.size),
        None    => println!("[!] Could not reach target, check IP/port"),
    }

    let file = File::open(&args.wordlist)?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let vhost = match line {
            Ok(l) => l.trim().to_string(),
            Err(_) => { eprintln!("Error reading line"); continue; }
        };
        if vhost.is_empty() { continue; }

        if let Some(resp) = probe(&vhost, &args.domain, &args.ip) {
            // Filter mode: hide responses identical to baseline
            if args.filter_baseline {
                if let Some(ref base) = baseline {
                    if resp.code == base.code && resp.size == base.size {
                        continue;
                    }
                }
            } else {
                // Classic mode: only show matching codes
                if !codes.contains(&resp.code.to_string().as_str()) {
                    continue;
                }
            }
            println!("[+] Found: {}.{} → code={} size={}", vhost, args.domain, resp.code, resp.size);
        }
    }

    Ok(())
}
