use streaming_quotes::server::quotes_server::{QuotesServer, ServerControl, ControlCmd};
use log::*;

fn main() {
    if let Err(e) = init_log(Path::new(".")) {
        println!("Can't init logger: {e}");
        return;
    }

    let quotes_server =
    match QuotesServer::new("") {
        Ok(val) => val,
        Err(e) => {
            log::error!("Can't create server: {e}");
            return;
        }
    };

    let server_control =
    match quotes_server.start() {
        Ok(val) => val,
        Err(e) => {
            log::error!("Can't start server: {e}");
            return;
        }
    };

    let mut cmd_buf = String::new();
    let stdin = std::io::stdin();
    loop {
         println!("To stop server type \"exit\"");
         stdin.read_line(&mut buffer)?;
         if cmd_buf.to_lowercase() == "exit" {
            break;
         }
    }

    server_control.tx.send(ControlCmd::Stop);
    server_control.thread_handle.join();
    log::info!("Exit");
}