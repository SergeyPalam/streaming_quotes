use super::timer::Timer;
use anyhow::{Result, bail};
use rand::prelude::*;
use rand_distr::{Normal, StandardUniform};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Display;
use std::sync::mpsc::{self, channel};
use std::thread::{self, JoinHandle};

const HANDLE_CMD_PERIOD_MILLIS: u64 = 100;
const WAIT_CALLBACK_MILLIS: u64 = 100;

const WAIT_CMD_EVENT: &str = "cmd";
const WAIT_CALLBACK_EVENT: &str = "callback";

/// Треит, представляющий колбэк, передающийся генератору для обработки сгенерированных котировок
pub trait QuoteCallback {
    /// Обработка котировок
    fn handle(self, quotes: Vec<StockQuote>) -> Result<()>;
}

/// Управление генератором котировок
pub enum GeneratorCmd {
    /// Остановить генератор
    Stop,
}

#[derive(Serialize, Deserialize, Default, Debug)]
/// Информация о котировке
pub struct StockQuote {
    /// Короткое название фин. инструмента
    pub ticker: String,
    /// Текущаяя цена
    pub price: f64,
    /// Текущий объем
    pub volume: u32,
    /// Временная метка
    pub timestamp: u64,
}

impl Display for StockQuote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "T: {}, P: {:.4}, V: {}, TIME: {}",
            self.ticker, self.price, self.volume, self.timestamp
        )
    }
}

struct Ticker {
    upper_bound_price: f64,
    upper_bound_volume: u32,
    lower_bound_volume: u32,
    current_price: f64,
}

impl Ticker {
    fn from_json(json: Value) -> Option<Ticker> {
        let upper_bound_price = json["upper_bound_price"].as_f64()?;
        Some(Ticker {
            upper_bound_price,
            upper_bound_volume: json["upper_bound_volume"].as_u64()? as u32,
            lower_bound_volume: json["lower_bound_volume"].as_u64()? as u32,
            current_price: upper_bound_price / 2.0,
        })
    }
}

impl Ticker {
    fn price_range(&self) -> f64 {
        self.upper_bound_price
    }
    fn volume_range(&self) -> u32 {
        self.upper_bound_volume - self.lower_bound_volume
    }
}

/// Управление потоком генератора
pub struct GeneratorControl {
    /// Интерфейс отправки команд управления генератором
    pub tx: mpsc::Sender<GeneratorCmd>,
    /// Дескриптор потока генератора
    pub handle: JoinHandle<Result<()>>,
}

/// Канал передачи колбэка генератору
#[derive(Clone)]
pub struct CallbackSender<T>
where
    T: QuoteCallback + Send + 'static,
{
    /// Интерфейс отпраки колбэка
    pub tx: mpsc::Sender<T>,
}

/// Генератор котировок, использующий нормальное распределение для цены
/// и равномерное распределение для объема
pub struct QuoteGenerator {
    tickers: HashMap<String, Ticker>,
    timestamp_counter: u64,
    normal_distr: Normal<f64>,
}

impl QuoteGenerator {
    /// Создать новый генератор с указанием пути к конфигурации json
    /// ```
    /// [
    ///     {
    ///         "name": "AMD",
    ///         "upper_bound_price": 1000.0,
    ///         "upper_bound_volume": 1000000,
    ///         "lower_bound_volume": 1000
    ///     },
    ///     {
    ///         "name": "INT",
    ///         "upper_bound_price": 2000.0,
    ///         "upper_bound_volume": 2000000,
    ///         "lower_bound_volume": 1000
    ///     }
    ///]
    /// ```
    pub fn new(config_path: &str) -> Result<Self> {
        let json_str = std::fs::read_to_string(config_path)?;
        let json = serde_json::from_str::<Vec<Value>>(&json_str)?;
        let mut tickers = HashMap::new();

        for ticker_json in json {
            let ticker_name = if let Some(val) = ticker_json["name"].as_str() {
                val.to_string()
            } else {
                bail!("Can't read ticker name from config: {json_str}");
            };
            let ticker = if let Some(val) = Ticker::from_json(ticker_json) {
                val
            } else {
                bail!("Can't read ticker params from config: {json_str}");
            };
            tickers.insert(ticker_name, ticker);
        }
        Ok(Self {
            tickers,
            timestamp_counter: 1,
            normal_distr: Normal::new(0.0, 0.5)?,
        })
    }

