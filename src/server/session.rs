// Server session management

use super::connection::ServerConnection;
use super::streams::StreamManager;
use super::sender::DataSender;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

const SESSION_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Manages a complete session with a client
pub struct ServerSession<'a> {
    connection: &'a mut ServerConnection,
    stream_manager: StreamManager,
    data_sender: DataSender,
    message_sent: bool,
}

impl<'a> ServerSession<'a> {
    /// Create a new server session
    pub fn new(connection: &'a mut ServerConnection) -> Self {
        Self {
            connection,
            stream_manager: StreamManager::new(),
            data_sender: DataSender::new(),
            message_sent: false,
        }
    }

    /// Run the session until completion or timeout
    pub fn run(
        &mut self,
        socket: &UdpSocket,
        buf: &mut [u8],
        out: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Complete handshake first
        self.complete_handshake(socket, buf, out)?;

        if !self.connection.is_established() {
            return Err("Failed to establish connection".into());
        }

        println!("Connection established, initializing streams...");

        // Initialize 4 streams for this connection
        self.stream_manager.initialize_streams(self.connection)?;

        // Handle application data exchange
        self.handle_application_data(socket, buf, out)?;

        Ok(())
    }

    /// Complete the QUIC handshake
    fn complete_handshake(
        &mut self,
        socket: &UdpSocket,
        buf: &mut [u8],
        out: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        socket.set_nonblocking(true)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        
        println!("Server: completing handshake...");
        while !self.connection.is_established() && Instant::now() < deadline {
            // Try to receive packets
            match socket.recv_from(buf) {
                Ok((len, from)) => {
                    println!("Server: handshake recv {} bytes", len);
                    let to = socket.local_addr()?;
                    self.connection.process_packet(&mut buf[..len], from, to)?;
                    self.connection.send_packets(socket, out)?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No data available, send any pending packets
                    self.connection.send_packets(socket, out)?;
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => return Err(e.into()),
            }
        }
        
        if self.connection.is_established() {
            println!("Server: handshake complete!");
        } else {
            return Err("Handshake timeout".into());
        }
        
        Ok(())
    }

    /// Handle application data exchange
    fn handle_application_data(
        &mut self,
        socket: &UdpSocket,
        buf: &mut [u8],
        out: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        socket.set_nonblocking(true)?;
        let deadline = Instant::now() + SESSION_TIMEOUT;

        while Instant::now() < deadline && !self.connection.is_closed() {
            // Receive packets
            if let Ok((len, from)) = socket.recv_from(buf) {
                println!("Server: recv {} bytes from {}", len, from);
                let to = socket.local_addr()?;
                match self.connection.process_packet(&mut buf[..len], from, to) {
                    Ok(_) => {}
                    Err(e) => eprintln!("Server: packet processing error: {:?}", e),
                }
            }

            // Process readable streams
            self.process_readable_streams(buf)?;

            // Send any pending packets
            self.connection.send_packets(socket, out)?;

            // Exit if message was sent and flushed
            if self.message_sent {
                std::thread::sleep(Duration::from_millis(100));
                break;
            }

            std::thread::sleep(POLL_INTERVAL);
        }

        socket.set_nonblocking(false)?;

        if self.message_sent {
            println!("Message sent, closing server.");
        } else {
            println!("Timeout reached, no client message received. Closing server.");
        }

        Ok(())
    }

    /// Process all readable streams
    fn process_readable_streams(
        &mut self,
        buf: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let readable: Vec<u64> = self.connection.readable().collect();
        
        if !readable.is_empty() {
            println!("Server: conn.readable() -> {:?}", readable);
        }

        for stream_id in readable {
            self.handle_stream_data(stream_id, buf)?;
        }

        Ok(())
    }

    /// Handle data from a specific stream
    fn handle_stream_data(
        &mut self,
        stream_id: u64,
        buf: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            match self.connection.stream_recv(stream_id, buf) {
                Ok((read, fin)) => {
                    if read == 0 {
                        break;
                    }

                    let msg = String::from_utf8_lossy(&buf[..read]);
                    println!("Server received on stream {}: {}", stream_id, msg);

                    // Send response using DataSender
                    let reply = b"Hello from QUIC server!";
                    self.data_sender.send_data(
                        self.connection,
                        stream_id,
                        reply,
                        fin,
                    )?;
                    self.message_sent = true;

                    if fin {
                        break;
                    }
                }
                Err(quiche::Error::Done) => break,
                Err(e) => {
                    eprintln!("Server: stream_recv error on {}: {:?}", stream_id, e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Get the stream manager
    pub fn stream_manager(&self) -> &StreamManager {
        &self.stream_manager
    }

    /// Get the data sender
    pub fn data_sender(&self) -> &DataSender {
        &self.data_sender
    }
}
