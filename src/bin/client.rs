use streaming_quotes::client::quotes_client::{QuotesClient, ClientCmd};
use streaming_quotes::init_log;
use clap::Parser;
use std::path::Path;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Server addr
    #[arg(short, long)]
    server: String,

    /// Port for receive quotes
    #[arg(short, long)]
    port: u16,

    /// Path to file with tickers names
    #[arg(short, long)]
    tickers_path: String,
}

fn main(){
    if let Err(e) = init_log(Path::new("logs"), "client.log") {
        println!("Can't init logger: {e}");
        return;
    }

    let args = Args::parse();

    let client =
    match QuotesClient::new(&args.server, args.port, &args.tickers_path){
        Ok(val) => val,
        Err(e) => {
            log::error!("Can't create client application: {e}");
            return;
        }
    };

    log::info!("Client: {}", client);

    let control =
    match client.start_receive_quotes() {
        Ok(val) => val,
        Err(e) => {
            log::error!("Can't start client application: {e}");
            return
        }
    };

    let mut cmd_buf = String::new();
    let stdin = std::io::stdin();
    loop {
         println!("To stop client type \"exit\"");
         if let Err(e) = stdin.read_line(&mut cmd_buf) {
            log::error!("Can't read new command: {e}");
            break;
         }
         if cmd_buf.trim().to_lowercase() == "exit" {
            break;
         }else{
            cmd_buf.clear();
         }
    }

    if let Err(e) = control.tx.send(ClientCmd::Stop) {
        log::error!("Stop error: {e}");
    }
    
    if control.thread_handle.join().is_err(){
        log::error!("Can't join thread");
    }
    log::info!("Exit");
}