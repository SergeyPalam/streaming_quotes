//! # streaming_quotes
//! > Библиотека для создание клиентской и серверной части работы с котировками

#![warn(missing_docs)]
/// Генератор котировок
pub mod quote;

/// Протокол взаимодействия клиент-сервер
pub mod protocol;

/// Многопоточный сервер
pub mod server;

/// Многопоточный клиент
pub mod client;

/// Таймер для отслеживания разных событий
pub mod timer;

/// Утилиты
pub mod utils;

use anyhow::Result;
use flexi_logger::{Logger, FileSpec, Duplicate, opt_format};
use std::path::Path;

/// Инициализация лога
#[cfg(debug_assertions)]
pub fn init_log(log_path_dir: &Path, base_name: &str) -> Result<()> {
    Logger::try_with_str("debug")?.
        log_to_file(FileSpec::default().directory(log_path_dir).basename(base_name)).
        duplicate_to_stdout(Duplicate::All).
        format(opt_format)
        .start()?;

    Ok(())
}

#[cfg(not(debug_assertions))]
pub fn init_log(log_path_dir: &Path) -> Result<()> {
    Logger::try_with_str("info")?.
        log_to_file(FileSpec::default().directory(log_path_dir).basename("server.log")).
        duplicate_to_stdout(Duplicate::All)
        .start()?;

    Ok(())
}
