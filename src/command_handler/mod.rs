//! # command_handler module
//! This module separates logic for executing commands from the queue
//! Commands are handled with flow control; however, this could be a good place to use a chain of responsibility if the file gets too large.

use std::collections::{HashMap};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::client_data::{self, AccountUpdateFailure, TransactionID, ClientID};
use crate::command;
use crate::logger;

/// Handles command objects
/// 
/// # Arguments
/// 
/// client_data         data for all client accounts
/// rx                  a Reciever to gather commands
/// 
pub async fn handle_commands ( 
    client_data: Arc::<Mutex::<HashMap::<client_data::ClientID, Box<client_data::ClientData>>>>,
    mut rx: mpsc::Receiver<command::Command>
) -> () {

    while let Some(cmd) = rx.recv().await {

        // Identify the type of command
        match cmd.get_type() {

            // For deposits...
            command::CommandType::Deposit => {

                let mut c_d = client_data.lock().unwrap();

                // find the client
                if let Some(client) = c_d.get_mut(&cmd.get_client_id()) {

                    // If the client is known...
                    deposit_for_client(client, &cmd);
                }
                else {
                    
                    // If the client is unknown, create it, update it, then add it to our list of clients...
                    let mut client = Box::new(client_data::ClientData::new());

                    deposit_for_client(&mut client, &cmd);

                    c_d.insert(cmd.get_client_id(), client);
                }
            },
            // For withdrawals...
            command::CommandType::Withdraw => {

                let mut c_d = client_data.lock().unwrap();

                // find the client
                if let Some(client) = c_d.get_mut(&cmd.get_client_id()) {

                    // If the client is known...
                    withdraw_for_client(client, &cmd);
                }
                else {

                    // If the client is unknown, create it, update it, then add it to our list of clients...
                    let mut client = Box::new(client_data::ClientData::new());

                    withdraw_for_client(&mut client, &cmd);

                   c_d.insert(cmd.get_client_id(), client);
                }
            },
            // For disputes...
            command::CommandType::Dispute => {

                // find the client
                match client_data.lock().unwrap().get_mut(&cmd.get_client_id()) {

                    // If the client is known...
                    Some(client) => {

                        // handle the dispute.
                        match client.as_mut().dispute(cmd.get_transaction_id()) {

                            // if there was an issue with the dispute, handle it
                            Err(err) => {
                                match err {
                                    client_data::AccountUpdateFailure::Frozen => {
                                        logger::warning( &msg_build("dispute", "the corresponding user account is frozen", &cmd.get_transaction_id(), &cmd.get_client_id()) );
                                    },
                                    client_data::AccountUpdateFailure::RedundantDispute => {
                                        logger::warning( &msg_build("dispute", "the dispute was redundant", &cmd.get_transaction_id(), &cmd.get_client_id()) );
                                    },
                                    client_data::AccountUpdateFailure::TXNotFound => {
                                        logger::warning( &msg_build("dispute", "the transaction did not correspond to a known deposit for that user", &cmd.get_transaction_id(), &cmd.get_client_id()) );
                                    },
                                    _ => (),
                                };
                            },
                            Ok(()) => (),
                        };
                    },

                    // If the client is unknown...
                    None => {
                        logger::warning(&msg_build("dispute", "the transaction did not correspond to a known user", &cmd.get_transaction_id(), &cmd.get_client_id()));
                    },
                };
            },
            // For Resolves...
            command::CommandType::Resolve => {
                
                // find the client
                match client_data.lock().unwrap().get_mut(&cmd.get_client_id()) {

                    // if the client is known...
                    Some(client) => {
                        match client.as_mut().resolve( cmd.get_transaction_id() ) {
                            Err(err) => {
                                match err {
                                    client_data::AccountUpdateFailure::Frozen => {
                                        logger::warning( &msg_build("resolve", "the corresponding user account is frozen", &cmd.get_transaction_id(), &cmd.get_client_id()) );
                                    },
                                    client_data::AccountUpdateFailure::TXUndisputed => {
                                        logger::warning( &msg_build("resolve", "the transaction is not under dispute", &cmd.get_transaction_id(), &cmd.get_client_id()) );
                                    },
                                    client_data::AccountUpdateFailure::TXNotFound => {
                                        logger::warning( &msg_build("resolve", "the transaction did not correspond to a known deposit for that user", &cmd.get_transaction_id(), &cmd.get_client_id()) );
                                    },
                                    _ => (),
                                }
                            },
                            Ok(()) => (),
                        };
                    },
                    // if the client is unknown...
                    None => {
                        logger::warning(&msg_build("resolve", "the transaction did not correspond to a known user", &cmd.get_transaction_id(), &cmd.get_client_id()));
                    },
                };

            },
            // For Chargebacks...
            command::CommandType::Chargeback => {
                
                // find the client
                match client_data.lock().unwrap().get_mut(&cmd.get_client_id()) {

                    // if the client is known...
                    Some(client) => {
                        match client.as_mut().chargeback( cmd.get_transaction_id() ) {
                            Err(err) => {
                                match err {
                                    client_data::AccountUpdateFailure::Frozen => {
                                        logger::warning( &msg_build("chargeback", "the corresponding user account is frozen", &cmd.get_transaction_id(), &cmd.get_client_id()) );
                                    },
                                    client_data::AccountUpdateFailure::TXUndisputed => {
                                        logger::warning( &msg_build("chargeback", "the transaction is not under dispute", &cmd.get_transaction_id(), &cmd.get_client_id()) );
                                    },
                                    client_data::AccountUpdateFailure::TXNotFound => {
                                        logger::warning( &msg_build("chargeback", "the transaction did not correspond to a known deposit for that user", &cmd.get_transaction_id(), &cmd.get_client_id()) );
                                    },
                                    _ => (),
                                }
                            },
                            Ok(()) => (),
                        };
                    },
                    // if the client is unknown...
                    None => {
                        logger::warning(&msg_build("chargeback", "the transaction did not correspond to a known user", &cmd.get_transaction_id(), &cmd.get_client_id()));
                    },
                };

            },
        };

    }

}


