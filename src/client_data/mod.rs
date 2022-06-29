//! # client_data module
//! This module separates model logic for client data.  Client data consists of 
//!  > wealth
//!  > held wealth
//!  > frozen
//!  > deposit_history
//! 
//! These, along with the keys used to store client data, are sufficient to calculate desired output records (which is done in the transaction_csv module)
//! 
//! # why deposit_history?
//! 
//! Why is deposit_history only for deposits?
//! 
//! Reading the documentation, I got the impression that disputes were only in regards to deposits.
//! 
//! Why is deposit_history stored per-client rather than in a unified hashmap relying on tx ids as keys?  It would improve locality if it were in a unified hashmap...
//! 
//! It is because such gains are probably marginal with disputes hopefully not being the norm and because rather than just calling `dispute`, `resolve`, or `chargeback` methods, as is, each method would also need a copy of the unified deposit_history hashmap.  In short, I think it reads a little easier this way.
//! 
//! Why don't I just keep a history of the commands in order to role back deposits, rather than keeping a history of deposits?
//! 
//! At the moment, that would be the only application of the command history.  Comparably, the command history would take more space.
//! 
//! # The Decimal Crate
//! 
//! "Whitespaces and decimal precisions (up to four places past the decimal) must be accepted by your program."
//! 
//! For values much greater than 10^2, double precision floating point will not work.
//!     https://en.wikipedia.org/wiki/File:IEEE754.svg
//! 
//! I could manually solve this possible issue; however, rust_decimal gets a lot of traffic and should handle it for us
//!     'a 96 bit integer, a 1 bit sign, and a scaling factor'

use std::collections::HashMap;

use rust_decimal::prelude::Decimal;
use rust_decimal_macros::dec;

pub type ClientID = u16;
pub type TransactionID = u32;

pub struct ClientData {
    wealth: Decimal,
    held_wealth: Decimal,
    frozen: bool,
    deposit_history: HashMap<TransactionID, Box<Deposit>>,
}

struct Deposit {
    disputed: bool,
    ammount: Decimal,
}

// TODO: should I use Error instead?
#[derive(PartialEq, Debug)]
pub enum AccountUpdateFailure {
    Frozen,
    TXNotFound,
    TXUndisputed,
    InsufficientFunds,
    DuplicateDepositTX,
    RedundantDispute,
}

// accessors and constructor
impl ClientData {
    pub fn is_locked(&self) -> bool { self.frozen }
    pub fn get_total(&self) -> Decimal { self.wealth + self.held_wealth }
    pub fn get_held_wealth(&self) -> Decimal { self.held_wealth }
    pub fn get_wealth(&self) -> Decimal { self.wealth }
    pub fn new() -> ClientData {
        ClientData {
            wealth: dec!(0.0),
            held_wealth: dec!(0.0),
            frozen: false,
            deposit_history: HashMap::new(),
        }
    }
}

