use quiche::{ConnectionId, Config};
use std::net::UdpSocket;

const MAX_DATAGRAM_SIZE: usize = 1350;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind = "127.0.0.1:4443";
    let socket = UdpSocket::bind(bind)?;
    println!("Server listening on {}", bind);

    let mut config = Config::new(quiche::PROTOCOL_VERSION)?;
    // Use ALPN "hq-29"
    config.set_application_protos(&[b"hq-29"])?;
    config.verify_peer(false);
    config.set_max_idle_timeout(5000);
    config.set_max_recv_udp_payload_size(MAX_DATAGRAM_SIZE);
    config.set_max_send_udp_payload_size(MAX_DATAGRAM_SIZE);
    config.set_initial_max_data(10_000_000);
    config.set_initial_max_stream_data_bidi_local(1_000_000);
    config.set_initial_max_stream_data_bidi_remote(1_000_000);
    config.set_initial_max_streams_bidi(100);

    // Load server certificate and private key (expects files in project root)
    config.load_cert_chain_from_pem_file("cert.pem")?;
    config.load_priv_key_from_pem_file("key.pem")?;

    let mut buf = [0u8; MAX_DATAGRAM_SIZE];
    let mut out = [0u8; MAX_DATAGRAM_SIZE];

    // Single-connection demo: accept first incoming Initial and finish.
    loop {
        let (len, from) = socket.recv_from(&mut buf)?;

        let mut hdr_buf = &mut buf[..len];
        let hdr = match quiche::Header::from_slice(&mut hdr_buf, quiche::MAX_CONN_ID_LEN) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("failed to parse header: {:?}", e);
                continue;
            }
        };

        // Use destination connection id from the header as the server's connection id
        let scid = ConnectionId::from_ref(&hdr.dcid);

        let mut conn = match quiche::accept(&scid, None, socket.local_addr()?, from, &mut config) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("failed to accept connection: {:?}", e);
                continue;
            }
        };

        // Process the packet we just received
        let recv_info = quiche::RecvInfo { from, to: socket.local_addr()? };
        let _ = conn.recv(&mut buf[..len], recv_info);

        // Send any handshake packets produced by accept/recv
        while let Ok((write, send_info)) = conn.send(&mut out) {
            socket.send_to(&out[..write], send_info.to)?;
        }

        // Drive the handshake until established. Once established, wait for client application data
        // and reply on the same stream (bidirectional).
        loop {
            if conn.is_established() {
                println!("Connection established, waiting for client message...");

                // Switch to non-blocking mode to drive app IO for a short window
                use std::time::{Duration, Instant};
                socket.set_nonblocking(true)?;
                let deadline = Instant::now() + Duration::from_secs(10);
                let mut message_sent = false;

                while Instant::now() < deadline && !conn.is_closed() {
                    if let Ok((len, from2)) = socket.recv_from(&mut buf) {
                        println!("Server: recv {} bytes from {}", len, from2);
                        let recv_info = quiche::RecvInfo { from: from2, to: socket.local_addr()? };
                        match conn.recv(&mut buf[..len], recv_info) {
                            Ok(_) => {}
                            Err(e) => eprintln!("Server: conn.recv err: {:?}", e),
                        }
                    }

                    // If there are readable streams (client sent data), read and reply.
                    let readable: Vec<u64> = conn.readable().collect();
                    if !readable.is_empty() {
                        println!("Server: conn.readable() -> {:?}", readable);
                    }

                    for sid in readable {
                        loop {
                            match conn.stream_recv(sid, &mut buf) {
                                Ok((read, fin)) => {
                                    if read == 0 {
                                        break;
                                    }

                                    let msg = String::from_utf8_lossy(&buf[..read]);
                                    println!("Server received on stream {}: {}", sid, msg);

                                    // Reply on the same stream id
                                    let reply = b"Hello from QUIC server!";
                                    match conn.stream_send(sid, reply, fin) {
                                        Ok(wrote) => {
                                            println!("Server: stream_send wrote {} bytes on stream {}", wrote, sid);
                                            message_sent = true;
                                        }
                                        Err(e) => eprintln!("Server: stream_send error on {}: {:?}", sid, e),
                                    }

                                    if fin {
                                        break;
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Server: stream_recv error on {}: {:?}", sid, e);
                                    break;
                                }
                            }
                        }
                    }

                    while let Ok((write, send_info)) = conn.send(&mut out) {
                        socket.send_to(&out[..write], send_info.to)?;
                    }

                    // Exit after message sent and flushed
                    if message_sent {
                        std::thread::sleep(Duration::from_millis(100));
                        break;
                    }

                    std::thread::sleep(Duration::from_millis(10));
                }

                socket.set_nonblocking(false)?;

                if message_sent {
                    println!("Message sent, closing server.");
                } else {
                    println!("Timeout reached, no client message received. Closing server.");
                }

                break;
            }

            // Try to receive more packets (handshake continuation)
            if let Ok((len, from2)) = socket.recv_from(&mut buf) {
                let recv_info = quiche::RecvInfo { from: from2, to: socket.local_addr()? };
                let _ = conn.recv(&mut buf[..len], recv_info);

                while let Ok((write, send_info)) = conn.send(&mut out) {
                    socket.send_to(&out[..write], send_info.to)?;
                }
            }
        }

        break;
    }

    Ok(())
}
