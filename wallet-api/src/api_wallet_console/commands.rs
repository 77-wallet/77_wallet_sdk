use crate::batch_transfer::BatchTransferConfig;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum ConsoleCommand {
    ImportConfiguredWallets,
    RefreshWallets,
    RefreshAccountAddresses { wallet_address: String, chain_code: String },
    RefreshBalances { wallet_address: String, account_id: Option<u32>, chain_code: Option<String> },
    ImportBind { app_id: String, org_id: String, subaccount_uid: String, withdrawal_uid: String },
    ScanBind { app_id: String, org_id: String, subaccount_uid: String, withdrawal_uid: String },
    FetchPendingWithdrawOrders { withdrawal_uid: String, page_size: i64 },
    ReviewWithdrawOrders { trade_nos: Vec<String>, approve: bool },
    LoadTransferTargets { chain_code: String, sub_wallet_address: String },
    RunBatchTransfer { config: BatchTransferConfig },
}