    fn generate_quotes(&mut self) -> Vec<StockQuote> {
        let mut quotes = Vec::new();
        for (name, ticker) in self.tickers.iter_mut() {
            let mut quote = StockQuote::default();
            quote.ticker = name.to_string();

            quote.timestamp = self.timestamp_counter;

            let val_price: f64 = rand::rng().sample(self.normal_distr);
            quote.price = ticker.current_price + (ticker.price_range() / 64.0) * val_price;
            if quote.price < 0.0 {
                quote.price = 0.0;
            }
            if quote.price > ticker.upper_bound_price {
                quote.price = ticker.upper_bound_price;
            }
            ticker.current_price = quote.price;

            let val_volume: u32 = rand::rng().sample(StandardUniform);
            quote.volume = val_volume % ticker.volume_range() + ticker.lower_bound_volume;
            quotes.push(quote);
        }
        self.timestamp_counter += 1;
        quotes
    }

    /// Генерация котировки по выбранному тикеру
    pub fn start_generate_quote<T>(mut self) -> (GeneratorControl, CallbackSender<T>)
    where
        T: QuoteCallback + Send + 'static,
    {
        let (tx_cmd, rx_cmd): (mpsc::Sender<GeneratorCmd>, mpsc::Receiver<GeneratorCmd>) =
            channel();
        let (tx_callback, rx_callback): (mpsc::Sender<T>, mpsc::Receiver<T>) = channel();
        let handle = thread::spawn(move || {
            let mut timer = Timer::default();
            timer.add_event(WAIT_CMD_EVENT, HANDLE_CMD_PERIOD_MILLIS);
            timer.add_event(WAIT_CALLBACK_EVENT, WAIT_CALLBACK_MILLIS);

            loop {
                timer.sleep();

                if timer.is_expired_event(WAIT_CMD_EVENT)? {
                    timer.reset_event(WAIT_CMD_EVENT)?;
                    match rx_cmd.try_recv() {
                        Ok(cmd) => match cmd {
                            GeneratorCmd::Stop => {
                                break;
                            }
                        },
                        Err(e) => match e {
                            mpsc::TryRecvError::Disconnected => {
                                log::warn!("Parent thread is died");
                                break;
                            }
                            mpsc::TryRecvError::Empty => {}
                        },
                    }
                }

                if timer.is_expired_event(WAIT_CALLBACK_EVENT)? {
                    timer.reset_event(WAIT_CALLBACK_EVENT)?;
                    match rx_callback.try_recv() {
                        Ok(callback) => {
                            let _ = callback.handle(self.generate_quotes());
                        }
                        Err(_) => continue,
                    }
                }
            }

            log::info!("Quotes generator stopped");
            Ok(())
        });

        (
            GeneratorControl { tx: tx_cmd, handle },
            CallbackSender { tx: tx_callback },
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;

    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    const EPSILON: f64 = 1e-6;

    #[test]
    fn test_ticker_from_json() {
        let val = json!({
            "upper_bound_price" : 2.0,
            "upper_bound_volume" : 10,
            "lower_bound_volume" : 2,
        });
        let ticker = Ticker::from_json(val).unwrap();
        assert!((ticker.upper_bound_price - 2.0).abs() < EPSILON);
        assert_eq!(ticker.upper_bound_volume, 10);
        assert_eq!(ticker.lower_bound_volume, 2);
    }

    #[test]
    fn test_quotes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.txt");
        let mut file = File::create(&path).unwrap();
        let config = json!([
            {
                "name": "AMD",
                "upper_bound_price": 1000.0,
                "upper_bound_volume": 1000000,
                "lower_bound_volume": 1000
            },
            {
                "name": "INT",
                "upper_bound_price": 2000.0,
                "upper_bound_volume": 2000000,
                "lower_bound_volume": 1000
            }
        ])
        .to_string();
        file.write_all(config.as_bytes()).unwrap();
        file.flush().unwrap();

        let mut generator = QuoteGenerator::new(path.to_str().unwrap()).unwrap();
        let quotes = generator.generate_quotes();
        assert!(quotes.iter().any(|item| item.ticker == "AMD"));
        assert!(quotes.iter().any(|item| item.ticker == "INT"));
        assert!(!quotes.iter().any(|item| item.ticker == "GAZ"));
    }
}
