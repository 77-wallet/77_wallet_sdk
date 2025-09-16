#![feature(try_trait_v2, let_chains)]
#![allow(unreachable_code)]
#![allow(clippy::too_many_arguments)]
pub mod api;
pub mod app_state;
pub mod config;
pub(crate) mod default_data;
pub mod domain;
pub mod error;
pub mod infrastructure;

mod context;
mod data;
pub mod dirs;
pub mod manager;
pub mod messaging;

pub mod request;
pub mod response_vo;
pub mod service;
pub mod test;


pub use wallet_database::entities::api_wallet::ApiWalletType;

pub use wallet_database::entities::multisig_member::MemberVo;
pub use wallet_transport_backend::request::{
    TokenQueryHistoryPrice, TokenQueryPopularByPageReq, TokenQueryPrice, TokenQueryPriceReq,
};

pub use wallet_transport_backend::error::Error;