/**************************
 * 
 * 
 * PRIVATE FUNCTIONS
 * 
 * 
 **************************/


#[inline(always)]
fn msg_build (process_type: &str, problem: &str, tx: &TransactionID, client: &ClientID) -> String {
    format!( "TX:{} to {} for user:{} did not succeed because {}.", 
        tx, 
        process_type,
        client,
        problem )
}

#[inline(always)]
fn withdraw_for_client (client: &mut client_data::ClientData, cmd: &command::Command) {

    // get the deposit ammount
    if let Some(wealth) = cmd.get_wealth() {

        // withdraw the funds
        if let Err(err) = client.withdraw(*wealth) {

            // if there was an error, log it appropriately
            if client_data::AccountUpdateFailure::Frozen == err {
                logger::warning( &msg_build("withdraw", "their account is frozen", &cmd.get_transaction_id(), &cmd.get_client_id()) );
            }
            else {
                logger::warning( &msg_build("withdraw", "their account has insufficient funds", &cmd.get_transaction_id(), &cmd.get_client_id()) );
            }
        }
    }
    // this condition should never be reached because deposit commands should always have a value
    else {
        let msg = msg_build("withdraw", "the transaction did not contain the ammount", &cmd.get_transaction_id(), &cmd.get_client_id());
        logger::error( &msg );
    }
}

// This is what we do with a client's account when a deposit occurs.
#[inline(always)]
fn deposit_for_client (client: &mut client_data::ClientData, cmd: &command::Command) {

    // get the deposit ammount
    if let Some(wealth) = cmd.get_wealth() {

        // add the funds to the account
        match client.deposit(cmd.get_transaction_id(), *wealth) {

            // if there was an issue, log it
            Err(err) => {

                //identify the issue
                match err {

                    AccountUpdateFailure::Frozen => {
                        // log the error if the account was frozen
                        logger::warning( &msg_build("deposit","their account is frozen", &cmd.get_transaction_id(), &cmd.get_client_id()) );
                    },

                    AccountUpdateFailure::DuplicateDepositTX => {
                        // log the error if the deposit has a duplicate tx
                        logger::warning( &msg_build("deposit","the deposit tx id is a duplicate", &cmd.get_transaction_id(), &cmd.get_client_id()) );
                    },

                    _ => {
                        panic!("unexpected issue with deposit");
                    },

                }
            },
            Ok(()) => {},
        }
    }
    // this condition should never be reached because deposit commands should always have a value
    else {
        let msg = msg_build("deposit", "the transaction did not contain the ammount", &cmd.get_transaction_id(), &cmd.get_client_id());
        logger::error( &msg );
    }
}

