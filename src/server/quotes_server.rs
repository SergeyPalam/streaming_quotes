use std::io::Read;
use std::net::{TcpListener, TcpStream, SocketAddr, UdpSocket};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, mpsc::{TryRecvError}};
use std::thread;
use crate::quote::{QuoteGenerator, StockQuote};
use crate::protocol::{Message, QuoteRespMessage, TickerReqMessage};
use anyhow::{Result, bail};

const PING_TIMER_SEC: u64 = 30;
const STREAMING_TIMEOUT_MILLIS: u64 = 1000;
const CONNECTION_TIMEOUT_MILLIS: u64 = 100;
const MAX_SIZE_DATAGRAM: usize = 100;

pub enum ControlCmd{
    Stop,
    Quotes(TickerReqMessage),
    Noop,
}

impl ControlCmd {
    fn from_channel(rx: &mpsc::Receiver<ControlCmd>) -> Result<Self> {
        let cmd = 
        match rx.try_recv(){
            Ok(cmd) => cmd,
            Err(e) => {
                match e {
                    TryRecvError::Disconnected => {
                        bail!("Close connection: {e}");
                    }
                    TryRecvError::Empty => {
                        ControlCmd::Noop
                    }
                }
            }
        };
        Ok(cmd)
    }
}

struct QuotesStream {
    quote_generator: Arc<Mutex<QuoteGenerator>>,
}

impl QuotesStream {
    fn new(quote_generator: Arc<Mutex<QuoteGenerator>>) -> Self {
        Self {
            quote_generator,
        }
    }

    fn check_ping(&self, socket: &UdpSocket) -> Result<()> {
        let mut recv_buf = [0u8; MAX_SIZE_DATAGRAM];
        let (pack_len, client_addr) = socket.recv_from(&mut recv_buf)?;
        if pack_len == 0 {
            return Ok(())
        }

        let msg = postcard::from_bytes::<Message>(&recv_buf[..pack_len])?;
        match msg {
            Message::Ping => log::info!("PING"),
            _ => bail!("Wrong message"),
        }

        let bin_pong = postcard::to_stdvec(&Message::Pong)?;
        socket.send_to(&bin_pong, client_addr)?;
        log::info!("PONG");

        Ok(())
    }

    fn send_quote(&self, socket: &UdpSocket, addr: SocketAddr, quote: Option<StockQuote>) -> Result<()> {
        let quote_msg =
        if let Some(val) = quote {
            Message::Quote(QuoteRespMessage{
                quote: val
            })
        }else{
            Message::Unknown
        };

        let bin_msg = postcard::to_stdvec(&quote_msg)?;
        let _= socket.send_to(&bin_msg, addr)?;
        Ok(())
    }

    fn start(self) -> QuotesStreamControl {
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move ||{
            let socket =
            match UdpSocket::bind("127.0.0.1:34254"){
                Ok(sock) => sock,
                Err(e) => {
                    log::error!("Can't bind udp socket: {e}");
                    return;
                }
            };

            if let Err(e) = socket.set_nonblocking(true){
                log::error!("Can't streaming read timeout: {e}");
                return;
            }

            let mut need_quotes = Vec::new();
            let mut cur_udp_addr = None;
            let mut sleep_counter = 0;
            loop {
                thread::sleep(std::time::Duration::from_millis(CONNECTION_TIMEOUT_MILLIS));
                sleep_counter += 1;
                
                let cmd =
                match ControlCmd::from_channel(&rx) {
                    Ok(val) => val,
                    Err(e) => {
                        log::info!("{e}");
                        break;
                    }
                };

                match cmd {
                    ControlCmd::Stop => {
                        log::info!("Quotes streaming has stopped");
                        break;
                    }
                    ControlCmd::Quotes(req) => {
                        cur_udp_addr = Some(req.recv_addr);
                        need_quotes = req.tickers;
                    }
                    _ => {}
                }

                let client_addr =
                if let Some(addr) = cur_udp_addr {
                    addr
                }else{
                    continue;
                };

                if let Err(e) = self.check_ping(&socket) {
                    log::error!("Ping error: {e}");
                    break;
                }
                
                if sleep_counter >= STREAMING_TIMEOUT_MILLIS / CONNECTION_TIMEOUT_MILLIS {
                    sleep_counter = 0;
                    for need_quote in need_quotes.iter() {
                        let quote = self.quote_generator.lock().unwrap().generate_quote(need_quote.as_str());
                        if let Err(e) = self.send_quote(&socket, client_addr, quote) {
                            log::warn!("Send quote error: {e}");
                        }
                    }
                }
            }

            log::info!("Close stream");
        });
        QuotesStreamControl {
            tx,
            thread_handle: handle,
        }
    }
}

