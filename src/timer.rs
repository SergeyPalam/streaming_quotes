use anyhow::{Result, bail};
use std::collections::HashMap;
use std::thread;
use std::time::Duration;

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
        if self.counter >= (self.bound / TICK_MILLIS) {
            return true;
        }
        false
    }
}

#[derive(Default)]
/// Таймер с минимольным тиком 10 мс
/// Используется для мониторинга событий с разными временными окнами
pub struct Timer {
    events: HashMap<String, Event>,
}

impl Timer {
    /// Усыпляет поток на 10 мс и увеличивает счетчик всех подписанных событий
    pub fn sleep(&mut self) {
        thread::sleep(Duration::from_millis(TICK_MILLIS));
        for (_, event) in self.events.iter_mut() {
            event.tick();
        }
    }

    /// Подписывает событие на мониторинг
    pub fn add_event(&mut self, event_name: &str, bound_millis: u64) {
        self.events
            .insert(event_name.to_string(), Event::new(bound_millis));
    }

    /// Удаляет подписку события для таймера
    pub fn remove_event(&mut self, event_name: &str) -> Result<()> {
        match self.events.remove(event_name) {
            Some(_) => Ok(()),
            None => {
                bail!("Wrong event name");
            }
        }
    }

    /// Если время для события истекло, то чтобы нужно явно обнулить счетчик
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

    /// Прошло ли время для события
    pub fn is_expired_event(&self, event_name: &str) -> Result<bool> {
        match self.events.get(event_name) {
            Some(evt) => Ok(evt.is_expired()),
            None => {
                bail!("Wrong event name");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sleep() {
        let mut timer = Timer::default();
        timer.add_event("A", 20);
        timer.add_event("B", 30);

        timer.sleep();
        assert_eq!(timer.is_expired_event("A").unwrap(), false);
        assert_eq!(timer.is_expired_event("B").unwrap(), false);
        timer.sleep();
        assert_eq!(timer.is_expired_event("A").unwrap(), true);
        assert_eq!(timer.is_expired_event("B").unwrap(), false);
        timer.sleep();
        assert_eq!(timer.is_expired_event("A").unwrap(), true);
        assert_eq!(timer.is_expired_event("B").unwrap(), true);

        timer.reset_event("A").unwrap();
        timer.reset_event("B").unwrap();

        assert_eq!(timer.is_expired_event("A").unwrap(), false);
        assert_eq!(timer.is_expired_event("B").unwrap(), false);
    }
}
