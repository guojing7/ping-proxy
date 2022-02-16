mod cli;
mod ping;

use std::sync::Arc;

use signal_hook::consts::signal::*;
use signal_hook_tokio::Signals;

use futures::stream::StreamExt;

use ping::Ping;

#[tokio::main]
async fn main() {
    let cli_args = cli::parse().await;
    if let Err(err) = cli_args {
        println!("{}", err);
        std::process::exit(1);
    }

    let signals = Signals::new(&[SIGINT]);
    if let Err(err) = signals {
        println!("create singals error: {}", err);
        std::process::exit(1);
    }

    let signals = signals.unwrap();
    let handle = signals.handle();

    let ping = Arc::new(Ping::new(cli_args.unwrap()));
    let ping_by_signal = ping.clone();
    tokio::spawn(async move { handle_signals(signals, &ping_by_signal).await });

    if let Err(err) = ping.run().await {
        println!("ping error: {}", err);
        std::process::exit(1);
    }

    handle.close();
}

async fn handle_signals(mut signals: Signals, ping: &Arc<Ping>) {
    while let Some(signal) = signals.next().await {
        if signal == SIGINT {
            ping.print_stats();
            signals.handle().close();
            std::process::exit(0);
        }
    }
}
