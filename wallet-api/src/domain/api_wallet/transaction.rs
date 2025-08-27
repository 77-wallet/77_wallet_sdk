use crate::domain::api_wallet::account::ApiAccountDomain;
use crate::domain::api_wallet::adapter_factory::ApiChainAdapterFactory;
use crate::domain::chain::ChainDomain;
use crate::response_vo::{FeeDetails, FeeDetailsVo};
use crate::{
    domain::{
        api_wallet::{adapter::Tx, adapter_factory::API_ADAPTER_FACTORY, bill::ApiBillDomain},
        coin::CoinDomain,
    },
    infrastructure::task_queue::{self, BackendApiTaskData, task::Tasks},
    request::transaction::{self, Signer},
};
use wallet_chain_interact::{
    tron::{TronChain, protocol::account::AccountResourceDetail},
    types::ChainPrivateKey,
};
use wallet_database::entities::chain::ChainEntity;
use wallet_database::{
    entities::{
        api_account::ApiAccountEntity,
        api_assets::ApiAssetsEntity,
        api_bill::{ApiBillEntity, ApiBillKind},
        assets::AssetsId,
        coin::CoinEntity,
    },
    repositories::{
        api_account::ApiAccountRepo, api_assets::ApiAssetsRepo, api_bill::ApiBillRepo,
        permission::PermissionRepo,
    },
};
use wallet_transport_backend::{
    api::permission::TransPermission, consts::endpoint, request::PermissionData,
};
use wallet_types::constant::chain_code;

// sol 默认计算单元
pub const DEFAULT_UNITS: u64 = 100_000;

pub struct ApiChainTransDomain;

impl ApiChainTransDomain {
    pub async fn assets(
        chain_code: &str,
        symbol: &str,
        from: &str,
        token_address: Option<String>,
    ) -> Result<ApiAssetsEntity, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let assets_id = AssetsId {
            address: from.to_string(),
            chain_code: chain_code.to_string(),
            symbol: symbol.to_string(),
            token_address,
        };
        let assets = ApiAssetsRepo::find_by_id(&pool, &assets_id)
            .await?
            .ok_or(crate::BusinessError::Assets(crate::AssetsError::NotFound))?;

        Ok(assets)
    }

    pub async fn account(
        chain_code: &str,
        address: &str,
    ) -> Result<ApiAccountEntity, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let account = ApiAccountRepo::find_one_by_address_chain_code(address, chain_code, &pool)
            .await?
            .ok_or(crate::BusinessError::Account(
                crate::AccountError::NotFound(address.to_string()),
            ))?;
        Ok(account)
    }

    pub async fn update_balance(
        address: &str,
        chain_code: &str,
        symbol: &str,
        token_address: Option<String>,
        balance: &str,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let assets_id = AssetsId {
            address: address.to_string(),
            chain_code: chain_code.to_string(),
            symbol: symbol.to_string(),
            token_address,
        };

        // 查询余额
        let asset = ApiAssetsRepo::find_by_id(&pool, &assets_id).await?;
        if let Some(asset) = asset {
            // 余额不一致
            if asset.balance != balance {
                // 更新本地余额后在上报后端
                ApiAssetsRepo::update_balance(&pool, &assets_id, balance)
                    .await
                    .map_err(crate::ServiceError::Database)?;

                // 上报后端修改余额
                let backend = crate::manager::Context::get_global_backend_api()?;
                let rs = backend
                    .wallet_assets_refresh_bal(address, chain_code, symbol)
                    .await;
                if let Err(e) = rs {
                    tracing::warn!("upload balance refresh error = {}", e);
                }
            }
        }

        Ok(())
    }

    pub async fn main_coin(chain_code: &str) -> Result<CoinEntity, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let coin = CoinEntity::main_coin(chain_code, pool.as_ref())
            .await?
            .ok_or(crate::BusinessError::Coin(
                crate::error::business::coin::CoinError::NotFound(format!(
                    "chian = {}",
                    chain_code
                )),
            ))?;
        Ok(coin)
    }

    // btc 验证是否存在未确认的交易
    async fn check_ongoing_bill(from: &str, chain_code: &str) -> Result<bool, crate::ServiceError> {
        let pool = crate::Context::get_global_sqlite_pool()?;

        if chain_code == chain_code::BTC {
            let res = ApiBillRepo::on_going_bill(chain_code::BTC, from, &pool).await?;
            return Ok(!res.is_empty());
        };

        Ok(false)
    }

    /// transfer
    pub async fn transfer(
        mut params: transaction::TransferReq,
        bill_kind: ApiBillKind,
    ) -> Result<String, crate::ServiceError> {
        //  check ongoing tx
        if Self::check_ongoing_bill(&params.base.from, &params.base.chain_code).await? {
            return Err(crate::BusinessError::Bill(
                crate::BillError::ExistsUnConfirmationTx,
            ))?;
        };

        tracing::info!("transfer ------------------- 7:");
        let private_key = ApiAccountDomain::get_private_key(
            &params.base.from,
            &params.base.chain_code,
            &params.password,
        )
        .await?;

        tracing::info!("transfer ------------------- 8:");

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let chain = ChainEntity::detail(pool.as_ref(), &params.base.chain_code)
            .await?
            .ok_or(crate::BusinessError::Chain(crate::ChainError::NotFound(
                params.base.chain_code.to_string(),
            )))?;

        let coin = CoinDomain::get_coin(
            &params.base.chain_code,
            &params.base.symbol,
            params.base.token_address.clone(),
        )
        .await?;

        // params.base.with_token(coin.token_address());
        params.base.with_decimals(coin.decimals);

        tracing::info!("get_coin ------------------- 9:");
        let adapter = API_ADAPTER_FACTORY
            .get_or_init(|| async { ApiChainAdapterFactory::new().await.unwrap() })
            .await
            .get_transaction_adapter(params.base.chain_code.as_str())
            .await?;

        let resp = adapter.transfer(&params, private_key).await?;

        tracing::info!("get_coin ------------------- 10:");
        let mut new_bill = ApiBillEntity::try_from(&params)?;
        new_bill.tx_kind = bill_kind;
        new_bill.hash = resp.tx_hash.clone();
        new_bill.resource_consume = resp.resource_consume()?;
        new_bill.transaction_fee = resp.fee;

        // 如果使用了权限，上报给后端
        if let Some(signer) = params.signer {
            let pool = crate::Context::get_global_sqlite_pool()?;
            let permission = PermissionRepo::permission_with_user(
                &pool,
                &params.base.from,
                signer.permission_id,
                false,
            )
            .await?
            .ok_or(crate::BusinessError::Permission(
                crate::PermissionError::ActivesPermissionNotFound,
            ))?;

            let users = permission.users();

            let params = TransPermission {
                address: params.base.from,
                chain_code: params.base.chain_code,
                tx_kind: bill_kind.to_i8(),
                hash: resp.tx_hash.clone(),
                permission_data: PermissionData {
                    opt_address: signer.address.to_string(),
                    users: users.clone(),
                },
            };

            let task = task_queue::BackendApiTask::BackendApi(BackendApiTaskData::new(
                endpoint::UPLOAD_PERMISSION_TRANS,
                &params,
            )?);
            let _ = Tasks::new().push(task).send().await;

            new_bill.signer = users.join(",");
        }

        ApiBillDomain::create_bill(new_bill).await?;

        if let Some(request_id) = params.base.request_resource_id {
            let backend = crate::manager::Context::get_global_backend_api()?;
            let _ = backend.delegate_complete(&request_id).await;
        }

        Ok(resp.tx_hash)
    }
}
