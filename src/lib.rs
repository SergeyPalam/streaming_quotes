pub mod quote;
pub mod protocol;
pub mod server;
pub mod client;
pub mod timer;
pub mod utils;

use anyhow::Result;
use flexi_logger::{Logger, FileSpec, Duplicate, opt_format};
use std::path::Path;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        
    }
}