// This is controller logic, arguably.
// On the other hand, it enforces the only means in which this data is meant to be used, so I feel packaging it with the model is appropriate.
impl ClientData {
    /// Deposits money in a the account; remembers the event in case of a later dispute.
    /// 
    /// # Example
    /// 
    /// '''
    /// let mut client = ClientData::new();
    /// client.deposit( 22, 50.0 );
    /// '''
    /// 
    /// # Return Value
    /// 
    /// false      the user's account is locked, which occurs when a chargeback happens on their account
    /// true
    /// 
    pub fn deposit(&mut self, transaction_id: TransactionID, wealth: Decimal) -> Result<(), AccountUpdateFailure> {
        if self.frozen {
            Err(AccountUpdateFailure::Frozen)
        }
        else if self.deposit_history.contains_key(&transaction_id) {
            Err(AccountUpdateFailure::DuplicateDepositTX)
        }
        else {
            self.wealth += wealth;
            self.deposit_history.insert(
                transaction_id, 
                Box::new(Deposit { 
                    disputed: false,
                    ammount: wealth 
                })
            );
            Ok(())
        }
    }
    /// Withdraws money from the account
    /// 
    /// # Return Value
    /// 
    /// Err(AccountUpdateFailure::Frozen)               The account is locked, which occurs when a chargeback happens on the account
    /// Err(AccountUpdateFailure::InsufficientFunds)    The account does not have sufficient funds*1 to cover the withdrawal
    /// Ok(())
    /// 
    /// *1 Held funds are not considered available for withdrawal.
    /// 
    pub fn withdraw(&mut self, wealth: Decimal)-> Result<(),AccountUpdateFailure> {
        if self.frozen {
            Err(AccountUpdateFailure::Frozen)
        }
        else if self.wealth < wealth {
            Err(AccountUpdateFailure::InsufficientFunds)
        }
        else {
            self.wealth-=wealth;
            Ok(())
        }
    }
    /// Submits a dispute on a deposit into the account, putting a hold on the associated funds
    /// 
    /// # Return Value
    /// 
    /// Err(AccountUpdateFailure::Frozen)               The account is locked, which occurs when a chargeback happens on the account
    /// Err(AccountUpdateFailure::RedundantDispute)     The transaction has already been disputed
    /// Err(AccountUpdateFailure::TXNotFound)           The deposit to be disputed was not made to this user account
    /// Ok(())
    /// 
    pub fn dispute(&mut self, transaction: TransactionID) -> Result<(),AccountUpdateFailure> {
        if self.frozen {
            Err(AccountUpdateFailure::Frozen)
        }
        else if let Some(transaction) = self.deposit_history.get_mut(&transaction) {
            if transaction.disputed {
                Err(AccountUpdateFailure::RedundantDispute)
            }
            else {
                transaction.disputed = true;
// TODO: what if withdrawals have taken place, leaving insufficient funds for this dispute?  As is, account 'wealth' will become negative.
                self.wealth-=transaction.ammount;
                self.held_wealth+=transaction.ammount;
                Ok(())
            }
        }
        else {
            Err(AccountUpdateFailure::TXNotFound)
        }
    } 
    /// Submits a chargeback on a dispute into the account, freezing the account, removing the funds put on hold by the dispute, and removing the deposit from the account's history
    /// 
    /// # Return Value
    /// 
    /// Err(AccountUpdateFailure::Frozen)               The account is locked, which occurs when a chargeback happens on the account
    /// Err(AccountUpdateFailure::TXUndisputed)         The transaction was not under dispute, so a chargeback does not make since
    /// Err(AccountUpdateFailure::TXNotFound)           The deposit to be disputed was not made to this user account
    /// Ok(())
    /// 
    pub fn chargeback(&mut self, transaction: TransactionID) -> Result<(), AccountUpdateFailure> {
        if self.frozen {
            Err(AccountUpdateFailure::Frozen)
        }
        else if let Some(transaction_event) = self.deposit_history.get_mut(&transaction) {
            if transaction_event.disputed {
                self.held_wealth -= transaction_event.ammount;
                self.frozen = true;
                // The deposit which was disputed has been overturned.
                // Since that is the case, we can lose this transaction.
                // An alternative might be to change disputed to a trinary state variable.
                //  Then, transactions which are chargeback, we would ensure did not fall again under dispute.
                //  For the problem as currently described, there is no known need to do so.
                //  That would be different if:
                //   we had to keep a history of such activities,
                //   we could undo chargebacks
                //   etc.
                self.deposit_history.remove(&transaction);
                
                Ok(())
            }
            else {
                Err(AccountUpdateFailure::TXUndisputed)
            }
        }
        else {
            Err(AccountUpdateFailure::TXNotFound)
        }
    }
    /// Submits a resolve on a dispute into the account, releasing the funds held in dispute
    /// 
    /// # Return Value
    /// 
    /// Err(AccountUpdateFailure::Frozen)               The account is locked, which occurs when a chargeback happens on the account
    /// Err(AccountUpdateFailure::TXUndisputed)         The transaction was not under dispute, so a resolve does not make since
    /// Err(AccountUpdateFailure::TXNotFound)           The deposit to be disputed was not made to this user account
    /// Ok(())
    /// 
    pub fn resolve(&mut self, transaction: TransactionID) -> Result<(), AccountUpdateFailure> {
        if self.frozen {
            Err(AccountUpdateFailure::Frozen)
        }
        else if let Some(transaction) = self.deposit_history.get_mut(&transaction) {
            if transaction.disputed {
                transaction.disputed = false;
                self.wealth += transaction.ammount;
                self.held_wealth -= transaction.ammount;
                Ok(())
            }
            else {
                Err(AccountUpdateFailure::TXUndisputed)
            }
        }
        else {
            Err(AccountUpdateFailure::TXNotFound)
        }
    }
}

#[cfg(test)]
mod client_data_tests {
    use crate::client_data::AccountUpdateFailure;

    use super::ClientData;
    use rust_decimal_macros::dec;

    #[test]
    fn test_deposit() {
        let mut client = ClientData::new();
        assert_eq!(client.get_wealth(), dec!(0.0000));
        assert_eq!(Ok(()), client.deposit(1, dec!(20.0)) );
        assert_eq!(client.get_wealth(), dec!(20.0));

        assert_eq!(Err(AccountUpdateFailure::DuplicateDepositTX), client.deposit(1, dec!(20.0)) );

        client.frozen = true;
        assert_eq!( Err(AccountUpdateFailure::Frozen), client.deposit(2, dec!(2.0)) )
    }

