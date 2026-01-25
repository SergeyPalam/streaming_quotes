pub mod quote;
pub mod protocol;
pub mod server;
pub mod client;

use anyhow::Result;
use flexi_logger::{Logger, FileSpec, Duplicate};
use std::path::Path;

pub fn init_log(log_path_dir: &Path) -> Result<()> {
    Logger::try_with_str("debug")?.
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
