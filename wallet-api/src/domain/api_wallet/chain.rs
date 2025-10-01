use wallet_database::entities::{api_wallet::ApiWalletType, coin::CoinEntity};
use wallet_transport_backend::request::{
    TokenQueryPriceReq, api_wallet::address::ApiAddressInitReq,
};
use wallet_types::chain::chain::ChainCode;

use crate::domain::{
    api_wallet::account::ApiAccountDomain, assets::AssetsDomain, chain::ChainDomain,
    wallet::WalletDomain,
};

pub struct ApiChainDomain {}

impl ApiChainDomain {
    pub(crate) async fn init_chains_api_assets(
        coins: &[CoinEntity],
        req: &mut TokenQueryPriceReq,
        api_address_init_req: &mut ApiAddressInitReq,
        // expand_address_req: &mut AddressBatchInitReq,
        // subkeys: &mut Vec<wallet_tree::file_ops::BulkSubkey>,
        chain_list: &[String],
        seed: &[u8],
        account_index_map: &wallet_utils::address::AccountIndexMap,
        uid: &str,
        wallet_address: &str,
        account_name: &str,
        is_default_name: bool,
        wallet_password: &str,
        api_wallet_type: ApiWalletType,
    ) -> Result<(), crate::error::service::ServiceError> {
        for chain in chain_list.iter() {
            // let index = account_index_map.input_index;
            // let mut params = AddressParam::new(index);

            let code: ChainCode = chain.as_str().try_into()?;
            let address_types = WalletDomain::address_type_by_chain(code);

            let Ok(node) = ChainDomain::get_node(chain).await else {
                continue;
            };

            for address_type in address_types {
                let instance: wallet_chain_instance::instance::ChainObject =
                    (&code, &address_type, node.network.as_str().into()).try_into()?;
                // (&code, &address_type, "mainnet".into()).try_into()?;
                let (account_address, address_init_req) = ApiAccountDomain::derive_subkey(
                    uid,
                    seed,
                    wallet_address,
                    account_index_map,
                    &instance,
                    account_name,
                    is_default_name,
                    wallet_password,
                    api_wallet_type,
                )
                .await?;

                if let Some(address_init_req) = address_init_req {
                    api_address_init_req.address_list.add_address(address_init_req);
                    // params.push(&account_address.address);
                } else {
                    tracing::info!("不上报： {}", account_address);
                };

                // subkeys.push(
                //     AccountDomain::generate_subkey(
                //         &instance,
                //         seed,
                //         &account_address.address,
                //         &code.to_string(),
                //         account_index_map,
                //         derivation_path.as_str(),
                //     )
                //     .await?,
                // );

                AssetsDomain::init_default_api_assets(
                    coins,
                    &account_address,
                    &code.to_string(),
                    req,
                )
                .await?;
            }

            // if !params.address_list.is_empty() {
            //     expand_address_req.add_chain_code(chain, params);
            // }
        }

        Ok(())
    }
}
