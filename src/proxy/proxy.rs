use std::{
    error::Error,
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use tokio::net::UdpSocket;

use buf_view::{BufView, BufViewMut};

use crate::ping::Ping;

#[derive(Debug)]
pub struct ProxyInfo {
    pub target: SocketAddr,
    pub seq: u32,
    pub elapse: u32,
    pub ttl: u8,
}

pub async fn server(addr: &str, port: u16) -> Result<(), Box<dyn Error>> {
    let ping = Arc::new(Ping::new().await?);

    let host = format! {"{}:{}", addr, port};
    let socket = Arc::new(UdpSocket::bind(host).await?);

    println!("listen on port {port} ...");

    ping_v4_run(&ping, &socket);
    ping_v6_run(&ping, &socket);

    let mut buf = [0u8; 1024];

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, addr)) => proxy_rx(&ping, &buf, len, addr).await,
            Err(err) => println!("proxy rx error: {}", err),
        }
    }
}

async fn proxy_rx(ping: &Ping, buf: &[u8], len: usize, addr: SocketAddr) {
    let mut buf = BufView::wrap_with(buf, 0, len);
    let seq = buf.read_u32();
    let pkt_len = buf.read_u16() as usize;
    let host_len = buf.read_u8() as usize;

    if host_len + 7 != len {
        return;
    }

    let host = if host_len == 4 {
        let mut v4 = [0u8; 4];
        buf.read_bytes(&mut v4);
        IpAddr::from(v4)
    } else {
        let mut v6 = [0u8; 16];
        buf.read_bytes(&mut v6);
        IpAddr::from(v6)
    };

    let target = SocketAddr::new(host, 0);
    if let Err(err) = ping.send_to(&addr, &target, seq, pkt_len).await {
        println!("ping {:?} error: {}", target, err);
    }
}

fn ping_v4_run(ping: &Arc<Ping>, socket: &Arc<UdpSocket>) {
    let ping = ping.clone();
    let socket = socket.clone();
    tokio::spawn(async move { ping_v4_rx(&ping, &socket).await });
}

fn ping_v6_run(ping: &Arc<Ping>, socket: &Arc<UdpSocket>) {
    let ping = ping.clone();
    let socket = socket.clone();
    tokio::spawn(async move { ping_v6_rx(&ping, &socket).await });
}

async fn ping_v4_rx(ping: &Arc<Ping>, socket: &Arc<UdpSocket>) {
    loop {
        if let Some(info) = ping.recv_from_v4().await {
            ping_rx(socket, &info).await;
        }
    }
}

async fn ping_v6_rx(ping: &Arc<Ping>, socket: &UdpSocket) {
    loop {
        if let Some(info) = ping.recv_from_v6().await {
            ping_rx(socket, &info).await;
        }
    }
}

async fn ping_rx(socket: &UdpSocket, info: &ProxyInfo) {
    let mut buf = [0u8; 32];
    let len = build_proxy_respone(&mut buf, info);
    if let Err(err) = socket.send_to(&buf[..len], &info.target).await {
        println!("proxy response error: {}", err);
    }
}

fn build_proxy_respone(buf: &mut [u8], info: &ProxyInfo) -> usize {
    let mut buf = BufViewMut::wrap(buf);
    buf.write_u32(info.seq);
    buf.write_u32(info.elapse);
    buf.write_u8(info.ttl);
    buf.remaining()
}
