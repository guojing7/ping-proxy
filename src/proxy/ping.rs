use socket2::{Domain, Protocol, Socket, Type};
use std::{
    io,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::net::UdpSocket;

use buf_view::BufViewMut;

use crate::proxy::ProxyInfo;

pub const PING_MAGIC: u32 = 0x19170923;

#[derive(Debug)]
enum IcmpError {
    Magic,
    IpHeader,
    Type,
    Checksum,
    ID,
}

impl std::fmt::Display for IcmpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IcmpError::Magic => write!(f, "Invalid MAGIC"),
            IcmpError::IpHeader => write!(f, "Invalid IP header"),
            IcmpError::Type => write!(f, "Invalid ICMP type"),
            IcmpError::Checksum => write!(f, "Invalid checksum"),
            IcmpError::ID => write!(f, "Invalid ID"),
        }
    }
}

impl std::error::Error for IcmpError {}

#[derive(Debug)]
pub struct Ping {
    identifier: u16,
    seq: Arc<Mutex<u16>>,
    pid: u32,
    socket4: UdpSocket,
    socket6: UdpSocket,
    uptime: Instant,
}

impl Ping {
    pub async fn new() -> io::Result<Ping> {
        let sock4 = create_socket(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))?;
        let sock6 = create_socket(Domain::IPV6, Type::RAW, Some(Protocol::ICMPV6))?;

