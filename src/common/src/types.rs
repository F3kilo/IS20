use candid::{CandidType, Nat, Principal};
use serde::Deserialize;

#[allow(non_snake_case)]
#[derive(Deserialize, CandidType, Clone, Debug)]
pub struct Metadata {
    pub logo: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub totalSupply: Nat,
    pub owner: Principal,
    pub fee: Nat,
    pub feeTo: Principal,
    pub isTestToken: Option<bool>,
}

#[derive(CandidType, Debug, Clone, Deserialize)]
pub struct SignedTx {
    /// Principal of token that called `receive_is20`
    pub principal: Principal,
    pub publickey: Vec<u8>,
    pub signature: Vec<u8>,
    /// Transaction serialized with `serde-cbor`.
    pub serialized_tx: Vec<u8>,
}
