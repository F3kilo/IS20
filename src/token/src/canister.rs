use crate::canister::dip20_transactions::{approve, burn, mint, transfer, transfer_from};
use crate::canister::is20_auction::{
    auction_info, bid_cycles, bidding_info, run_auction, AuctionError, BiddingInfo,
};
use crate::canister::is20_notify::{notify, transfer_and_notify};
use crate::canister::is20_transactions::transfer_include_fee;
use crate::state::CanisterState;
use crate::types::{AuctionInfo, StatsData, Timestamp, TokenInfo, TxError, TxReceipt, TxRecord};
use candid::Nat;
use common::types::Metadata;
use ic_canister::{init, query, update, Canister};
use ic_cdk::export::candid::Principal;
use num_traits::ToPrimitive;
use std::cell::RefCell;
use std::rc::Rc;

mod dip20_transactions;
mod inspect;
pub mod is20_auction;
pub mod is20_notify;
mod is20_transactions;

// 1 day in nanoseconds.
const DEFAULT_AUCTION_PERIOD: Timestamp = 24 * 60 * 60 * 1_000_000;

const MAX_TRANSACTION_QUERY_LEN: usize = 1000;

#[derive(Clone, Canister)]
pub struct TokenCanister {
    #[id]
    principal: Principal,

    #[state]
    state: Rc<RefCell<CanisterState>>,
}

#[allow(non_snake_case)]
impl TokenCanister {
    #[init]
    fn init(&self, metadata: Metadata) {
        self.state
            .borrow_mut()
            .balances
            .0
            .insert(metadata.owner, metadata.totalSupply.clone());
        self.state.borrow_mut().ledger.mint(
            metadata.owner,
            metadata.owner,
            metadata.totalSupply.clone(),
        );
        self.state.borrow_mut().stats = metadata.into();
        self.state.borrow_mut().bidding_state.auction_period = DEFAULT_AUCTION_PERIOD;
    }

    #[query]
    fn getTokenInfo(&self) -> TokenInfo {
        let StatsData {
            fee_to,
            deploy_time,
            ..
        } = self.state.borrow().stats;
        TokenInfo {
            metadata: self.state.borrow().get_metadata(),
            feeTo: fee_to,
            historySize: self.state.borrow().ledger.len(),
            deployTime: deploy_time,
            holderNumber: self.state.borrow().balances.0.len(),
            cycles: ic_kit::ic::balance(),
        }
    }

    #[query]
    fn getHolders(&self, start: usize, limit: usize) -> Vec<(Principal, Nat)> {
        self.state.borrow().balances.get_holders(start, limit)
    }

    #[query]
    fn getAllowanceSize(&self) -> usize {
        self.state.borrow().allowance_size()
    }

    #[query]
    fn getUserApprovals(&self, who: Principal) -> Vec<(Principal, Nat)> {
        self.state.borrow().user_approvals(who)
    }

    #[query]
    fn isTestToken(&self) -> bool {
        self.state.borrow().stats.is_test_token
    }

    #[update]
    fn toggleTest(&self) -> bool {
        check_caller(self.owner()).unwrap();
        let stats = &mut self.state.borrow_mut().stats;
        stats.is_test_token = !stats.is_test_token;
        stats.is_test_token
    }

    #[query]
    fn name(&self) -> String {
        self.state.borrow().stats.name.clone()
    }

    #[query]
    fn symbol(&self) -> String {
        self.state.borrow().stats.symbol.clone()
    }

    #[query]
    fn logo(&self) -> String {
        self.state.borrow().stats.logo.clone()
    }

    #[query]
    fn decimals(&self) -> u8 {
        self.state.borrow().stats.decimals
    }

    #[query]
    fn totalSupply(&self) -> Nat {
        self.state.borrow().stats.total_supply.clone()
    }

    #[query]
    fn balanceOf(&self, holder: Principal) -> Nat {
        self.state.borrow().balances.balance_of(&holder)
    }

    #[query]
    fn allowance(&self, owner: Principal, spender: Principal) -> Nat {
        self.state.borrow().allowance(owner, spender)
    }

    #[query]
    fn getMetadata(&self) -> Metadata {
        self.state.borrow().get_metadata()
    }

    #[query]
    fn historySize(&self) -> Nat {
        self.state.borrow().ledger.len()
    }

    #[query]
    fn getTransaction(&self, id: Nat) -> TxRecord {
        self.state
            .borrow()
            .ledger
            .get(&id)
            .unwrap_or_else(|| ic_kit::ic::trap(&format!("Transaction {} does not exist", id)))
    }

