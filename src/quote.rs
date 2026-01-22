use serde::{Serialize, Deserialize};
use rand::prelude::*;
use rand_distr::{StandardNormal, StandardUniform};

#[derive(Serialize, Deserialize, Default)]
pub struct StockQuote {
    pub ticker: String,
    pub price: f64,
    pub volume: u32,
    pub timestamp: u64,
}

struct Ticker {
    name: String,
    upper_bound_price: f64,
    upper_bound_volume: u32,
    lower_bound_volume: u32,
    current_price: f64,
}

impl Ticker {
    fn price_range(&self) -> f64{
        self.upper_bound_price
    }
    fn volume_range(&self) -> u32 {
        self.upper_bound_volume - self.lower_bound_volume
    }
}

pub struct QuoteGenerator {
    tickers: Vec<Ticker>,
    timestamp_counter: u64,
}

impl QuoteGenerator {
    pub fn new(config_path: &str) -> Self {
        Self {
            tickers: Vec::new(),
            timestamp_counter: 1,
        }
    }

    pub fn generate_quotes(&mut self, ticker: &str) -> Vec<StockQuote> {
        let mut res = Vec::new();
        for ticker in self.tickers.iter_mut() {
            let mut cur_quote = StockQuote::default();
            cur_quote.ticker = ticker.name.clone();

            cur_quote.timestamp = self.timestamp_counter;
            self.timestamp_counter += 1;

            let val_price: f64 = rand::rng().sample(StandardNormal);
            cur_quote.price = cur_quote.price + (ticker.price_range() / 8.0) * val_price;
            if cur_quote.price < 0.0 {
                cur_quote.price = 0.0;
            }
            if cur_quote.price > ticker.upper_bound_price {
                cur_quote.price = ticker.upper_bound_price;
            }
            ticker.current_price = cur_quote.price;

            let val_volume: u32 = rand::rng().sample(StandardUniform);
            cur_quote.volume = val_volume % ticker.volume_range() + ticker.lower_bound_volume;
            res.push(cur_quote);
        }
        res
    }
}