struct QuotesStreamControl {
    tx: mpsc::Sender<ControlCmd>,
    thread_handle: thread::JoinHandle<()>,
}

struct CommandHandler {
    conn: TcpStream,
    client_addr: SocketAddr,
}

struct HanlerControl {
    tx: mpsc::Sender<ControlCmd>,
    thread_handle: thread::JoinHandle<()>,
}

impl CommandHandler {
    fn new(connection: TcpStream, client_addr: SocketAddr) -> Result<Self> {
        connection.set_write_timeout(Some(std::time::Duration::from_millis(CONNECTION_TIMEOUT_MILLIS)))?;
        Ok(Self {
            conn: connection,
            client_addr,
        })
    }

    fn read_pack(&mut self, len: u32) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len as usize];
        self.conn.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn start(mut self, quote_generator: Arc<Mutex<QuoteGenerator>>) -> HanlerControl {
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move ||{
            let qoutes_stream_control = QuotesStream::new(quote_generator).start();
            
            loop {
                let cmd =
                match ControlCmd::from_channel(&rx) {
                    Ok(val) => val,
                    Err(e) => {
                        log::info!("{e}");
                        break;
                    }
                };

                match cmd {
                    ControlCmd::Stop => {
                        log::info!("Connection handler stopped");
                        break;
                    }
                    _ => {}
                }
                
                let mut len = [0u8; 4];
                if let Err(e) = self.conn.read(&mut len) {
                    log::warn!("Connection closed: {e}");
                    break;
                }

                let len = u32::from_be_bytes(len);
                let bin_message =
                match self.read_pack(len) {
                    Ok(val) => val,
                    Err(e) => {
                        log::warn!("Connection closed: {e}");
                        break;
                    }
                };

                let req_tickers =
                match postcard::from_bytes::<Message>(&bin_message) {
                    Ok(msg) => {
                        if let Message::Tickers(req) = msg{
                            req
                        }else{
                            log::warn!("Wrong request");
                            break;    
                        }
                    }
                    Err(e) => {
                        log::warn!("Wrong request: {e}");
                        break;
                    }
                };
                if let Err(e) = qoutes_stream_control.tx.send(ControlCmd::Quotes(req_tickers)) {
                    let _ = qoutes_stream_control.tx.send(ControlCmd::Stop);
                    log::warn!("{e}");
                    break;
                }
            }

            let _ = qoutes_stream_control.tx.send(ControlCmd::Stop);
            qoutes_stream_control.thread_handle.join();
            log::info!("Close connection {}", self.client_addr);
        });
        HanlerControl {
            tx,
            thread_handle: handle,
        }
    }
}

pub struct ServerControl {
    pub tx: mpsc::Sender<ControlCmd>,
    pub thread_handle: thread::JoinHandle<()>,
}

pub struct QuotesServer {
    quotes_generator: Arc<Mutex<QuoteGenerator>>,
}

impl QuotesServer {
    pub fn new(config_path: &str) -> Result<Self> {
        let generator = Arc::new(Mutex::new(QuoteGenerator::new(config_path)?));
        Ok(Self{
            quotes_generator: generator,
        })
    }

    pub fn start(self) -> Result<ServerControl> {
        let listener = TcpListener::bind("127.0.0.1:80")?;
        listener.set_nonblocking(true)?;

        log::info!("Quotes streaming server is started");        
        let (tx, rx) = mpsc::channel();
        
        let handle = thread::spawn(move ||{
            let mut handlers = Vec::new();

            loop {
                thread::sleep(std::time::Duration::from_millis(CONNECTION_TIMEOUT_MILLIS));
                let cmd =
                match ControlCmd::from_channel(&rx) {
                    Ok(val) => val,
                    Err(e) => {
                        log::info!("{e}");
                        break;
                    }
                };

                match cmd {
                    ControlCmd::Stop => {
                        log::info!("Connection handler stopped");
                        break;
                    }
                    _ => {}
                }

                let (connection, addr) =
                match listener.accept() {
                    Ok((conn, addr)) => (conn, addr),
                    Err(e) => {
                        match e.kind() {
                            std::io::ErrorKind::WouldBlock => {
                                continue;
                            }
                            _ => {
                                log::error!("Can't accept connection");
                                break;
                            }
                        }
                    }
                };

                let handler =
                match CommandHandler::new(connection, addr) {
                    Ok(val) => val.start(self.quotes_generator.clone()),
                    Err(e) => {
                        log::error!("Can't handle connection: {e}");
                        break;
                    }
                };

                handlers.push(handler);
            }

            for handler in handlers {
                handler.tx.send(ControlCmd::Stop);
                handler.thread_handle.join();
            }
            log::info!("Server is stopped");
        }
        );
        Ok(ServerControl{
            tx,
            thread_handle: handle,
        })
    }
}
