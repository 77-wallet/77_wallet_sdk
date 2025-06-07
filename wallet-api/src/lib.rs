#![feature(try_trait_v2, let_chains)]
#![allow(unreachable_code)]
#![allow(clippy::too_many_arguments)]
pub mod api;
pub mod app_state;
pub mod config;
pub(crate) mod default_data;
pub mod domain;
mod error;
pub(crate) mod infrastructure;
pub use infrastructure::log::*;
mod manager;
mod messaging;
pub use messaging::notify::event::NotifyEvent;
pub use messaging::notify::FrontendNotifyEvent;
pub mod request;
mod response;
pub mod response_vo;
pub mod service;
pub mod test;

pub use error::{
    business::{
        account::AccountError, announcement::AnnouncementError, assets::AssetsError,
        bill::BillError, chain::ChainError, chain_node::ChainNodeError, coin::CoinError,
        config::ConfigError, device::DeviceError, exchange_rate::ExchangeRate,
        multisig_account::MultisigAccountError, multisig_queue::MultisigQueueError,
        permission::PermissionError, stake::StakeError, wallet::WalletError, BusinessError,
    },
    system::SystemError,
    Errors, ServiceError,
};

pub use config::*;
pub use manager::{Context, Dirs, WalletManager};
pub use request::assets::GetChain;
pub use request::{
    account::CreateAccountReq, app::UploadLogFileReq, devices::InitDeviceReq,
    wallet::CreateWalletReq,
};
pub use test::net_api;
pub use wallet_database::entities::multisig_member::MemberVo;
pub use wallet_transport_backend::request::{
    TokenQueryHistoryPrice, TokenQueryPopularByPageReq, TokenQueryPrice, TokenQueryPriceReq,
};
