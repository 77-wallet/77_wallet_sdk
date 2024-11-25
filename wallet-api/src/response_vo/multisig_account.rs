use serde::Serialize;
use wallet_database::entities::{
    multisig_account::MultisigAccountEntity, multisig_member::MultisigMemberEntity,
};

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MultisigAccountInfo {
    #[serde(flatten)]
    pub account: MultisigAccountEntity,
    pub member: Vec<MultisigMemberEntity>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MultisigAccountList {
    #[serde(flatten)]
    pub account: MultisigAccountEntity,
    pub symbol: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MultisigFeeVo {
    pub symbol: String,
    pub fee: String,
    pub address: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AddressStatus {
    pub address_status: i32,
}
