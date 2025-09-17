#![allow(unused_variables)]
pub mod account;
pub mod address_book;
pub mod announcement;
pub mod api_account;
pub mod api_assets;
pub mod api_collect;
pub mod api_collect_strategy;
pub mod api_fee;
pub mod api_nonce;
pub mod api_wallet;
pub mod api_withdraw;
pub mod api_withdraw_strategy;
pub mod assets;
pub mod bill;
pub mod chain;
pub mod coin;
pub mod config;
pub mod device;
pub mod exchange_rate;
pub mod multisig_account;
pub mod multisig_member;
pub mod multisig_queue;
pub mod multisig_signatures;
pub mod node;
pub mod permission;
pub mod permission_user;
pub mod stake;
pub mod system_notification;
pub mod task_queue;
pub mod wallet;

// 是否过期
fn has_expiration(timestamp: i64, chain_code: wallet_types::chain::chain::ChainCode) -> bool {
    match chain_code {
        // 1天
        wallet_types::chain::chain::ChainCode::Bitcoin => {
            timestamp < chrono::Utc::now().timestamp() - 86400
        }
        // 10分钟
        wallet_types::chain::chain::ChainCode::Solana => {
            timestamp < chrono::Utc::now().timestamp() - (10 * 60)
        }
        // 30分钟
        wallet_types::chain::chain::ChainCode::Ethereum => {
            timestamp < chrono::Utc::now().timestamp() - (30 * 60)
        }
        // 30分钟
        wallet_types::chain::chain::ChainCode::Tron => {
            timestamp < chrono::Utc::now().timestamp() - (30 * 60)
        }
        // 30分钟
        wallet_types::chain::chain::ChainCode::BnbSmartChain => {
            timestamp < chrono::Utc::now().timestamp() - (30 * 60)
        }
        _ => {
            // 默认 给到30分钟
            timestamp < chrono::Utc::now().timestamp() - (30 * 60)
        }
    }
}
