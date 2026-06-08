use crate::batch_transfer::BatchTransferSummary;

use super::events::{
    AccountAddressRow, ApiWalletRow, BalanceAssetRow, ClientRuntimeInfo, WithdrawOrderRow,
};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum WorkerEvent {
    Started,
    Failed { error: String },
    Log { message: String },
    Notify { payload: String },
    ImportFinished { messages: Vec<String> },
    RuntimeInfoLoaded { info: ClientRuntimeInfo },
    WalletsLoaded { wallets: Vec<ApiWalletRow> },
    AccountAddressesLoaded { wallet_address: String, rows: Vec<AccountAddressRow> },
    BalanceAssetsLoaded { wallet_address: String, rows: Vec<BalanceAssetRow> },
    WithdrawOrdersLoaded { rows: Vec<WithdrawOrderRow> },
    WithdrawReviewFinished,
    LoadedTargets { targets: Vec<String> },
    TransferFinished { summary: BatchTransferSummary },
}
