//! # transaction parser project
//! 
//! A simple example project which parses a csv to enact transactions on client data and produce a description of the client account.
//! 
//! Output is generated to stdout; logging is performed to stderr
//! 
//! # tests
//! 
//! transaction_csv_tests
//! client_data_tests
//! 

use std::collections::{HashMap};
use std::env;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

mod client_data;
mod command;
mod command_handler;
mod logger;
mod transaction_csv;

// In this program, thread count shouldn't cause issues on most computers; however, to be scalable we spawn async threads.

#[tokio::main]
async fn main() {

    let (tx, rx) = mpsc::channel::<command::Command>(16);

    // Get the file argument from args
    let input_args: Vec<String> = env::args().collect();
    let file_path: &String = match input_args.get(1) {
        Some(arg) => {
            arg
        },
        None => {
            logger::error( "Transaction Parser expects a file path for the transactions csv file.  Example: `./transaction_parser \"C:\\input.csv\"`" );
            std::process::exit(1);
        }
    };

    // Create a client data object container
    // If many many clients are present, this may need to be re-engineered to handle clients in a DB
    let data = Arc::new(Mutex::new(HashMap::<client_data::ClientID, Box<client_data::ClientData>>::new()));

    // split concurrent asynchronous processes
    let parse = tokio::spawn(transaction_csv::parse_csv(
        file_path.clone(), 
        tx
    ) );
    let handle = tokio::spawn(command_handler::handle_commands(data.clone(), rx));

    // Join threads
    
    if let Err(err) = parse.await {
        logger::error(format!("Parser thread err: {:?}", err).as_str());
    }
    if let Err(err) = handle.await {
        logger::error(format!("Handler thread err: {:?}", err).as_str());
    }

    // write output
    
    transaction_csv::write_csv(data.clone()).await;

}
