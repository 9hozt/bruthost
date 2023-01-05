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

   /// HTTP code to match, code list, comma separated 
   #[arg(short, long, default_value_t = String::from("200"))]
   code: String,
}

fn test_host(vhost: String, codes: Vec<&str>, domain: String, ip: String){

    let mut easy = Easy::new();

    let mut url = String::from("http://");
    url.push_str(ip.as_str());
    easy.url(url.as_str()).unwrap();

    let mut list = List::new();
    let current_host = vhost.clone()+"."+&domain;
    let mut host_header = String::from("Host: ");
    host_header.push_str(&current_host);
    list.append(&host_header).unwrap();

    easy.http_headers(list).unwrap();
    easy.perform().unwrap();

    let code = easy.response_code().unwrap();

    if codes.contains(&code.to_string().as_str()) {
        println!("[+] Match for {} with code {}", vhost, code);
    }
    
}
fn main() -> io::Result<()> {
    let args = Args::parse();
    let path = args.wordlist;

    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let binding = args.code.clone().to_owned();
    let codes = binding.split(",");
    let vec = codes.collect::<Vec<&str>>();

    for line in reader.lines() {
            match line {
                Ok(line) => {
                    test_host(line, vec.clone(), args.domain.clone(), args.ip.clone());
                    
                },
                Err(_) => println!("Error while reading line"),
            }
    }
    Ok(())
} 
