use crate::protocol::*;
use crate::timer::Timer;
use anyhow::{Result, bail};
use std::fmt::Display;
use std::io::BufReader;
use std::io::{BufRead, ErrorKind, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;

const PING_PERIOD_MILLIS: u64 = 30000;
const WAIT_PONG_MILLIS: u64 = 5000;
const HANDLE_CMD_PERIOD_MILLIS: u64 = 300;
const WAIT_QUOTES_MILLIS: u64 = 100;

const WAIT_PING_EVENT: &str = "ping";
const WAIT_PONG_EVENT: &str = "pong";
const WAIT_CMD_EVENT: &str = "cmd";
const WAIT_QUOTES_EVENT: &str = "quotes";

/// Команды управления клиентом
pub enum ClientCmd {
    /// Остановить клиент
    Stop,
}

fn is_stop_cmd(rx: &mpsc::Receiver<ClientCmd>) -> bool {
    match rx.try_recv() {
        Ok(cmd) => match cmd {
            ClientCmd::Stop => return true,
        },
        Err(e) => match e {
            TryRecvError::Disconnected => {
                log::warn!("Parent thread is died");
                return true;
            }
            TryRecvError::Empty => return false,
        },
    }
}

struct PingControl {
    thread_handle: thread::JoinHandle<Result<()>>,
    tx: mpsc::Sender<ClientCmd>,
}

enum PingState {
    WaitPing,
    WaitPong,
}

struct PingPong {
    server_addr: SocketAddr,
}

impl PingPong {
    fn new(server_addr: SocketAddr) -> Self {
        Self { server_addr }
    }

    fn ping(&self, sock: &UdpSocket) -> Result<()> {
        let bin_ping = postcard::to_stdvec(&Message::Ping)?;
        sock.send_to(&bin_ping, self.server_addr)?;
        log::info!("PING");
        Ok(())
    }

    fn is_pong_received(&self, sock: &UdpSocket) -> bool {
        let mut recv_buf = [0u8; MAX_SIZE_DATAGRAM];
        let (pack_len, server_addr) = match sock.recv_from(&mut recv_buf) {
            Ok(len) => len,
            Err(_) => return false,
        };

        if self.server_addr != server_addr {
            return false;
        }

        let msg = match postcard::from_bytes::<Message>(&recv_buf[..pack_len]) {
            Ok(msg) => msg,
            Err(_) => return false,
        };
        match msg {
            Message::Pong => {
                log::info!("PONG");
                return true;
            }
            _ => {
                log::warn!("Wrong response");
                return false;
            }
        }
    }

    fn start(self) -> Result<PingControl> {
        let udp_sock = UdpSocket::bind("127.0.0.1:5433")?;
        udp_sock.set_nonblocking(true)?;
        log::info!("Ping pong start to server: {}", self.server_addr);
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let mut state = PingState::WaitPing;
            let mut timer = Timer::default();
            timer.add_event(WAIT_PING_EVENT, PING_PERIOD_MILLIS);
            timer.add_event(WAIT_CMD_EVENT, HANDLE_CMD_PERIOD_MILLIS);

            loop {
                timer.sleep();
                if timer.is_expired_event(WAIT_CMD_EVENT)? {
                    timer.reset_event(WAIT_CMD_EVENT)?;
                    if is_stop_cmd(&rx) {
                        log::debug!("Stop ping from stop cmd");
                        break;
                    }
                }

                match state {
                    PingState::WaitPing => {
                        if timer.is_expired_event(WAIT_PING_EVENT)? {
                            self.ping(&udp_sock)?;
                            timer.remove_event(WAIT_PING_EVENT)?;
                            timer.add_event(WAIT_PONG_EVENT, WAIT_PONG_MILLIS);
                            state = PingState::WaitPong;
                        }
                    }
                    PingState::WaitPong => {
                        if timer.is_expired_event(WAIT_PONG_EVENT)? {
                            if !self.is_pong_received(&udp_sock) {
                                log::info!("Pong doesn't received");
                                break;
                            }
                            timer.remove_event(WAIT_PONG_EVENT)?;
                            timer.add_event(WAIT_PING_EVENT, PING_PERIOD_MILLIS);
                            state = PingState::WaitPing;
                        }
                    }
                }
            }

            log::info!("Ping pong finish");
            Ok(())
        });
        Ok(PingControl {
            thread_handle: handle,
            tx,
        })
    }
}

/// Интерфейс управления потоком клиента
pub struct ClientControl {
    /// Отправка команды потоку-клиента
    pub tx: mpsc::Sender<ClientCmd>,
    /// Дескриптор потока-клиента
    pub thread_handle: thread::JoinHandle<Result<()>>,
}

