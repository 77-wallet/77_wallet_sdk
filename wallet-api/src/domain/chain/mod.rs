use crate::response_vo;
use reqwest::Url;
use wallet_chain_interact::{btc::ParseBtcAddress, eth::FeeSetting};
use wallet_types::chain::network;

pub mod adapter;
pub mod transaction;

/// Parses a fee setting string into a `FeeSetting` struct.
pub fn pare_fee_setting(fee_setting: &str) -> Result<FeeSetting, crate::ServiceError> {
    fee_setting
        .try_into()
        .and_then(|s: response_vo::EthereumFeeDetails| FeeSetting::try_from(s))
}

pub fn rpc_need_header(url: &str) -> Result<bool, crate::ServiceError> {
    let url = Url::parse(url).expect("Invalid URL");

    Ok(url.host_str() == Some(wallet_transport_backend::consts::BASE_RPC_URL))
}

pub fn check_address(
    address: &str,
    chain: wallet_types::chain::chain::ChainCode,
    network: network::NetworkKind,
) -> Result<(), crate::error::ServiceError> {
    match chain {
        wallet_types::chain::chain::ChainCode::Bitcoin => {
            let parse = ParseBtcAddress::new(network);
            parse.parse_address(address).map(|_| true).map_err(|_| {
                crate::BusinessError::Account(crate::AccountError::AddressNotCorrect)
            })?
        }
        wallet_types::chain::chain::ChainCode::BnbSmartChain
        | wallet_types::chain::chain::ChainCode::Ethereum => {
            wallet_utils::address::parse_eth_address(address)
                .map(|_| true)
                .map_err(|_| {
                    crate::BusinessError::Account(crate::AccountError::AddressNotCorrect)
                })?
        }
        wallet_types::chain::chain::ChainCode::Tron => {
            if wallet_utils::address::is_tron_address(address) {
                true
            } else {
                return Err(crate::BusinessError::Account(
                    crate::AccountError::AddressNotCorrect,
                ))?;
            }
        }
        wallet_types::chain::chain::ChainCode::Solana => {
            wallet_utils::address::parse_sol_address(address)
                .map(|_| true)
                .map_err(|_| {
                    crate::BusinessError::Account(crate::AccountError::AddressNotCorrect)
                })?
        }
    };
    Ok(())
}
