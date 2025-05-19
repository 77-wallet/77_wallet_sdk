use crate::{
    infrastructure::task_queue::{BackendApiTask, BackendApiTaskData, Task, Tasks},
    response_vo,
};
use wallet_chain_interact::{
    btc::ParseBtcAddress, dog::ParseDogAddress, eth::FeeSetting, ltc::ParseLtcAddress,
    ton::address::parse_addr_from_bs64_url, BillResourceConsume,
};
use wallet_database::repositories::{account::AccountRepoTrait, chain::ChainRepoTrait};
use wallet_transport_backend::request::ChainRpcListReq;
use wallet_types::chain::network;

pub mod adapter;
pub mod transaction;

pub struct TransferResp {
    pub tx_hash: String,
    pub fee: String,
    pub consumer: Option<BillResourceConsume>,
}
impl TransferResp {
    pub fn new(tx_hash: String, fee: String) -> Self {
        Self {
            tx_hash,
            fee,
            consumer: None,
        }
    }
    pub fn with_consumer(&mut self, consumer: BillResourceConsume) {
        self.consumer = Some(consumer);
    }

    pub fn resource_consume(&self) -> Result<String, crate::ServiceError> {
        if let Some(consumer) = &self.consumer {
            Ok(consumer.to_json_str()?)
        } else {
            Ok(String::new())
        }
    }
}

/// Parses a fee setting string into a `FeeSetting` struct.
pub fn pare_fee_setting(fee_setting: &str) -> Result<FeeSetting, crate::ServiceError> {
    fee_setting
        .try_into()
        .and_then(|s: response_vo::EthereumFeeDetails| FeeSetting::try_from(s))
}

pub fn rpc_need_header(_url: &str) -> Result<bool, crate::ServiceError> {
    // let url = Url::parse(url).expect("Invalid URL");
    // Ok(url.host_str() == Some(wallet_transport_backend::consts::BASE_RPC_URL))
    Ok(true)
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
        wallet_types::chain::chain::ChainCode::Ton => parse_addr_from_bs64_url(address)
            .map(|_| true)
            .map_err(|_| crate::BusinessError::Account(crate::AccountError::AddressNotCorrect))?,
        wallet_types::chain::chain::ChainCode::Litecoin => {
            let parse = ParseLtcAddress::new(network);
            parse.parse_address(address).map(|_| true).map_err(|_| {
                crate::BusinessError::Account(crate::AccountError::AddressNotCorrect)
            })?
        }
        wallet_types::chain::chain::ChainCode::Dogcoin => {
            let parse = ParseDogAddress::new(network);
            parse.parse_address(address).map(|_| true).map_err(|_| {
                crate::BusinessError::Account(crate::AccountError::AddressNotCorrect)
            })?
        }
    };
    Ok(())
}

pub struct ChainDomain;

impl ChainDomain {
    pub(crate) async fn upsert_multi_chain_than_toggle(
        chains: wallet_transport_backend::response_vo::chain::ChainList,
    ) -> Result<bool, crate::ServiceError> {
        tracing::warn!("upsert_multi_chain_than_toggle, chains: {:#?}", chains);
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());

        // // 本地后端节点
        // let local_backend_nodes =
        //     wallet_database::repositories::node::NodeRepoTrait::list(&mut repo, Some(0)).await?;

        // // 本地配置节点
        // let default_nodes =
        //     wallet_database::repositories::node::NodeRepoTrait::list(&mut repo, Some(1)).await?;

        let mut input = Vec::new();
        let mut chain_codes = Vec::new();
        let mut has_new_chain = false;
        let account_list = AccountRepoTrait::list(&mut repo).await?;
        let app_version = super::app::config::ConfigDomain::get_app_version()
            .await?
            .app_version;
        for chain in chains.list {
            let Some(master_token_code) = chain.master_token_code else {
                continue;
            };

            let status = match (
                super::app::config::ConfigDomain::compare_versions(
                    &app_version,
                    &chain.app_version_code,
                ),
                chain.enable,
            ) {
                (std::cmp::Ordering::Less, _) => 0,
                (_, true) => 1,
                (_, false) => 0,
            };

            if account_list
                .iter()
                .all(|acc_chain| acc_chain.chain_code != chain.chain_code && chain.enable)
            {
                has_new_chain = true;
            }

            // if local_backend_nodes
            //     .iter()
            //     .any(|node| node.chain_code == chain.chain_code)
            // {
            //     input.push(
            //         wallet_database::entities::chain::ChainCreateVo::new(
            //             &chain.name,
            //             &chain.chain_code,
            //             &[],
            //             &master_token_code,
            //         )
            //         .with_status(status),
            //     );
            // } else if default_nodes
            //     .iter()
            //     .any(|node| node.chain_code == chain.chain_code)
            // {
            //     input.push(
            //         wallet_database::entities::chain::ChainCreateVo::new(
            //             &chain.name,
            //             &chain.chain_code,
            //             &[],
            //             &master_token_code,
            //         )
            //         .with_status(status),
            //     );
            // }

            input.push(
                wallet_database::entities::chain::ChainCreateVo::new(
                    &chain.name,
                    &chain.chain_code,
                    &[],
                    &master_token_code,
                )
                .with_status(status),
            );
            if status == 1 {
                chain_codes.push(chain.chain_code);
            }
        }

        ChainRepoTrait::upsert_multi_chain(&mut repo, input).await?;
        Self::toggle_chains(&mut repo, &chain_codes).await?;
        let chain_rpc_list_req = BackendApiTaskData::new(
            wallet_transport_backend::consts::endpoint::CHAIN_RPC_LIST,
            &ChainRpcListReq::new(chain_codes),
        )?;
        Tasks::new()
            .push(Task::BackendApi(BackendApiTask::BackendApi(
                chain_rpc_list_req,
            )))
            .send()
            .await?;

        Ok(has_new_chain)
    }

    pub(crate) async fn toggle_chains(
        repo: &mut wallet_database::repositories::ResourcesRepo,
        chain_codes: &[String],
    ) -> Result<(), crate::ServiceError> {
        wallet_database::repositories::chain::ChainRepoTrait::toggle_chains_status(
            repo,
            chain_codes,
        )
        .await?;
        Ok(())
    }

    // pub(crate) async fn chain_node_info_left_join(
    //     repo: &mut wallet_database::repositories::ResourcesRepo,
    //     chain_code: &str,
    // ) -> Result<Option<ChainWithNode>, crate::ServiceError> {
    //     let chain = repo.chain_node_info_left_join(chain_code).await?;
    //     if let Some(chain) = chain {
    //         if chain.node_id.is_empty() {
    //             let existing_nodes = NodeRepoTrait::list(repo, Some(1)).await?;
    //             let existing_node = existing_nodes
    //                 .into_iter()
    //                 .find(|node| node.chain_code == chain_code);

    //             let chain = ChainRepoTrait::detail(repo, chain_code).await?;
    //             if let Some(chain) = chain
    //                 && let Some(existing_node) = existing_node
    //             {
    //                 Ok(Some((chain, existing_node).into()))
    //             } else {
    //                 Ok(None)
    //             }
    //         } else {
    //             Ok(Some(chain))
    //         }
    //     } else {
    //         Ok(None)
    //     }
    // }
}
