use std::collections::HashMap;
use std::time::Duration;
use std::thread;
use anyhow::{Result, bail};

const TICK_MILLIS: u64 = 10;

struct Event {
    counter: u64,
    bound: u64,
}

impl Event {
    fn new(bound_millis: u64) -> Self {
        Self {
            counter: 0,
            bound: bound_millis,
        }
    }

    fn tick(&mut self) {
        if self.counter < (self.bound / TICK_MILLIS) {
            self.counter += 1;
        }
    }

    fn is_expired(&self) -> bool {
        if self.counter >= (self.bound / TICK_MILLIS){
            return true;
        }
        false
    }
}

#[derive(Default)]
pub struct Timer {
    events: HashMap<String, Event>,
}

impl Timer {
    pub fn sleep(&mut self) {
        thread::sleep(Duration::from_millis(TICK_MILLIS));
        for (_, event) in self.events.iter_mut() {
            event.tick();
        }
    }

    pub fn add_event(&mut self, event_name: &str, bound_millis: u64) {
        self.events.insert(event_name.to_string(), Event::new(bound_millis));
    }

    pub fn remove_event(&mut self, event_name: &str) -> Result<()> {
        match self.events.remove(event_name) {
            Some(_) => {
                Ok(())
            }
            None => {
                bail!("Wrong event name");
            }
        }
    }

    pub fn reset_event(&mut self, event_name: &str) -> Result<()> {
        match self.events.get_mut(event_name) {
            Some(evt) => {
                evt.counter = 0;
                Ok(())
            }
            None => {
                bail!("Wrong event name");
            }
        }
    }

    pub fn is_expired_event(&self, event_name: &str) -> Result<bool> {
        match self.events.get(event_name) {
            Some(evt) => {
                Ok(evt.is_expired())
            }
            None => {
                bail!("Wrong event name");
            }
        }
    }
}