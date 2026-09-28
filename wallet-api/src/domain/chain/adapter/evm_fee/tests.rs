use super::{AdditionalFeePolicy, additional_fee_policy};
use wallet_types::chain::chain::ChainCode;

#[test]
fn only_scroll_requires_additional_fee_rpc_in_this_batch() {
    assert_eq!(additional_fee_policy(ChainCode::Scroll), AdditionalFeePolicy::ScrollRpc);
    for chain in [
        ChainCode::Ethereum,
        ChainCode::BnbSmartChain,
        ChainCode::Polygon,
        ChainCode::Avalanche,
        ChainCode::Arbitrum,
        ChainCode::Base,
    ] {
        assert_eq!(additional_fee_policy(chain), AdditionalFeePolicy::None);
    }
}
