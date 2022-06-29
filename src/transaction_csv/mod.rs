//! # transaction_csv module
//! This module separates logic for reading the transaction csv file, as well as writing the client data file
//! 
//! Todo ?
//!     Additional input validation to ensure fields are within expected parameters might not go amiss
//!     Either a feature to specify, or a means of detecting, the presence of a header
//!     A feature or flag to specify rather to output a header
//! 
//! 'transaction IDs (tx) are globally unique, though are also not guaranteed to be ordered.'
//! 'assume the transactions occur chronologically in the file'
//! 

use std::collections::{HashMap};
use std::sync::{Arc, Mutex};

use tokio::fs::File;
use tokio::io::{AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use crate::{logger, client_data, command};

/// Parses a csv file asynchronously into the command queue
/// The csv file should be a transaction csv, containing a series of transactions to affect client data... or 'commands'
/// 
/// By default, the csv reader will assume a header ("type, client, tx, amount") exists
/// It therefore skips the first line in csv input.
/// 
/// # Arguments
/// 
/// file_path           the path to the input csv file
/// tx                  transmitter to produce commands
/// 
pub async fn parse_csv(
    file_path: String,
    tx: mpsc::Sender<command::Command>
) {

    // open the file
    let mut rdr = csv_async::AsyncReaderBuilder::new()
        .trim(csv_async::Trim::All)
        .flexible(true)
        .create_deserializer(match File::open(&file_path).await {
            Err(err) => {
                let msg = format!("Opening {} failed: {}", &file_path, err);
                logger::error(&msg);
                panic!("{}", msg);
            }
            Ok(resolution) => resolution,
        });

    // get a stream for the file
    let mut records = rdr.deserialize::<command::Command>();

    // iterate over the file, deserializing 'records' (commands) as we go
    while let Some(record) = records.next().await {

        // handle any errors deserializing a 'record'
        let record: crate::command::Command = match record {

            Err(err) => {
                let msg = format!("Getting a command from {} failed: {}",file_path, err);

                logger::error(&msg);
                panic!("{}", msg);
            }

            Ok(resolution) => resolution,

        };

        // send command
        if let Err(err) = tx.send(record).await {
            let msg = format!("Failed to send command to rx: {:?}", err);
            logger::error(&msg);
            panic!("{}", msg);
        };

    };
}

/// Writes a csv file
/// The csv file contains information about user accounts
/// 
/// # Example Output
/// 
/// client, available, held, total, locked
/// 4, 36.0, 0.0, 36.0, true
/// 2, 33.0, 0.0, 30.0, false
/// 1, 30.0, 2.0, 32.0, false
/// 3, 36.0, 2.0, 32.0, true
/// 5, -6.0, 0.0, -6.0, true
/// 
/// # Arguments
/// 
/// command_queue       the queue to store commands in
/// 
pub async fn write_csv(
    client_data: Arc::<Mutex::<HashMap<client_data::ClientID, Box<client_data::ClientData>>>>
) {
    let mut stdout = tokio::io::stdout();

    // write the headers to the file
    let headers = "client,available,held,total,locked\n";
    match stdout.write_all(headers.as_bytes()).await {
        Ok(()) => (),
        Err(err) => {
            let msg = format!("An error occured while trying to write headers to the file: {}", err);
            logger::error(&msg);
            panic!("{}", msg);
        }
    };

    let c_d = {
        match client_data.lock() {
            Ok(c_d) => c_d,
            Err(err) => panic!("transaction_csv parser cannot lock the client_data for writing: {:?}", err),
        }
    };

    // output user data
    for (client_id, client) in c_d.iter() {

        let mut record = [
            client_id.to_string(),
            client.get_wealth().round_dp(4).to_string(), 
            client.get_held_wealth().round_dp(4).to_string(), 
            client.get_total().round_dp(4).to_string(), 
            client.is_locked().to_string(),
        ].join(",");

        record+="\n";
 
        match stdout.write_all(record.as_bytes()).await {
            Ok(()) => (),
            Err(err) => {
                let msg = format!("An error occured while trying to write records to the file: {}", err);
                logger::error(&msg);
                panic!("{}", msg);
            }
        };

    }

}



#[cfg(test)]
mod transaction_csv_tests {
    use std::collections::{HashMap};
    use std::fs::File;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use rust_decimal_macros::dec;
    use tempfile::tempdir;
    use tokio::time::timeout;

    use crate::client_data::{self, ClientData};

    macro_rules! assert_ok {
        ($in:expr) => {
            assert!( Ok(()) == $in );
        };
    }

    macro_rules! write_str {
        ($dst:expr, $fmt:expr) => {{
            if let Ok(result) = $dst.write_fmt(format_args!("{}", $fmt)) {
                result
            }
            else {
                panic!()
            }
        }};
    }

    #[tokio::test]
    async fn test_read() {

        // Create a directory inside of `std::env::temp_dir()`.
        if let Ok(dir) = tempdir() {

            let file_path = dir.path().join("temp_transactions.csv");
            
            if let Ok(mut file) = File::create(&file_path) {

                let content = concat!(
                    "type,  client,     tx, amount\n",
                    "deposit,    2,     44, 22.125\n",
                    "deposit,    2,     43, 11.0625\n",
                    "withdrawal, 1,     40, 15\n", // client won't be found; insufficient funds should be raised
                    "dispute,    2,     43, 17.0\n", // deposit 43 under dispute, ammount meaningless
                    "deposit,    1,     45, 20002.0001\n",
                    "deposit,    3,     44, 9999999.9999\n",
                    "resolve,    2,     43\n", // 43 not under dispute anymore
                    "dispute,    2,     43, 17.0\n", // deposit 43 under dispute
                    "dispute,    2,     43, 17.0\n", // attempt to duplicate dispute
                    "chargeback, 2  ,  43 , 23.33\n", // ammount should be stored in command but ignored by handler
                    "dispute,    2,     43, 17.0\n", // account locked; dispute no longer present
                    "dispute,    1,     11, 17.0\n", // dispute cannot find tx
                    "  deposit , 1,   50  ,  13  \n",
                    "deposit,    1,     51, \n", // will 0 be used for the ammount or will it raise an issue?
                );

                write_str!(file, content);

                let (tx, mut rx) = tokio::sync::mpsc::channel(16);

                let parser = tokio::spawn( crate::transaction_csv::parse_csv(
                    file_path.to_str().unwrap().to_owned(),
                    tx,
                ) );                
                
                let tester = tokio::spawn( async move {

                    let mut counter = 0;

                    while let Ok(Some(cmd)) = timeout(Duration::from_millis(1500), rx.recv()).await {

                        match counter {
                            0 => {
                                assert_eq!(cmd.get_type(), crate::command::CommandType::Deposit);
                                assert_eq!(cmd.get_client_id(), 2);
                                assert_eq!(cmd.get_transaction_id(), 44);
                                assert_eq!(cmd.get_wealth().unwrap(), dec!(22.125));
                            },
                            1 => {
                                assert_eq!(cmd.get_type(), crate::command::CommandType::Deposit);
                                assert_eq!(cmd.get_client_id(), 2);
                                assert_eq!(cmd.get_transaction_id(), 43);
                                assert_eq!(cmd.get_wealth().unwrap(), dec!(11.0625));
                            },
                            2 => {
                                assert_eq!(cmd.get_type(), crate::command::CommandType::Withdraw);
                                assert_eq!(cmd.get_client_id(), 1);
                                assert_eq!(cmd.get_transaction_id(), 40);
                                assert_eq!(cmd.get_wealth().unwrap(), dec!(15));
                            },
                            3 => {
                                assert_eq!(cmd.get_type(), crate::command::CommandType::Dispute);
                                assert_eq!(cmd.get_client_id(), 2);
                                assert_eq!(cmd.get_transaction_id(), 43);
                                assert_eq!(cmd.get_wealth().unwrap(), dec!(17.0));
                            },
                            4 => {
                                assert_eq!(cmd.get_type(), crate::command::CommandType::Deposit);
                                assert_eq!(cmd.get_client_id(), 1);
                                assert_eq!(cmd.get_transaction_id(), 45);
                                assert_eq!(cmd.get_wealth().unwrap(), dec!(20002.0001));
                            },
                            5 => {
                                assert_eq!(cmd.get_type(), crate::command::CommandType::Deposit);
                                assert_eq!(cmd.get_client_id(), 3);
                                assert_eq!(cmd.get_transaction_id(), 44);
                                assert_eq!(cmd.get_wealth().unwrap(), dec!(9999999.9999));
                            },
                            6 => {
                                assert_eq!(cmd.get_type(), crate::command::CommandType::Resolve);
                                assert_eq!(cmd.get_client_id(), 2);
                                assert_eq!(cmd.get_transaction_id(), 43);
                                assert!(&None == cmd.get_wealth());
                            },
                            7 => {
                                assert_eq!(cmd.get_type(), crate::command::CommandType::Dispute);
                                assert_eq!(cmd.get_client_id(), 2);
                                assert_eq!(cmd.get_transaction_id(), 43);
                                assert_eq!(cmd.get_wealth().unwrap(), dec!(17.0));
                            },
                            8 => {
                                assert_eq!(cmd.get_type(), crate::command::CommandType::Dispute);
                                assert_eq!(cmd.get_client_id(), 2);
                                assert_eq!(cmd.get_transaction_id(), 43);
                                assert_eq!(cmd.get_wealth().unwrap(), dec!(17.0));
                            },
                            9 => {
                                assert_eq!(cmd.get_type(), crate::command::CommandType::Chargeback);
                                assert_eq!(cmd.get_client_id(), 2);
                                assert_eq!(cmd.get_transaction_id(), 43);
                                assert_eq!(cmd.get_wealth().unwrap(), dec!(23.33));
                            },
                            10 => {
                                assert_eq!(cmd.get_type(), crate::command::CommandType::Dispute);
                                assert_eq!(cmd.get_client_id(), 2);
                                assert_eq!(cmd.get_transaction_id(), 43);
                                assert_eq!(cmd.get_wealth().unwrap(), dec!(17.0));
                            },
                            11 => {
                                assert_eq!(cmd.get_type(), crate::command::CommandType::Dispute);
                                assert_eq!(cmd.get_client_id(), 1);
                                assert_eq!(cmd.get_transaction_id(), 11);
                                assert_eq!(cmd.get_wealth().unwrap(), dec!(17.0));
                            },
                            12 => {
                                assert_eq!(cmd.get_type(), crate::command::CommandType::Deposit);
                                assert_eq!(cmd.get_client_id(), 1);
                                assert_eq!(cmd.get_transaction_id(), 50);
                                assert_eq!(cmd.get_wealth().unwrap(), dec!(13));
                            },
                            13 => {
                                assert_eq!(cmd.get_type(), crate::command::CommandType::Deposit);
                                assert_eq!(cmd.get_client_id(), 1);
                                assert_eq!(cmd.get_transaction_id(), 51);
                                assert!(&None == cmd.get_wealth());
                            },
                            _ => {
                                panic!("unexpected command parsed in test");
                            }
                        };

                        counter=counter+1;
                    }

                    assert_eq!(14, counter);
                } );

                if let Err(_) = parser.await {
                    panic!("Couldn't await parse_csv");
                }

                if let Err(_) = tester.await {
                    panic!("Couldn't await parse_csv's tester");
                }

                drop(file);
            }
            else {
                panic!("Couldn't create temp file")
            };

            if let Err(err) = dir.close() {
                panic!("Temp directory did not close properly: {}", err);
            }

        }
        else {
            panic!("Could not get temp dir");
        }
    }

    // where there is a todo!() in this test..
    //     there isn't a great way to finish this at the moment: https://users.rust-lang.org/t/how-to-test-output-to-stdout/4877/4
    #[allow(unreachable_code)]
    #[allow(unused)]
    #[ignore]
    #[tokio::test]
    async fn test_write() {

        // client, available, held, total, locked
        // 4, 0.0, 36.0, 36.0, true
        // 2, 33.0, 4.0, 37.0, false
        // 1, 30.0, 2.0, 32.0, false
        // 5, -6.0, 0.0, -6.0, true
        let mut data: HashMap<client_data::ClientID, Box<client_data::ClientData>> = HashMap::new();
        data.insert(
            4,
            Box::new( {
                let mut ret = ClientData::new();
                assert_ok!(ret.deposit(3, dec!(3333333.3333)));
                assert_ok!(ret.deposit(17, dec!(36)));
                assert_ok!(ret.dispute(3));
                assert_ok!(ret.dispute(17));
                assert_ok!(ret.chargeback(3));
                ret
            } )
        );
        data.insert(
            2,
            Box::new( {
                let mut ret = ClientData::new();
                assert_ok!(ret.deposit(3, dec!(99999999.9999)));
                assert_ok!(ret.withdraw(dec!(99999966.9999)));
                assert_ok!(ret.deposit(8, dec!(4)));
                assert_ok!(ret.dispute(8));
                ret
            } )
        );
        data.insert(
            1,
            Box::new( {
                let mut ret = ClientData::new();
                assert_ok!(ret.deposit(51, dec!(2)));
                assert_ok!(ret.deposit(52, dec!(30)));
                assert_ok!(ret.dispute(51));
                ret
            } )
        );
        data.insert(
            5,
            Box::new( {
                let mut ret = ClientData::new();
                assert_ok!(ret.deposit(55, dec!(6)));
                assert_ok!(ret.withdraw(dec!(6)));
                assert_ok!(ret.dispute(55));
                assert_ok!(ret.chargeback(55));
                ret
            } )
        );

        if let Ok(dir) = tempdir() {

            let file_path = dir.path().join("temp_output.csv").to_str().unwrap().to_owned();
            if let Ok(file) = tokio::fs::File::create(file_path).await {

                todo!();
// direct std out to the file at the path.
// tokio::io::stdout().;

                let temp = Arc::new(Mutex::new(data));
                crate::transaction_csv::write_csv(temp.clone()).await;
            }
            else {
                panic!("failed to create tokio file");
            }

            let hdr = "client,available,held,total,locked";
            let c5 = "5,-6,0,-6,true";
            let c4 = "4,0.0000,36.0000,36.0000,true";
            let c1 = "1,30,2,32,false";
            let c2 = "2,33.0000,4,37.0000,false";

            if let Ok(actual_content) = tokio::fs::read_to_string(&file_path).await {
                actual_content.split('\n').for_each(|line| {
                    if line.len() > 0 {
                        let line_content = line.split_once(',');
                        match line_content.unwrap().0 {
                            "client" => assert_eq!(hdr, line),
                            "5" => assert_eq!(c5, line),
                            "4" => assert_eq!(c4, line),
                            "1" => assert_eq!(c1, line),
                            "2" => assert_eq!(c2, line),
                            _ => panic!(),
                        }
                    }
                });
            }
            else {
                panic!("Could not read file to string for write test.");
            }
        }

    }

}