    #[query]
    fn getTransactions(&self, start: Nat, limit: Nat) -> Vec<TxRecord> {
        if limit > MAX_TRANSACTION_QUERY_LEN {
            ic_kit::ic::trap(&format!(
                "Limit must be less then {}",
                MAX_TRANSACTION_QUERY_LEN
            ));
        }

        self.state
            .borrow()
            .ledger
            .get_range(&start, &limit)
            .to_vec()
    }

    #[update]
    fn setName(&self, name: String) {
        check_caller(self.owner()).unwrap();
        self.state.borrow_mut().stats.name = name;
    }

    #[update]
    fn setLogo(&self, logo: String) {
        check_caller(self.owner()).unwrap();
        self.state.borrow_mut().stats.logo = logo;
    }

    #[update]
    fn setFee(&self, fee: Nat) {
        check_caller(self.owner()).unwrap();
        self.state.borrow_mut().stats.fee = fee;
    }

    #[update]
    fn setFeeTo(&self, fee_to: Principal) {
        check_caller(self.owner()).unwrap();
        self.state.borrow_mut().stats.fee_to = fee_to;
    }

    #[update]
    fn setOwner(&self, owner: Principal) {
        check_caller(self.owner()).unwrap();
        self.state.borrow_mut().stats.owner = owner;
    }

    #[query]
    fn owner(&self) -> Principal {
        self.state.borrow().stats.owner
    }

    /// Returns an array of transaction records in range [start, start + limit) related to user `who`.
    /// Unlike `getTransactions` function, the range [start, start + limit) for `getUserTransactions`
    /// is not the global range of all transactions. The range [start, start + limit) here pertains to
    /// the transactions of user who. Implementations are allowed to return less TxRecords than
    /// requested to fend off DoS attacks.
    #[query]
    fn getUserTransactions(&self, who: Principal, start: Nat, limit: Nat) -> Vec<TxRecord> {
        let mut transactions = vec![];

        let limit_usize = limit.0.to_usize().unwrap_or(usize::MAX);
        if limit_usize > MAX_TRANSACTION_QUERY_LEN {
            ic_kit::ic::trap(&format!(
                "Limit must be less then {}",
                MAX_TRANSACTION_QUERY_LEN
            ));
        }

        for tx in self.state.borrow().ledger.get_range(&start, &limit) {
            if tx.from == who || tx.to == who || tx.caller == Some(who) {
                transactions.push(tx.clone());
            }
        }

        transactions
    }

    /// Returns total number of transactions related to the user `who`.
    #[query]
    fn getUserTransactionAmount(&self, who: Principal) -> Nat {
        let mut amount = Nat::from(0);
        for tx in self.state.borrow().ledger.iter() {
            if tx.from == who || tx.to == who || tx.caller == Some(who) {
                amount += tx.amount.clone();
            }
        }

        amount
    }

    #[update]
    fn transfer(&self, to: Principal, value: Nat, fee_limit: Option<Nat>) -> TxReceipt {
        transfer(self, to, value, fee_limit)
    }

    #[update]
    fn transferFrom(&self, from: Principal, to: Principal, value: Nat) -> TxReceipt {
        transfer_from(self, from, to, value)
    }

    /// Transfers `value` amount to the `to` principal, applying American style fee. This means, that
    /// the recipient will receive `value - fee`, and the sender account will be reduced exactly by `value`.
    ///
    /// Note, that the `value` cannot be less than the `fee` amount. If the value given is too small,
    /// transaction will fail with `TxError::AmountTooSmall` error.
    #[update]
    fn transferIncludeFee(&self, to: Principal, value: Nat) -> TxReceipt {
        transfer_include_fee(self, to, value)
    }

    #[update]
    fn approve(&self, spender: Principal, value: Nat) -> TxReceipt {
        approve(self, spender, value)
    }

    #[update]
    fn mint(&self, to: Principal, amount: Nat) -> TxReceipt {
        if !self.isTestToken() {
            check_caller(self.owner())?;
        }

        mint(self, to, amount)
    }

    #[update]
    fn burn(&self, amount: Nat) -> TxReceipt {
        burn(self, amount)
    }

    /********************** AUCTION ***********************/

    /// Bid cycles for the next cycle auction.
    ///
    /// This method must be called with the cycles provided in the call. The amount of cycles cannot be
    /// less than 1_000_000. The provided cycles are accepted by the canister, and the user bid is
    /// saved for the next auction.
    #[update]
    fn bidCycles(&self, bidder: Principal) -> Result<u64, AuctionError> {
        bid_cycles(self, bidder)
    }