/// Клиент приёма котировок
#[derive(Debug)]
pub struct QuotesClient {
    server_addr: SocketAddr,
    recv_quote_port: u16,
    tickers: Vec<String>,
}

impl Display for QuotesClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "serv addr: {}, receive quotes port: {}",
            self.server_addr, self.recv_quote_port
        )?;
        writeln!(f, "Tickers:")?;
        for ticker in self.tickers.iter() {
            writeln!(f, "{ticker}")?;
        }
        Ok(())
    }
}

impl QuotesClient {
    /// Создаёт новый клиент котировок:
    /// server_addr - ip-алрес сервера для подключения по tcp
    /// recv_quote_port - Порт для приема котировок
    /// tickers_path - Путь к файлу с котировками в формате:
    ///
    /// TICKER1
    /// TICKER2
    pub fn new(server_addr: &str, recv_quote_port: u16, tickers_path: &str) -> Result<Self> {
        let file = std::fs::File::open(tickers_path)?;
        let read_buf = BufReader::new(file);
        let mut tickers = Vec::new();
        for line in read_buf.lines() {
            let line = line?.trim().to_string();
            if line.is_empty() {
                continue;
            }
            tickers.push(line);
        }

        Ok(Self {
            server_addr: server_addr.parse()?,
            recv_quote_port,
            tickers,
        })
    }

    fn recv_quotes(sock: &UdpSocket, ping_control: &mut Option<PingControl>) -> Result<()> {
        let mut recv_buf = [0u8; MAX_SIZE_DATAGRAM];
        let (pack_len, server_addr) = match sock.recv_from(&mut recv_buf) {
            Ok((len, addr)) => (len, addr),
            Err(e) => match e.kind() {
                ErrorKind::WouldBlock => return Ok(()),
                _ => bail!("{e}"),
            },
        };

        if let Some(control) = ping_control.as_ref() {
            if control.thread_handle.is_finished() {
                bail!("Server at address {server_addr} doesn't response");
            }
        } else {
            let control = match PingPong::new(server_addr).start() {
                Ok(val) => val,
                Err(e) => {
                    bail!("Can't start ping pong logic: {e}");
                }
            };
            *ping_control = Some(control);
        }

        let msg = postcard::from_bytes::<Message>(&recv_buf[..pack_len])?;
        let quotes = match msg {
            Message::Quote(quotes) => quotes,
            _ => {
                log::warn!("Wrong response");
                return Ok(());
            }
        };
        println!("{}", quotes.quote);
        Ok(())
    }

    /// Запуск потока приёма котировок
    pub fn start_receive_quotes(self) -> Result<ClientControl> {
        let (tx, rx) = mpsc::channel();
        let udp_addr = SocketAddr::from(([127, 0, 0, 1], self.recv_quote_port));
        let udp_sock = UdpSocket::bind(udp_addr)?;
        log::info!("Start receive quotes at addr: {udp_addr}");
        udp_sock.set_nonblocking(true)?;

        let mut stream = TcpStream::connect(self.server_addr)?;
        let ticker_req = Message::Tickers(TickerReqMessage {
            port: self.recv_quote_port,
            tickers: self.tickers.clone(),
        });

        log::debug!("Request tickers: {:?}", ticker_req);

        let bin_req = pack_message_with_len(&ticker_req)?;
        log::debug!("Pack message len: {}", bin_req.len());
        stream.write_all(&bin_req)?;

        let handle = std::thread::spawn(move || {
            let mut ping_control: Option<PingControl> = None;
            let mut timer = Timer::default();
            timer.add_event(WAIT_QUOTES_EVENT, WAIT_QUOTES_MILLIS);
            timer.add_event(WAIT_CMD_EVENT, HANDLE_CMD_PERIOD_MILLIS);
            loop {
                timer.sleep();
                if timer.is_expired_event(WAIT_CMD_EVENT)? {
                    timer.reset_event(WAIT_CMD_EVENT)?;
                    if is_stop_cmd(&rx) {
                        log::debug!("Stop cmd");
                        break;
                    }
                }

                if timer.is_expired_event(WAIT_QUOTES_EVENT)? {
                    timer.reset_event(WAIT_QUOTES_EVENT)?;
                    if let Err(e) = Self::recv_quotes(&udp_sock, &mut ping_control) {
                        log::error!("Can't receive quotes: {e}");
                        break;
                    }
                }
            }

            let res = if let Some(control) = ping_control {
                control.tx.send(ClientCmd::Stop)?;
                match control.thread_handle.join() {
                    Ok(res) => res,
                    Err(_) => {
                        bail!("Can't join thread");
                    }
                }
            } else {
                Ok(())
            };

            log::info!("Stop receive quotes");
            res
        });

        Ok(ClientControl {
            thread_handle: handle,
            tx,
        })
    }
}