    #[test]
    fn test_withdraw() {
        let mut client = ClientData::new();
        assert_eq!(client.get_wealth(), dec!(0.0000));
        assert_eq!(Ok(()), client.deposit(1, dec!(20.0)));

        assert_eq!(Ok(()), client.withdraw(dec!(10.0)));
        assert_eq!(client.get_wealth(), dec!(10.0));
        
        client.frozen = true;
        let result = client.withdraw(dec!(5.0));
        assert_eq!(result, Err(AccountUpdateFailure::Frozen));
        client.frozen = false;

        let result = client.withdraw(dec!(500.0));
        assert_eq!(result, Err(AccountUpdateFailure::InsufficientFunds));

        assert_eq!(Ok(()), client.dispute(1));
        let result = client.withdraw(dec!(5.0));
        assert_eq!(result, Err(AccountUpdateFailure::InsufficientFunds));
    }

    #[test]
    fn test_dispute() {
        let mut client = ClientData::new();
        assert_eq!(client.get_wealth(), dec!(0.0000));
        assert_eq!(Ok(()), client.deposit(1, dec!(20.0)));
        
        assert_eq!(Ok(()), client.dispute(1));
        assert_eq!(client.get_wealth(), dec!(0.0000));
        assert_eq!(client.get_held_wealth(), dec!(20.0));

        assert_eq!(Err(AccountUpdateFailure::RedundantDispute), client.dispute(1));

        // to verify disput can be done again after resolve
        assert_eq!(Ok(()), client.resolve(1));
        // to verify disputing insufficient funds forces available balance negative
        assert_eq!(Ok(()), client.withdraw(dec!(5.0)));

        assert_eq!(Ok(()), client.dispute(1));
        assert_eq!(client.get_wealth(), dec!(-5.0));

        assert_eq!(Err(AccountUpdateFailure::TXNotFound), client.dispute(42));
        
        client.frozen = true;
        assert_eq!(Err(AccountUpdateFailure::Frozen), client.dispute(1));
    }

    #[test]
    fn test_resolve() {
        let mut client = ClientData::new();
        assert_eq!(client.get_wealth(), dec!(0.0000));
        assert_eq!(Ok(()), client.deposit(1, dec!(20.0)));

        assert_eq!(Ok(()), client.dispute(1));
        assert_eq!(client.get_wealth(), dec!(0.0000));
        assert_eq!(client.get_held_wealth(), dec!(20.0));

        assert_eq!(Ok(()), client.resolve(1));
        assert_eq!(client.get_held_wealth(), dec!(0.0000));
        assert_eq!(client.get_wealth(), dec!(20.0));

        assert_eq!(Err(AccountUpdateFailure::TXNotFound), client.resolve(42));

        assert_eq!(Err(AccountUpdateFailure::TXUndisputed), client.resolve(1));

        client.frozen = true;
        assert_eq!(Err(AccountUpdateFailure::Frozen), client.resolve(1));
    }

    #[test]
    fn test_chargeback() {
        let mut client = ClientData::new();
        assert_eq!(client.get_wealth(), dec!(0.0000));
        assert_eq!(Ok(()), client.deposit(1, dec!(20.0)));

        assert_eq!(Ok(()), client.dispute(1));
        assert_eq!(client.get_wealth(), dec!(0.0000));
        assert_eq!(client.get_held_wealth(), dec!(20.0));

        assert_eq!(Ok(()), client.chargeback(1));
        assert_eq!(client.get_wealth(), dec!(0.0000));
        assert_eq!(client.get_held_wealth(), dec!(0.0000));

        // client should be frozen after chargeback
        assert_eq!(Err(AccountUpdateFailure::Frozen), client.chargeback(1));
        client.frozen = false;
        assert_eq!(Ok(()), client.deposit(1, dec!(20.0)));

        // to verify chargeback of insufficient funds forces available balance negative
        assert_eq!(Ok(()), client.withdraw(dec!(5.0)));
        assert_eq!(Ok(()), client.dispute(1));
        assert_eq!(Ok(()), client.chargeback(1));
        assert_eq!(client.get_wealth(), dec!(-5.0000));
        assert_eq!(client.get_held_wealth(), dec!(0.0000));
        client.frozen = false;

        assert_eq!(Err(AccountUpdateFailure::TXNotFound), client.chargeback(42));

        assert_eq!(Ok(()), client.deposit(1, dec!(20.0)));
        assert_eq!(Err(AccountUpdateFailure::TXUndisputed), client.chargeback(1));
    }

}