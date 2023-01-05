# bruthost

Simple tool to brutforce for vhost on a given ip and domain (CTF oriented tool)

Just an http curl request with Host header, based on curl crate.

## Usage

```bash
Usage: bruthost [OPTIONS] --ip <IP> --domain <DOMAIN> --wordlist <WORDLIST>

Options:
  -i, --ip <IP>              Target ip
  -d, --domain <DOMAIN>      Main domain name
  -w, --wordlist <WORDLIST>  Wordlist path
  -c, --code <CODE>          HTTP code to match, code list, comma separated [default: 200]
  -h, --help                 Print help information
  -V, --version              Print version information

```
