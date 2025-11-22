use anyhow::Result;
use wallet_api::infrastructure::asset_calc::actor_model::AssetKey;
use wallet_types::chain::chain::ChainCode;

#[tokio::test]
async fn test_asset_key_functionality() -> Result<()> {
    // 测试AssetKey的基本功能
    
    // 测试原生资产
    let native_asset_key = AssetKey {
        wallet_address: "wallet_123".to_string(),
        address: "0x1234567890123456789012345678901234567890".to_string(),
        chain_code: ChainCode::Ethereum.to_string(),
        token_address: "".to_string()
    };
    
    // ChainCode::Ethereum.to_string() 返回的是 "eth" 而不是 "ethereum"
    assert_eq!(native_asset_key.chain_code, "eth");
    assert_eq!(native_asset_key.address, "0x1234567890123456789012345678901234567890");
    assert_eq!(native_asset_key.wallet_address, "wallet_123");
    assert_eq!(native_asset_key.token_address, "");
    
    // 测试代币资产
    let token_asset_key = AssetKey {
        wallet_address: "wallet_123".to_string(),
        address: "0x1234567890123456789012345678901234567890".to_string(),
        chain_code: ChainCode::Ethereum.to_string(),
        token_address: "0xdAC17F958D2ee523a2206206994597C13D831ec7".to_string()
    };
    
    // ChainCode::Ethereum.to_string() 返回的是 "eth" 而不是 "ethereum"
    assert_eq!(token_asset_key.chain_code, "eth");
    assert_eq!(token_asset_key.address, "0x1234567890123456789012345678901234567890");
    assert_eq!(token_asset_key.wallet_address, "wallet_123");
    assert_eq!(token_asset_key.token_address, "0xdAC17F958D2ee523a2206206994597C13D831ec7");
    
    // 测试gen_key方法
    let native_key = native_asset_key.gen_key();
    assert!(native_key.contains("eth"));
    assert!(native_key.contains("0x1234567890123456789012345678901234567890"));
    
    Ok(())
}

// 注意：ExchangeRateCache是私有的，不能在测试中直接访问
// 以下是测试被注释掉
// #[tokio::test]
// async fn test_exchange_rate_cache() -> Result<()> {
//     // 测试ExchangeRateCache的基本功能
//     // 由于ExchangeRateCache是私有的，此测试被禁用
//     Ok(())
// }

// 注意：AssetCalcState是私有的，不能在测试中直接访问
// 以下是测试被注释掉
// #[tokio::test]
// async fn test_asset_calc_state_basic() -> Result<()> {
//     // 由于AssetCalcState是私有的，此测试被禁用
//     Ok(())
// }