use std::net::{TcpListener, TcpStream, SocketAddr, UdpSocket, IpAddr};
use std::sync::mpsc::{self, Receiver, TryRecvError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use crate::quote::{QuoteGenerator, StockQuote};
use crate::protocol::*;
use crate::timer::Timer;
use crate::utils::StreamReader;
use anyhow::{Result, bail, anyhow};
use std::io::ErrorKind;

const STREAMING_TIMEOUT_MILLIS: u64 = 1000;
const CHECK_TCP_CMD_MILLIS: u64 = 100;
const HANDLE_CMD_PERIOD_MILLIS: u64 = 300;
const CHECK_PING_MILLIS: u64 = 100;
const ACCEPT_MILLIS: u64 = 100;

const STREAM_EVENT: &str = "stream";
const WAIT_CMD_EVENT: &str = "cmd";
const CHECK_PING_EVENT: &str = "check_ping";
const CHECK_TCP_CMD_EVENT: &str = "check_tcp_cmd";
const ACCEPT_EVENT: &str = "accept";

pub enum ControlCmd{
    Stop,
    Quotes(TickerReqMessage),
    Noop,
}

fn cmd_from_channel(rx: &mpsc::Receiver<ControlCmd>) -> ControlCmd {
    match rx.try_recv(){
        Ok(cmd) => cmd,
        Err(e) => {
            match e {
                TryRecvError::Disconnected => {
                    log::warn!("Parent thread is died");
                    return ControlCmd::Stop;
                }
                TryRecvError::Empty => return ControlCmd::Noop,
            }
        }
    }
}

struct QuotesStreamControl {
    tx: mpsc::Sender<ControlCmd>,
    thread_handle: thread::JoinHandle<Result<()>>,
}

struct QuotesStream {
    quote_generator: Arc<Mutex<QuoteGenerator>>,
    client_ip_addr: IpAddr,
}

impl QuotesStream {
    fn new(quote_generator: Arc<Mutex<QuoteGenerator>>, client_ip_addr: IpAddr) -> Self {
        Self {
            quote_generator,
            client_ip_addr,
        }
    }

    fn check_ping(&self, socket: &UdpSocket) -> Result<()> {
        let mut recv_buf = [0u8; MAX_SIZE_DATAGRAM];
        let (pack_len, client_addr) =
        match socket.recv_from(&mut recv_buf){
            Ok((len, addr)) => (len, addr),
            Err(e) => {
                match e.kind() {
                    ErrorKind::WouldBlock => return Ok(()),
                    _ => {
                        bail!("Can't read from socket: {e}");
                    }
                }
            }
        };

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

    fn send_quote(&self, socket: &UdpSocket, port: u16, quote: Option<StockQuote>) -> Result<()> {
        let quote_msg =
        if let Some(val) = quote {
            Message::Quote(QuoteRespMessage{
                quote: val
            })
        }else{
            Message::Unknown
        };

        let bin_msg = postcard::to_stdvec(&quote_msg)?;
        let _= socket.send_to(&bin_msg, SocketAddr::new(self.client_ip_addr, port))?;
        Ok(())
    }

    fn start(self) -> QuotesStreamControl {
        log::info!("Start streaming quotes");
        let (tx, rx): (Sender<ControlCmd>, Receiver<ControlCmd>) = mpsc::channel();
        let handle = thread::spawn(move ||{
            let socket = UdpSocket::bind("127.0.0.1:34254")?;
            socket.set_nonblocking(true)?;

            let mut need_quotes = Vec::new();
            let mut cur_client_port = None;
            let mut timer = Timer::default();
            timer.add_event(WAIT_CMD_EVENT, HANDLE_CMD_PERIOD_MILLIS);
            timer.add_event(STREAM_EVENT, STREAMING_TIMEOUT_MILLIS);
            timer.add_event(CHECK_PING_EVENT, CHECK_PING_MILLIS);

            loop {
                timer.sleep();

                if timer.is_expired_event(WAIT_CMD_EVENT)? {
                    timer.reset_event(WAIT_CMD_EVENT)?;
                    match cmd_from_channel(&rx) {
                        ControlCmd::Stop => {
                            log::info!("Stop streaming");
                            break
                        },
                        ControlCmd::Quotes(req) => {
                            log::debug!("Quotes request: {:?}", req);
                            cur_client_port = Some(req.port);
                            need_quotes = req.tickers;
                        }
                        ControlCmd::Noop => {}
                    }
                }

                if timer.is_expired_event(CHECK_PING_EVENT)? {
                    timer.reset_event(CHECK_PING_EVENT)?;
                    
                    if let Err(e) = self.check_ping(&socket){
                        log::error!("Check ping error: {e}");
                        break;
                    }
                }
                
                if timer.is_expired_event(STREAM_EVENT)? {
                    timer.reset_event(STREAM_EVENT)?;
                    if let Some(port) = cur_client_port {
                        for need_quote in need_quotes.iter() {
                            let quote = self.quote_generator.lock().unwrap().generate_quote(need_quote.as_str());
                            if let Err(e) = self.send_quote(&socket, port, quote){
                                log::error!("Send quote error: {e}");
                                break;
                            }
                        }
                    }
                }
            }

            log::info!("Close stream");
            Ok(())
        });
        QuotesStreamControl {
            tx,
            thread_handle: handle,
        }
    }
}

enum HandlerState {
    WaitPackLen,
    WaitPack(u32),
}

struct CommandHandler {
    conn: TcpStream,
    client_addr: SocketAddr,
}

struct HanlerControl {
    tx: mpsc::Sender<ControlCmd>,
    thread_handle: thread::JoinHandle<Result<()>>,
}

impl CommandHandler {
    fn new(connection: TcpStream, client_addr: SocketAddr) -> Result<Self> {
        connection.set_nonblocking(true)?;
        Ok(Self {
            conn: connection,
            client_addr,
        })
    }

    fn start(mut self, quote_generator: Arc<Mutex<QuoteGenerator>>) -> HanlerControl {
        let (tx, rx) = mpsc::channel();

        log::info!("Start new handler for quote requests");
        let handle = thread::spawn(move ||{
            let qoutes_stream_control = QuotesStream::new(quote_generator, self.client_addr.ip()).start();
            let mut state = HandlerState::WaitPackLen;
            let mut timer = Timer::default();
            timer.add_event(WAIT_CMD_EVENT, HANDLE_CMD_PERIOD_MILLIS);
            timer.add_event(CHECK_TCP_CMD_EVENT, CHECK_TCP_CMD_MILLIS);

            let mut stream_reader = StreamReader::default();

            loop {
                timer.sleep();

                if timer.is_expired_event(WAIT_CMD_EVENT)? {
                    timer.reset_event(WAIT_CMD_EVENT)?;
                    match cmd_from_channel(&rx) {
                        ControlCmd::Stop => {
                            log::debug!("Stop command received from Client handler");
                            break;
                        },
                        _ => {}
                    }
                }

                if timer.is_expired_event(CHECK_TCP_CMD_EVENT)? {
                    timer.reset_event(CHECK_TCP_CMD_EVENT)?;
                    match state {
                        HandlerState::WaitPackLen => {
                            if let Err(e) = stream_reader.read_from_stream(&mut self.conn){
                                log::info!("Connection error: {e}");
                                break;
                            }
                            let bin_len =
                            if let Some(val) = stream_reader.extract_chunk(4){
                                val
                            }else{
                                continue;
                            };

                            let len: [u8; 4]  = bin_len.try_into().map_err(|_| anyhow!("Parse error"))?;

                            log::debug!("Packet len is received: {}", u32::from_be_bytes(len.into()));
                            state = HandlerState::WaitPack(u32::from_be_bytes(len));
                        }
                        HandlerState::WaitPack(len) => {
                            if let Err(e) = stream_reader.read_from_stream(&mut self.conn){
                                log::info!("Connection error: {e}");
                                break;
                            }
                            let bin_message =
                            if let Some(val) = stream_reader.extract_chunk(len as usize){
                                val
                            }else{
                                log::error!("Can't receive full packet");
                                break;
                            };

                            let msg = postcard::from_bytes::<Message>(&bin_message)?;
                            log::debug!("Message: {:?}", msg);
                            let tickers =
                            match msg {
                                Message::Tickers(tickers) => tickers,
                                _ => break,
                            };

                            qoutes_stream_control.tx.send(ControlCmd::Quotes(tickers))?;
                            state = HandlerState::WaitPackLen;
                        }
                    }
                }
            }

            let _ = qoutes_stream_control.tx.send(ControlCmd::Stop);
            let res =
            match qoutes_stream_control.thread_handle.join(){
                Ok(val) => val,
                Err(_) => {
                    bail!("Can't join thread");
                }
            };
            log::info!("Close connection {}", self.client_addr);
            res
        });
        HanlerControl {
            tx,
            thread_handle: handle,
        }
    }
}

pub struct ServerControl {
    pub tx: mpsc::Sender<ControlCmd>,
    pub thread_handle: thread::JoinHandle<Result<()>>,
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
            let mut timer = Timer::default();
            timer.add_event(WAIT_CMD_EVENT, HANDLE_CMD_PERIOD_MILLIS);
            timer.add_event(ACCEPT_EVENT, ACCEPT_MILLIS);

            loop {
                timer.sleep();
                if timer.is_expired_event(WAIT_CMD_EVENT)? {
                    timer.reset_event(WAIT_CMD_EVENT)?;
                    match cmd_from_channel(&rx) {
                        ControlCmd::Stop => {
                            log::debug!("Stop command received in quote server");
                            break;
                        },
                        _ => {}
                    }
                }

                if timer.is_expired_event(ACCEPT_EVENT)? {
                    let (connection, addr) =
                    match listener.accept() {
                        Ok((conn, addr)) => {
                            log::debug!("Accept new connection from address: {addr}");
                            (conn, addr)
                        },
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
            }

            for handler in handlers {
                handler.tx.send(ControlCmd::Stop)?;
                match handler.thread_handle.join(){
                    Ok(res) => {
                        if res.is_err() {
                            return res;
                        }
                    }
                    Err(_) => {
                        bail!("Can't join thread");
                    }
                }
            }
            log::info!("Server is stopped");
            Ok(())
        }
        );
        Ok(ServerControl{
            tx,
            thread_handle: handle,
        })
    }
}
