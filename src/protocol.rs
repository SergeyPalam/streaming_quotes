use serde::{Serialize, Deserialize};
use super::quote::StockQuote;
use std::net::SocketAddr;
use postcard::to_stdvec;
use anyhow::Result;

#[derive(Serialize, Deserialize)]
pub struct QuoteRespMessage {
    pub quote: StockQuote,
}

#[derive(Serialize, Deserialize)]
pub struct TickerReqMessage {
    pub recv_addr: SocketAddr,
    pub tickers: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub enum Message {
    Quote(QuoteRespMessage),
    Tickers(TickerReqMessage),
    Ping,
    Pong,
    Unknown,
}

// for stream handling
pub fn pack_message_with_len<T: Serialize>(msg: &T) -> Result<Vec<u8>> {
    let mut bin_msg = to_stdvec(&msg)?;
    let msg_len = (bin_msg.len() as u32).to_be_bytes();
    let mut res = msg_len.to_vec();
    res.append(&mut bin_msg);
    Ok(bin_msg)
}
