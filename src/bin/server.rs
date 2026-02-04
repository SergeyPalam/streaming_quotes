use clap::Parser;
use std::path::Path;
use streaming_quotes::init_log;
use streaming_quotes::server::quotes_server::{ControlCmd, QuotesServer};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Server config path
    #[arg(short, long)]
    config_path: String,
}

fn main() {
    if let Err(e) = init_log(Path::new("logs"), "server.log") {
        println!("Can't init logger: {e}");
        return;
    }

    let args = Args::parse();

    let quotes_server = match QuotesServer::new(&args.config_path) {
        Ok(val) => val,
        Err(e) => {
            log::error!("Can't create server: {e}");
            return;
        }
    };

    let server_control = match quotes_server.start() {
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
        if let Err(e) = stdin.read_line(&mut cmd_buf) {
            log::error!("Can't read new command: {e}");
            break;
        }
        if cmd_buf.trim().to_lowercase() == "exit" {
            break;
        } else {
            cmd_buf.clear();
        }
    }

    if let Err(e) = server_control.tx.send(ControlCmd::Stop) {
        log::error!("Stop error: {e}");
    }

    if server_control.thread_handle.join().is_err() {
        log::error!("Can't join thread");
    }
    log::info!("Exit");
}
