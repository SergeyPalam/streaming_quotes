use serde::{Serialize, Deserialize};
use serde_json::Value;
use rand::prelude::*;
use rand_distr::{StandardNormal, StandardUniform};
use std::collections::HashMap;
use anyhow::{bail, Result};

#[derive(Serialize, Deserialize, Default)]
pub struct StockQuote {
    pub ticker: String,
    pub price: f64,
    pub volume: u32,
    pub timestamp: u64,
}

struct Ticker {
    upper_bound_price: f64,
    upper_bound_volume: u32,
    lower_bound_volume: u32,
    current_price: f64,
}

impl Ticker {
    fn from_json(json: Value) -> Option<Ticker> {
        Some(Ticker {
            upper_bound_price: json["upper_bound_price"].as_f64()?,
            upper_bound_volume: json["upper_bound_volume"].as_u64()? as u32,
            lower_bound_volume: json["lower_bound_volume"].as_u64()? as u32,
            current_price: 0.0,
        })
    }
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
    tickers: HashMap<String, Ticker>,
    timestamp_counter: u64,
}

impl QuoteGenerator {
    pub fn new(config_path: &str) -> Result<Self> {
        let json_str = std::fs::read_to_string(config_path)?;
        let json = serde_json::from_str::<Vec<Value>>(&json_str)?;
        let mut tickers = HashMap::new();

        for ticker_json in json {
            let ticker_name =
            if let Some(val) = ticker_json["name"].as_str() {
                val.to_string()
            }else{
                bail!("Can't read ticker name from config: {json_str}");
            };
            let ticker =
            if let Some(val) = Ticker::from_json(ticker_json) {
                val
            }else{
                bail!("Can't read ticker params from config: {json_str}");
            };
            tickers.insert(ticker_name, ticker);
        }
        Ok(Self {
            tickers,
            timestamp_counter: 1,
        })
    }

    pub fn generate_quote(&mut self, ticker_name: &str) -> Option<StockQuote> {
        let ticker = self.tickers.get_mut(ticker_name)?;
        let mut quote = StockQuote::default();
        quote.ticker = ticker_name.to_string();

        quote.timestamp = self.timestamp_counter;
        self.timestamp_counter += 1;

        let val_price: f64 = rand::rng().sample(StandardNormal);
        quote.price = quote.price + (ticker.price_range() / 8.0) * val_price;
        if quote.price < 0.0 {
            quote.price = 0.0;
        }
        if quote.price > ticker.upper_bound_price {
            quote.price = ticker.upper_bound_price;
        }
        ticker.current_price = quote.price;

        let val_volume: u32 = rand::rng().sample(StandardUniform);
        quote.volume = val_volume % ticker.volume_range() + ticker.lower_bound_volume;
        
        Some(quote)
    }
}