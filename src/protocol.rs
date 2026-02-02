use serde::{Serialize, Deserialize};
use super::quote::StockQuote;
use postcard::to_stdvec;
use anyhow::Result;

pub const MAX_SIZE_DATAGRAM: usize = 100;

#[derive(Serialize, Deserialize, Debug)]
pub struct QuoteRespMessage {
    pub quote: StockQuote,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TickerReqMessage {
    pub port: u16,
    pub tickers: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
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
    Ok(res)
}