    /// Current information about bids and auction.
    #[query]
    fn biddingInfo(&self) -> BiddingInfo {
        bidding_info(self)
    }

    /// Starts the cycle auction.
    ///
    /// This method can be called only once in a [BiddingState.auction_period]. If the time elapsed
    /// since the last auction is less than the set period, [AuctionError::TooEarly] will be returned.
    ///
    /// The auction will distribute the accumulated fees in proportion to the user cycle bids, and
    /// then will update the fee ratio until the next auction.
    #[update]
    fn runAuction(&self) -> Result<AuctionInfo, AuctionError> {
        run_auction(self)
    }

    /// Returns the information about a previously held auction.
    #[query]
    fn auctionInfo(&self, id: usize) -> Result<AuctionInfo, AuctionError> {
        auction_info(self, id)
    }

    /// Returns the minimum cycles set for the canister.
    ///
    /// This value affects the fee ratio set by the auctions. The more cycles available in the canister
    /// the less proportion of the fees will be transferred to the auction participants. If the amount
    /// of cycles in the canister drops below this value, all the fees will be used for cycle auction.
    #[query]
    fn getMinCycles(&self) -> u64 {
        self.state.borrow().stats.min_cycles
    }

    /// Sets the minimum cycles for the canister. For more information about this value, read [get_min_cycles].
    ///
    /// Only the owner is allowed to call this method.
    #[update]
    fn setMinCycles(&self, min_cycles: u64) -> Result<(), TxError> {
        check_caller(self.owner())?;
        self.state.borrow_mut().stats.min_cycles = min_cycles;
        Ok(())
    }

    /// Sets the minimum time between two consecutive auctions, in seconds.
    ///
    /// Only the owner is allowed to call this method.
    #[update]
    fn setAuctionPeriod(&self, period_sec: u64) -> Result<(), TxError> {
        check_caller(self.owner())?;
        // IC timestamp is in nanoseconds, thus multiplying
        self.state.borrow_mut().bidding_state.auction_period = period_sec * 1_000_000;
        Ok(())
    }

    /*********************** NOTIFY **********************/

    /// Notifies the transaction receiver about a previously performed transaction.
    ///
    /// This method guarantees that a notification for the same transaction id can be sent only once.
    /// It allows to use this method to reliably inform the transaction receiver without danger of
    /// duplicate transaction attack.
    ///
    /// In case the notification call fails, an [TxError::NotificationFailed] error is returned and
    /// the transaction will still be marked as not notified.
    ///
    /// If a notification request is made for a transaction that was already notified, a
    /// [TxError::AlreadyNotified] error is returned.
    #[update]
    async fn notify(&self, transaction_id: Nat) -> TxReceipt {
        notify(self, transaction_id).await
    }

    /// Convenience method to make a transaction and notify the receiver with just one call.
    ///
    /// If the notification fails for any reason, the transaction is still completed, but it will be
    /// marked as not notified, so a [notify] call can be done later to re-request the notification of
    /// this transaction.
    #[update]
    async fn transferAndNotify(
        &self,
        to: Principal,
        amount: Nat,
        fee_limit: Option<Nat>,
    ) -> TxReceipt {
        transfer_and_notify(self, to, amount, fee_limit).await
    }
}

fn check_caller(owner: Principal) -> Result<(), TxError> {
    if ic_kit::ic::caller() == owner {
        Ok(())
    } else {
        Err(TxError::Unauthorized {
            owner: owner.to_string(),
            caller: ic_kit::ic::caller().to_string(),
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_upgrade_from_previous() {
        use ic_storage::stable::write;
        write(&()).unwrap();
        let canister = TokenCanister::init_instance();
        canister.__post_upgrade_inst();
    }

    #[test]
    fn test_upgrade_from_current() {
        // Set a value on the state...
        let canister = TokenCanister::init_instance();
        let mut state = canister.state.borrow_mut();
        state.bidding_state.fee_ratio = 12345.0;
        drop(state);
        // ... write the state to stable storage
        canister.__pre_upgrade_inst();

        // Update the value without writing it to stable storage
        let mut state = canister.state.borrow_mut();
        state.bidding_state.fee_ratio = 0.0;
        drop(state);

        // Upgrade the canister should have the state
        // written before pre_upgrade
        canister.__post_upgrade_inst();
        let state = canister.state.borrow();
        assert_eq!(state.bidding_state.fee_ratio, 12345.0);
    }
}
