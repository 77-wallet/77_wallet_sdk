use crate::{
    domain::{api_wallet::assets::ApiAssetsDomain, assets::AssetsDomain},
    response_vo::api_wallet::assets::ApiAccountChainAssetList,
};

#[derive(Debug, Clone)]
pub struct AddressChainCode {
    pub address: String,
    pub chain_code: String,
}

pub struct ApiAssetsService {}

impl ApiAssetsService {
    pub fn new() -> Self {
        Self {}
    }

    // 根据地址来同步余额(链)
    pub async fn sync_assets_by_addr(
        self,
        addr: Vec<String>,
        chain_code: Option<String>,
        symbol: Vec<String>,
    ) -> Result<(), crate::error::service::ServiceError> {
        AssetsDomain::sync_assets_by_addr_chain(addr, chain_code, symbol).await
    }

    // 从后端同步余额(后端)
    pub async fn sync_assets_from_backend(
        self,
        addr: String,
        chain_code: Option<String>,
        _symbol: Vec<String>,
    ) -> Result<(), crate::error::service::ServiceError> {
        AssetsDomain::async_balance_from_backend_addr(addr, chain_code).await
    }

    // 根据钱包地址来同步资产余额
    pub async fn sync_assets_by_wallet_chain(
        self,
        wallet_address: &str,
        account_id: Option<u32>,
        _symbol: Vec<String>,
    ) -> Result<(), crate::error::service::ServiceError> {
        ApiAssetsDomain::sync_assets_by_wallet(wallet_address, account_id, _symbol).await
    }

    pub async fn sync_assets_by_wallet_backend(
        self,
        wallet_address: String,
        account_id: Option<u32>,
        _symbol: Vec<String>,
    ) -> Result<(), crate::error::service::ServiceError> {
        AssetsDomain::async_balance_from_backend_wallet(wallet_address, account_id).await
    }

    pub async fn get_api_assets_list(
        self,
        wallet_address: String,
        account_id: Option<u32>,
        chain_code: Option<String>,
        is_multisig: Option<bool>,
    ) -> Result<ApiAccountChainAssetList, crate::error::service::ServiceError> {
        // let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // let chain_codes = chain_code.clone().map(|c| vec![c]).unwrap_or_default();
        // let account_addresses = self
        //     .account_domain
        //     .get_addresses(&mut tx, address, account_id, chain_codes, is_multisig)
        //     .await?;

        // let mut res = AccountChainAssetList::default();
        // let token_currencies = self.coin_domain.get_token_currencies_v2(&mut tx).await?;

        // // 根据账户地址、网络查询币资产
        // for address in account_addresses {
        //     let assets_list: Vec<AssetsEntity> = tx
        //         .get_chain_assets_by_address_chain_code_symbol(
        //             vec![address.address],
        //             Some(address.chain_code),
        //             None,
        //             None,
        //         )
        //         .await?;
        //     for assets in assets_list {
        //         let coin = CoinDomain::get_coin(
        //             &assets.chain_code,
        //             &assets.symbol,
        //             assets.token_address(),
        //         )
        //         .await?;
        //         if let Some(existing_asset) = res
        //             .iter_mut()
        //             .find(|a| a.symbol == assets.symbol && a.is_default && coin.is_default == 1)
        //         {
        //             token_currencies.calculate_assets(assets, existing_asset).await?;
        //             existing_asset
        //                 .chain_list
        //                 .entry(coin.chain_code.clone())
        //                 .or_insert(coin.token_address.unwrap_or_default());
        //         } else {
        //             let balance = token_currencies.calculate_assets_entity(&assets).await?;

        //             let chain_code = if chain_code.is_none()
        //                 && let Some(chain) = tx.detail_with_main_symbol(&assets.symbol).await?
        //             {
        //                 chain.chain_code
        //             } else {
        //                 assets.chain_code
        //             };

        //             res.push(AccountChainAsset {
        //                 chain_code: chain_code.clone(),
        //                 symbol: assets.symbol,
        //                 name: assets.name,
        //                 chain_list: ChainList(HashMap::from([(chain_code, assets.token_address)])),
        //                 balance,
        //                 is_multisig: assets.is_multisig, // chains: vec![chain_assets],
        //                 is_default: coin.is_default == 1,
        //             })
        //         }
        //     }
        // }

        // // 过滤掉multisig的资产
        // if let Some(is_multisig) = is_multisig {
        //     res.retain(|asset| {
        //         if is_multisig {
        //             asset.is_multisig == 1
        //         } else {
        //             asset.is_multisig == 0 || asset.is_multisig == 2
        //         }
        //     });
        // }
        // // res.mark_multichain_assets();
        // res.sort_account_chain_assets();
        // Ok(res)
        todo!()
    }
}
