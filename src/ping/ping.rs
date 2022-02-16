use std::{
    error::Error,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::{
    net::UdpSocket,
    time::{sleep, timeout, Duration},
};

use buf_view::BufViewMut;

use crate::cli::CliArgs;

#[derive(Debug)]
struct Stats {
    rtt_min: u32,
    rtt_max: u32,
    rtt_total: u64,
    rx_count: u32,
    tx_count: u32,
    lost_count: u32,
    timeout_count: u32,
}

impl Stats {
    pub fn new() -> Self {
        Stats {
            rtt_min: u32::MAX,
            rtt_max: 0,
            rtt_total: 0,
            rx_count: 0,
            tx_count: 0,
            lost_count: 0,
            timeout_count: 0,
        }
    }
}

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let loss = if self.tx_count > 0 {
            (self.tx_count - self.rx_count) * 100 / self.tx_count
        } else {
            0
        };

        let _ = write!(
            f,
            "{} packets tx, {} rx, {} lost, {} timeout, {}% packets loss",
            self.tx_count, self.rx_count, self.lost_count, self.timeout_count, loss
        );

        if self.rx_count > 0 {
            let _ = write!(
                f,
                "\nrtt min/max/avg {:03}/{:03}/{:03} ms",
                self.rtt_min as f32 / 1000.0,
                self.rtt_max as f32 / 1000.0,
                self.rtt_total as f32 / (self.rx_count as f32 * 1000.0)
            );
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Ping {
    args: CliArgs,
    stats: Arc<Mutex<Stats>>,
}

impl Ping {
    pub fn new(args: CliArgs) -> Self {
        Ping {
            args,
            stats: Arc::new(Mutex::new(Stats::new())),
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn Error>> {
        println!(
            "ping {} ({}) {} bytes of data",
            self.args.host_name, self.args.host_addr, self.args.length
        );
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        let proxy_addr = SocketAddr::new(self.args.proxy, self.args.port);
        socket.connect(&proxy_addr).await?;

        let mut buf = [0u8; 64];
        let mut buf = BufViewMut::wrap(&mut buf);
        let mut count = self.args.count;
        let mut seq = 0;
        let mut last_time = Instant::now();
        let interval = self.args.interval as u32 * 1000;

        loop {
            if self.args.count != 0 {
                if count == 0 {
                    break;
                }
                count -= 1;
            }

            if seq != 0 {
                let elapse = Instant::now().duration_since(last_time).as_millis() as u32;
                if elapse < interval {
                    let delay = interval - elapse;
                    sleep(Duration::from_millis(delay as u64)).await;
                }
            }

            seq += 1;
            {
                let mut stats = self.stats.lock().unwrap();
                stats.tx_count = seq;
            }

            build_request(&mut buf, seq, self.args.length, &self.args.host_addr);

            last_time = Instant::now();

            if let Err(err) = socket.send(buf.as_slice()).await {
                let mut stats = self.stats.lock().unwrap();
                stats.lost_count += 1;
                if !self.args.quiet {
                    println!(
                        "{} packets tx {} timeout {} lost",
                        stats.tx_count, stats.timeout_count, stats.lost_count
                    );
                }
                if self.args.show_error {
                    println!("send to proxy error: {}", err)
                }
                continue;
            }

            let rx = socket.recv(buf.as_raw_slice());
            let result = timeout(Duration::from_millis(self.args.timeout.into()), rx).await;
            if let Err(err) = result {
                let mut stats = self.stats.lock().unwrap();
                stats.timeout_count += 1;
                if !self.args.quiet {
                    println!(
                        "{} packets tx {} timeout {} lost",
                        stats.tx_count, stats.timeout_count, stats.lost_count
                    );
                }
                if self.args.show_error {
                    println!("recv from proxy error: {}", err)
                }
                continue;
            }

            let result = result.unwrap();
            if let Err(err) = result {
                let mut stats = self.stats.lock().unwrap();
                stats.lost_count += 1;
                if !self.args.quiet {
                    println!(
                        "{} packets tx {} timeout {} lost",
                        stats.tx_count, stats.timeout_count, stats.lost_count
                    );
                }

                if self.args.show_error {
                    println!("read error: {}", err)
                }
                continue;
            }

            let len = result.unwrap();
            self.process_reply(&mut buf, len);
        }

        self.print_stats();

        Ok(())
    }

    fn process_reply(&self, buf: &mut BufViewMut, len: usize) {
        buf.clear();
        buf.set_writer_index(len);
        let seq = buf.read_u32();
        let elapse = buf.read_u32();
        let ttl = buf.read_u8();

        println!(
            "{} bytes from {}: seq {} ttl {} time {}.{:03} ms",
            self.args.length,
            self.args.host_addr,
            seq,
            ttl,
            elapse / 1000,
            elapse % 1000
        );

        self.update_stats(elapse);
    }

    fn update_stats(&self, elapse: u32) {
        let mut stats = self.stats.lock().unwrap();
        if elapse != u32::MAX {
            if stats.rtt_min > elapse {
                stats.rtt_min = elapse;
            }

            if stats.rtt_max < elapse {
                stats.rtt_max = elapse;
            }

            stats.rtt_total += elapse as u64;
        }
        stats.rx_count += 1;
    }

    pub fn print_stats(&self) {
        let stats = self.stats.lock().unwrap();
        println!(
            "\n--- {} ping statistics ---\n{}",
            self.args.host_name, stats
        );
    }
}

///
/// Client to Proxy request
/// | seq(4B) | length(2B) | host length(1B) | host |
/// Proxy to client reply
/// | seq(4B) | elapse (4B) | ttl(1B) |
/// elapse is u32::MAX mean ping timeout
///
fn build_request(buf: &mut BufViewMut, seq: u32, length: u16, addr: &IpAddr) -> usize {
    buf.clear();
    buf.write_u32(seq);
    buf.write_u16(length);
    match addr {
        IpAddr::V4(ip) => {
            buf.write_u8(4);
            buf.write_bytes(&ip.octets());
        }
        IpAddr::V6(ip) => {
            buf.write_u8(16);
            buf.write_bytes(&ip.octets());
        }
    }
    buf.remaining()
}
