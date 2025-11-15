use quiche::{Config, ConnectionId};
use ring::rand::*;
use std::net::UdpSocket;

const MAX_DATAGRAM_SIZE: usize = 1350;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_addr = "127.0.0.1:4443";

    // Setup config
    let mut config = Config::new(quiche::PROTOCOL_VERSION)?;
    config.set_application_protos(&[b"hq-29"])?;
    config.verify_peer(false);
    config.set_max_idle_timeout(5000);
    config.set_max_recv_udp_payload_size(MAX_DATAGRAM_SIZE);
    config.set_max_send_udp_payload_size(MAX_DATAGRAM_SIZE);
    config.set_initial_max_data(10_000_000);
    config.set_initial_max_stream_data_bidi_local(1_000_000);
    config.set_initial_max_streams_bidi(100);

    // UDP bind
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect(server_addr)?;

    let local_addr = socket.local_addr()?;
    let peer_addr = socket.peer_addr()?;

    // Random SCID
    let mut rand_bytes = [0u8; quiche::MAX_CONN_ID_LEN];
    if let Err(_) = SystemRandom::new().fill(&mut rand_bytes) {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "ring::rand::SystemRandom::fill failed",
        )));
    }
    let scid = ConnectionId::from_ref(&rand_bytes);

    // Establish QUIC connection
    let mut conn = quiche::connect(
        Some("localhost"),
        &scid,
        local_addr,
        peer_addr,
        &mut config,
    )?;
    println!("Client: connecting to {}", peer_addr);

    let mut buf = [0u8; 65535];
    let mut out = [0u8; MAX_DATAGRAM_SIZE];

    // Send initial packet
    let (len, send_info) = conn.send(&mut out)?;
    socket.send_to(&out[..len], send_info.to)?;
    println!("Client: sent initial packet ({} bytes) to {}", len, send_info.to);

    // --- WAIT FOR HANDSHAKE + TRANSPORT PARAMS ---
    let mut iter: usize = 0;
    loop {
        // Receive packets
        if let Ok((len, from)) = socket.recv_from(&mut buf) {
            println!("Client: recv {} bytes from {}", len, from);
            let recv_info = quiche::RecvInfo { from, to: local_addr };
            match conn.recv(&mut buf[..len], recv_info) {
                Ok(_) => {}
                Err(e) => eprintln!("Client: conn.recv error: {:?}", e),
            }
        }

        // Send handshake flight packets
        while let Ok((len, send_info)) = conn.send(&mut out) {
            socket.send_to(&out[..len], send_info.to)?;
        }

        // Safe moment to send application data:
        if conn.is_established() && conn.peer_streams_left_bidi() > 0 {
            println!("Client: handshake complete");
            break;
        }

        iter += 1;
        if iter % 5 == 0 {
            println!("Client: handshake loop iter={} is_established={} peer_streams_left_bidi={}",
                iter, conn.is_established(), conn.peer_streams_left_bidi());
        }
    }

    println!("Client: handshake complete, sending application message...");

    // Send a small application message to open a bidirectional stream
    let stream_id = 0;
    let message = b"Hello from client!";

    match conn.stream_send(stream_id, message, true) {
        Ok(wrote) => println!("Client: stream_send wrote {} bytes on stream {}", wrote, stream_id),
        Err(e) => eprintln!("Client: stream_send error: {:?}", e),
    }

    // Flush packets until everything is sent
    while let Ok((len, send_info)) = conn.send(&mut out) {
        socket.send_to(&out[..len], send_info.to)?;
    }

    println!("Client: waiting for server message...");

    // Wait for application data from the server and print it.
    use std::time::Duration;
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;
    
    let mut done = false;
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(5);
    
    loop {
        match socket.recv_from(&mut buf) {
            Ok((len, from)) => {
                println!("Client: recv {} bytes from {}", len, from);
                let recv_info = quiche::RecvInfo { from, to: local_addr };
                match conn.recv(&mut buf[..len], recv_info) {
                    Ok(_) => {}
                    Err(e) => eprintln!("Client: conn.recv error: {:?}", e),
                }

                let readable: Vec<u64> = conn.readable().collect();
                if !readable.is_empty() {
                    println!("Client: conn.readable() -> {:?}", readable);
                }

                for sid in readable {
                    loop {
                        match conn.stream_recv(sid, &mut buf) {
                            Ok((read, fin)) => {
                                if read == 0 {
                                    break;
                                }

                                let msg = String::from_utf8_lossy(&buf[..read]);
                                println!("Client received on stream {}: {}", sid, msg);

                                if fin {
                                    done = true;
                                    break;
                                }
                            }
                            Err(e) => {
                                eprintln!("Client: stream_recv error on {}: {:?}", sid, e);
                                break;
                            }
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                // Timeout is normal, continue
            }
            Err(e) => {
                eprintln!("Client: recv_from error: {:?}", e);
                break;
            }
        }

        // Send any pending packets
        while let Ok((len, send_info)) = conn.send(&mut out) {
            socket.send_to(&out[..len], send_info.to)?;
        }

        if done || conn.is_closed() {
            break;
        }
        
        if start.elapsed() > timeout {
            println!("Client: timeout waiting for server response");
            break;
        }
        
        std::thread::sleep(Duration::from_millis(10));
    }

    // Clean close
    let _ = conn.close(true, 0x00, b"done");

    println!("Client: finished.");
    Ok(())
}
