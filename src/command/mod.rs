//! # command module
//! This module separates model logic relating to Command objects, which serve as Commands in a Command Pattern.
//! The benefits of using the command pattern are 
//!  > asynchronous file reading and data processing,
//!  > the potential to keep a command history and role back changes if needed, 
//!  > the potential to (after solving race conditions which would occur), have more than one thread servicing commands for data processing
//!  > ...
use rust_decimal::prelude::Decimal;
use serde::Deserialize;

use crate::client_data::{TransactionID, ClientID};

// TODO A more flexible sollution would be to have the command processor accept commands which implement a common trait, like 'execute'
// TODO: what if disputed deposit should send acconut negative?
//   TODO: verify disputes are on deposits... check examples' transaction numbers

#[derive(Deserialize, Copy, Clone, PartialEq, Debug)]
pub enum CommandType {
    #[serde(rename = "withdrawal")]
    Withdraw,
    #[serde(rename = "deposit")]
    Deposit,
    #[serde(rename = "dispute")]
    Dispute,
    #[serde(rename = "resolve")]
    Resolve,
    #[serde(rename = "chargeback")]
    Chargeback,
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct Command {
    #[serde(rename = "type")]
    command_type: CommandType,
    #[serde(rename = "client")]
    client_id: ClientID,
    #[serde(rename = "tx")]
    transaction_id: TransactionID,
    #[serde(rename = "amount")]
    wealth: Option<Decimal>,
}

impl Command {
    pub fn get_type(&self) -> CommandType {
        self.command_type
    }
    pub fn get_client_id(&self) -> ClientID {
        self.client_id
    }
    pub fn get_transaction_id(&self) -> TransactionID {
        self.transaction_id
    }
    pub fn get_wealth(&self) -> &Option<Decimal> {
        &self.wealth
    }
}