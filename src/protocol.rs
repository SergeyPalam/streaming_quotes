use serde::{Serialize, Deserialize};
use super::quote::StockQuote;
use postcard::to_stdvec;
use anyhow::Result;

/// Максимальный размер датаграммы. Если пакет будет больше, то нужно учесть нумерацию пакетов
pub const MAX_SIZE_DATAGRAM: usize = 100;

#[derive(Serialize, Deserialize, Debug)]
/// Котировки ответ сервера
pub struct QuoteRespMessage {
    /// котировка
    pub quote: StockQuote,
}

#[derive(Serialize, Deserialize, Debug)]
/// Запрос котировок
pub struct TickerReqMessage {
    /// UDP порт, на который присылать котировки
    pub port: u16,
    /// Названия фин. инструментов, по которым необходимо получать котировки
    /// Эти инструменты должны быть в конфигурации сервера
    pub tickers: Vec<String>,
}

/// Типы сообщений в протоколе
#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    /// Котировка
    Quote(QuoteRespMessage),
    /// Запрос котировок
    Tickers(TickerReqMessage),
    /// Пинг
    Ping,
    /// Понг
    Pong,
    /// Не поддерживаемы тип
    Unknown,
}

/// Добавляет длину пакета перед самим бинарным пакетом.
/// Необходимо для потоковых протоколов
pub fn pack_message_with_len<T: Serialize>(msg: &T) -> Result<Vec<u8>> {
    let mut bin_msg = to_stdvec(&msg)?;
    let msg_len = (bin_msg.len() as u32).to_be_bytes();
    let mut res = msg_len.to_vec();
    res.append(&mut bin_msg);
    Ok(res)
}