        Ok(Ping {
            identifier: 0x1917,
            seq: Arc::new(Mutex::new(0x0923)),
            pid: std::process::id(),
            socket4: sock4,
            socket6: sock6,
            uptime: Instant::now(),
        })
    }

    pub async fn send_to(
        &self,
        source: &SocketAddr,
        target: &SocketAddr,
        seq: u32,
        len: usize,
    ) -> io::Result<usize> {
        let mut buf = [0u8; 1024 * 64];
        assert!(len < buf.len());
        let mut buf = BufViewMut::wrap(&mut buf);
        self.icmp_request_build(seq, source, len, &mut buf);
        let socket = if target.is_ipv4() {
            &self.socket4
        } else {
            &self.socket6
        };
        socket.send_to(buf.as_slice(), target).await?;

        Ok(len)
    }

    pub async fn recv_from_v4(&self) -> Option<ProxyInfo> {
        let mut buf = [0u8; 1024 * 64];
        if let Ok((len, _)) = self.socket4.recv_from(&mut buf).await {
            if let Ok(info) = self.parse(&mut buf[..len]) {
                return Some(info);
            }
        }

        None
    }

    pub async fn recv_from_v6(&self) -> Option<ProxyInfo> {
        let mut buf = [0u8; 1024 * 64];
        if let Ok((len, _)) = self.socket6.recv_from(&mut buf).await {
            if let Ok(info) = self.parse(&mut buf[..len]) {
                return Some(info);
            }
        }

        None
    }

    fn parse(&self, buf: &mut [u8]) -> Result<ProxyInfo, IcmpError> {
        let now = self.elapsed().as_micros() as u64;
        let len = buf.len();
        let mut buf = BufViewMut::wrap_with(buf, 0, len);

        let ihl = buf.get_u8(0);
        let ver = ihl >> 4;
        let ttl;
        let icmp_offset;

        if ver == 4 {
            ttl = buf.get_u8(8);
            icmp_offset = ((ihl & 0xF) * 4) as usize;
            let icmp_type = buf.get_u8(icmp_offset);
            if icmp_type != 0 {
                return Err(IcmpError::Type);
            }
        } else if ver == 6 {
            ttl = buf.get_u8(7);
            icmp_offset = 40usize;
            let icmp_type = buf.get_u8(icmp_offset);
            if icmp_type != 129 {
                return Err(IcmpError::Type);
            }
        } else {
            return Err(IcmpError::IpHeader);
        }

        let magic_index = icmp_offset + 8;
        buf.set_reader_index(magic_index);
        let magic = buf.read_u32();
        if magic != PING_MAGIC {
            return Err(IcmpError::Magic);
        }

        let checksum = buf.read_u16();
        buf.set_u16(magic_index + 4, 0); // clear checksum

        let pid = buf.read_u32();
        if pid != self.pid {
            return Err(IcmpError::ID);
        }

        let seq = buf.read_u32();
        let tx_time = buf.read_u64();
        let port = buf.read_u16();
        let len = buf.read_u8();

        let host = if len == 4 {
            let mut v4 = [0u8; 4];
            buf.read_bytes(&mut v4);
            IpAddr::from(v4)
        } else {
            let mut v6 = [0u8; 16];
            buf.read_bytes(&mut v6);
            IpAddr::from(v6)
        };

        let index = buf.reader_index();

        if checksum != ip_checksum(&mut buf.as_raw_slice()[magic_index..index]) {
            return Err(IcmpError::Checksum);
        }

        let target = SocketAddr::new(host, port);
        let elapse = (now - tx_time) as u32;

        Ok(ProxyInfo {
            target,
            seq,
            elapse,
            ttl,
        })
    }

    //
    // see https://en.wikipedia.org/wiki/Internet_Control_Message_Protocol
    //
    fn icmp_request_build(
        &self,
        client_seq: u32,
        addr: &SocketAddr,
        len: usize,
        buf: &mut BufViewMut,
    ) {
        let icmp_type = if addr.ip().is_ipv4() { 8 } else { 128 };
        buf.write_u8(icmp_type); //type
        buf.write_u8(0); //code
        buf.write_u16(0); //checksum
        buf.write_u16(self.identifier);

        let seq;
        {
            let mut mseq = self.seq.lock().unwrap();
            seq = *mseq;
            *mseq = seq.overflowing_add(1).0;
        }
        buf.write_u16(seq);

        //
        // private data
        // checksum from magic to host
        // | magic(4B) | checksum(2B) | pid(4B) | client seq(4B) | micro_sec(8B) | port(2B) | host length(1B) | host |
        //
        let magic_index = buf.writer_index();
        buf.write_u32(PING_MAGIC);
        buf.write_u16(0); //clear checksum
        buf.write_u32(self.pid);
        let now = self.uptime.elapsed();
        buf.write_u32(client_seq);
        buf.write_u64(now.as_micros() as u64);
        buf.write_u16(addr.port());

        match addr.ip() {
            IpAddr::V4(ip) => {
                buf.write_u8(4);
                buf.write_bytes(&ip.octets());
            }
            IpAddr::V6(ip) => {
                buf.write_u8(16);
                buf.write_bytes(&ip.octets());
            }
        }

        let checksum = ip_checksum(&mut buf.as_slice()[magic_index..]);
        buf.set_u16(magic_index + 4, checksum);

        let index = buf.writer_index();
        for i in 0..(len - index) {
            buf.write_u8((i & 0xFF) as u8);
        }

        let checksum = ip_checksum(buf.as_slice());
        buf.set_u16(2, checksum);
    }

    pub fn elapsed(&self) -> Duration {
        self.uptime.elapsed()
    }
}

fn create_socket(domain: Domain, typ: Type, protocol: Option<Protocol>) -> io::Result<UdpSocket> {
    let socket = Socket::new(domain, typ, protocol)?;
    socket.set_nonblocking(true)?;
    let _ = socket.set_recv_buffer_size(1 << 20);
    #[cfg(unix)]
    let socket = {
        use std::os::unix::io::{FromRawFd, IntoRawFd};
        unsafe { std::net::UdpSocket::from_raw_fd(socket.into_raw_fd()) }
    };

    #[cfg(windows)]
    let socket = {
        use std::os::windows::prelude::{FromRawSocket, IntoRawSocket};
        unsafe { std::net::UdpSocket::from_raw_socket(socket.into_raw_socket()) }
    };

    UdpSocket::from_std(socket)
}

fn ip_checksum(buf: &mut [u8]) -> u16 {
    let odd = (buf.len() & 1) == 1;
    let len = if odd { buf.len() - 1 } else { buf.len() };

    let mut sum = 0u32;
    let mut index = 0;

    while index < len {
        sum += ((buf[index] as u32) << 8) | (buf[index + 1] as u32);
        index += 2;
    }

    if odd {
        sum += buf[index] as u32;
    }

    sum = (sum >> 16) + (sum & 0xFFFF);
    sum += sum >> 16;

    !sum as u16
}
