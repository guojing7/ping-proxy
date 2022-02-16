mod ping;
mod proxy;

#[derive(Debug)]
struct CliArgs {
    port: u16,
}

#[tokio::main]
async fn main() {
    let args = cli_parse();
    if let Err(err) = proxy::server("0.0.0.0", args.port).await {
        println!("proxy run error: {}", err);
        std::process::exit(1);
    }
}

impl CliArgs {
    pub fn new() -> Self {
        CliArgs { port: 2000 }
    }
}

fn usage() {
    println!("Usage: proxy [options]");
    println!("  -p    listen port, default 2000");
    println!("  -v    version");
    println!("  -h    help");
}

fn cli_parse() -> CliArgs {
    let mut cli_args = CliArgs::new();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut iter = args.iter();
    while let Some(key) = iter.next() {
        let key = key.as_str();
        if !key.starts_with('-') {
            println!("invalid option");
            std::process::exit(1);
        }

        match key {
            "-p" => {
                if let Some(value) = iter.next() {
                    if let Ok(port) = value.parse::<u16>() {
                        if port > 0 {
                            cli_args.port = port;
                            continue;
                        }
                    }
                    println!("invalid port");
                    std::process::exit(1);
                } else {
                    println!("no port specified");
                    std::process::exit(1);
                }
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
                println!("uknown option");
                std::process::exit(1);
            }
        }
    }

    cli_args
}
