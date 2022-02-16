use std::{
    env,
    net::{AddrParseError, IpAddr, Ipv4Addr},
    num::ParseIntError,
};

use tokio::net;

#[derive(Debug)]
pub struct CliArgumentError {
    kind: String,
}

impl CliArgumentError {
    pub fn new(msg: &str) -> Self {
        CliArgumentError {
            kind: msg.to_string(),
        }
    }
}

impl std::fmt::Display for CliArgumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl std::error::Error for CliArgumentError {}

#[derive(Debug)]
pub enum ParseError {
    Parse(ParseIntError),
    Address(AddrParseError),
    Argument(CliArgumentError),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            ParseError::Parse(ref err) => write!(f, "Parse integer error: {}", err),
            ParseError::Address(ref err) => write!(f, "Invalid IP address: {}", err),
            ParseError::Argument(ref err) => write!(f, "Invalid argument: {}", err),
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        println!("source {}", line!());
        match *self {
            ParseError::Parse(ref err) => Some(err),
            ParseError::Address(ref err) => Some(err),
            ParseError::Argument(ref err) => Some(err),
        }
    }
}

impl From<ParseIntError> for ParseError {
    fn from(err: ParseIntError) -> Self {
        ParseError::Parse(err)
    }
}

impl From<AddrParseError> for ParseError {
    fn from(err: AddrParseError) -> Self {
        ParseError::Address(err)
    }
}

impl From<CliArgumentError> for ParseError {
    fn from(err: CliArgumentError) -> Self {
        ParseError::Argument(err)
    }
}

#[derive(Debug)]
pub struct CliArgs {
    pub show_error: bool,
    pub quiet: bool,
    pub interval: u8,
    pub length: u16,
    pub port: u16,
    pub timeout: u16,
    pub count: u32,
    pub proxy: IpAddr,
    pub host_addr: IpAddr,
    pub host_name: String,
}

impl CliArgs {
    pub fn new() -> Self {
        CliArgs {
            show_error: false,
            quiet: false,
            interval: 1,
            length: 64,
            port: 2000,
            timeout: 4000,
            count: u32::MAX,
            proxy: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            host_addr: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            host_name: String::new(),
        }
    }
}

fn usage() {
    println!("Usage: ping [options] host");
    println!("  -c    ping count");
    println!("  -e    show error reason");
    println!("  -i    interval time (secs), default 1");
    println!("  -l    packet length");
    println!("  -r    proxy remote address");
    println!("  -p    proxy remote port");
    println!("  -q    quiet output");
    println!("  -t    ping timeout (millis), default 4000");
    println!("  -v    version");
    println!("  -h    help");
}

fn value_check(value: Option<&String>) -> Result<&String, CliArgumentError> {
    match value {
        Some(v) => Ok(v),
        None => Err(CliArgumentError::new("Miss arguments")),
    }
}

pub async fn parse() -> Result<CliArgs, ParseError> {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        let err = CliArgumentError::new("no host specified");
        return Err(ParseError::Argument(err));
    }

    let mut cli_args = CliArgs::new();
    let mut iter = args.iter();
    while let Some(key) = iter.next() {
        let key = key.as_str();
        if key.starts_with('-') {
            if !cli_args.host_addr.is_unspecified() {
                let err = CliArgumentError::new("invalid option order");
                return Err(ParseError::Argument(err));
            }

            match key {
                "-c" => {
                    let value = value_check(iter.next())?;
                    cli_args.count = value.parse::<u32>()?;
                }
                "-e" => {
                    cli_args.show_error = true;
                }
                "-l" => {
                    let value = value_check(iter.next())?;
                    cli_args.length = value.parse::<u16>()?;
                }
                "-i" => {
                    let value = value_check(iter.next())?;
                    cli_args.interval = value.parse::<u8>()?;
                }
                "-r" => {
                    let value = value_check(iter.next())?;
                    if let Ok(addr) = value.parse::<IpAddr>() {
                        cli_args.proxy = addr;
                    } else {
                        let host = format!("{}:0", value);
                        if let Ok(mut iter) = net::lookup_host(host).await {
                            cli_args.proxy = iter.next().unwrap().ip();
                        } else {
                            let err = CliArgumentError::new("invalid proxy host");
                            return Err(ParseError::Argument(err));
                        }
                    }
                }
                "-p" => {
                    let value = value_check(iter.next())?;
                    cli_args.port = value.parse::<u16>()?;
                    if cli_args.port == 0 {
                        let err = CliArgumentError::new("invalid port");
                        return Err(ParseError::Argument(err));
                    }
                }
                "-q" => {
                    cli_args.quiet = true;
                }
                "-t" => {
                    let value = value_check(iter.next())?;
                    cli_args.timeout = value.parse::<u16>()?;
                }
                "-v" => {
                    println!("version 0.1.0");
                    std::process::exit(0);
                }
                "-h" => {
                    usage();
                    std::process::exit(0);
                }
                _ => {
                    let err = CliArgumentError::new("unknown option");
                    return Err(ParseError::Argument(err));
                }
            }
        } else if cli_args.host_addr.is_unspecified() {
            if let Ok(addr) = key.parse::<IpAddr>() {
                cli_args.host_addr = addr;
            } else {
                let host = format!("{}:0", key);
                if let Ok(mut iter) = net::lookup_host(host).await {
                    cli_args.host_addr = iter.next().unwrap().ip();
                } else {
                    let err = CliArgumentError::new("invalid host");
                    return Err(ParseError::Argument(err));
                }
            }

            cli_args.host_name.push_str(key);
        } else {
            let err = CliArgumentError::new("already specified host");
            return Err(ParseError::Argument(err));
        }
    }

    if cli_args.host_addr.is_unspecified() {
        let err = CliArgumentError::new("no host specified");
        return Err(ParseError::Argument(err));
    }

    Ok(cli_args)
}
