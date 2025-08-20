use super::adapter::TransactionAdapter;
use crate::{
    domain::{bill::BillDomain, coin::CoinDomain},
    infrastructure::task_queue::{self, BackendApiTaskData, task::Tasks},
    request::transaction::{self, Signer},
};
use wallet_chain_interact::{
    eth,
    sol::{self, SolFeeSetting},
    tron::{TronChain, protocol::account::AccountResourceDetail},
    types::ChainPrivateKey,
};
use wallet_database::entities::api_account::ApiAccountEntity;
use wallet_database::entities::api_assets::ApiAssetsEntity;
use wallet_database::repositories::api_account::ApiAccountRepo;
use wallet_database::repositories::api_assets::ApiAssetsRepo;
use wallet_database::{
    entities::{
        coin::CoinEntity,
    },
    repositories::permission::PermissionRepo,
};
use wallet_transport_backend::{
    api::{BackendApi, permission::TransPermission},
    consts::endpoint,
    request::PermissionData,
    response_vo::chain::GasOracle,
};
use wallet_types::constant::chain_code;
use wallet_utils::unit;
use wallet_database::entities::api_bill::{ApiBillEntity, ApiBillKind};
use wallet_database::entities::assets::AssetsId;
use wallet_database::repositories::api_bill::ApiBillRepo;
use crate::domain::api_wallet::bill::ApiBillDomain;

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
        let asset = AssetsEntity::assets_by_id(pool.as_ref(), &assets_id).await?;
        if let Some(asset) = asset {
            // 余额不一致
            if asset.balance != balance {
                // 更新本地余额后在上报后端
                AssetsEntity::update_balance(&*pool, &assets_id, balance)
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
            let res = BillDao::on_going_bill(chain_code::BTC, from, pool.as_ref()).await?;
            return Ok(!res.is_empty());
        };

        Ok(false)
    }

    /// transfer
    pub async fn transfer(
        mut params: transaction::TransferReq,
        bill_kind: BillKind,
        adapter: &TransactionAdapter,
    ) -> Result<String, crate::ServiceError> {
        //  check ongoing tx
        if Self::check_ongoing_bill(&params.base.from, &params.base.chain_code).await? {
            return Err(crate::BusinessError::Bill(
                crate::BillError::ExistsUnConfirmationTx,
            ))?;
        };

        let private_key = Self::get_key(
            &params.base.from,
            &params.base.chain_code,
            &params.password,
            &params.signer,
        )
        .await?;

        let coin = CoinDomain::get_coin(
            &params.base.chain_code,
            &params.base.symbol,
            params.base.token_address.clone(),
        )
        .await?;

        // params.base.with_token(coin.token_address());
        params.base.with_decimals(coin.decimals);

        let resp = adapter.transfer(&params, private_key).await?;

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

    pub async fn gas_oracle(
        chain_code: &str,
        provider: &eth::Provider,
        backend: &BackendApi,
    ) -> Result<GasOracle, crate::ServiceError> {
        let gas_oracle = backend.gas_oracle(chain_code).await;

        match gas_oracle {
            Ok(gas_oracle) => Ok(gas_oracle),
            Err(_) => {
                // unit is wei need to gwei
                let eth_fee = provider.get_default_fee().await?;

                let propose = eth_fee.base_fee + eth_fee.priority_fee_per_gas;
                let propose = unit::format_to_string(propose, eth::consts::ETH_GWEI)?;
                let base = unit::format_to_string(eth_fee.base_fee, eth::consts::ETH_GWEI)?;

                let gas_oracle = GasOracle {
                    safe_gas_price: None,
                    propose_gas_price: Some(propose),
                    fast_gas_price: None,
                    suggest_base_fee: Some(base),
                    gas_used_ratio: None,
                };

                Ok(gas_oracle)
            }
        }
    }

    pub async fn default_gas_oracle(
        provider: &eth::Provider,
    ) -> Result<GasOracle, crate::ServiceError> {
        let eth_fee = provider.get_default_fee().await?;

        let propose = eth_fee.base_fee + eth_fee.priority_fee_per_gas;
        let propose = unit::format_to_string(propose, eth::consts::ETH_GWEI)?;
        let base = unit::format_to_string(eth_fee.base_fee, eth::consts::ETH_GWEI)?;

        let gas_oracle = GasOracle {
            safe_gas_price: None,
            propose_gas_price: Some(propose),
            fast_gas_price: None,
            suggest_base_fee: Some(base),
            gas_used_ratio: None,
        };

        Ok(gas_oracle)
    }

    // check balance
    pub async fn check_eth_balance(
        from: &str,
        balance: alloy::primitives::U256,
        token: Option<&str>,
        chain: &eth::EthChain,
        transfer_amount: alloy::primitives::U256,
    ) -> Result<alloy::primitives::U256, crate::ServiceError> {
        let cost_main = match token {
            Some(token) => {
                let token_balance = chain.balance(from, Some(token.to_string())).await?;
                if token_balance < transfer_amount {
                    return Err(crate::BusinessError::Chain(
                        crate::ChainError::InsufficientBalance,
                    ))?;
                }
                balance
            }
            None => {
                if balance < transfer_amount {
                    return Err(crate::BusinessError::Chain(
                        crate::ChainError::InsufficientBalance,
                    ))?;
                }
                balance - transfer_amount
            }
        };
        Ok(cost_main)
    }

    // 针对sol 是否需要给优先费计算,目前给到usdt的优先费位 2倍基础费
    pub fn sol_priority_fee(fee_setting: &mut SolFeeSetting, token: Option<&String>, units: u64) {
        if let Some(_token) = token {
            fee_setting.compute_units_consumed = units;
            fee_setting.priority_fee_per_compute_unit = Some(fee_setting.base_fee * 20);
        }
    }

    /// if main coin transfer return reduce transfer amount remain balance
    /// else return balance
    pub async fn check_sol_balance(
        from: &str,
        balance: alloy::primitives::U256,
        token: Option<&str>,
        chain: &sol::SolanaChain,
        transfer_amount: alloy::primitives::U256,
    ) -> Result<alloy::primitives::U256, crate::ServiceError> {
        let cost_main = match token {
            Some(token) => {
                let token_balance = chain.balance(from, Some(token.to_string())).await?;
                if token_balance < transfer_amount {
                    return Err(crate::BusinessError::Chain(
                        crate::ChainError::InsufficientBalance,
                    ))?;
                }
                balance
            }
            None => {
                if balance < transfer_amount {
                    return Err(crate::BusinessError::Chain(
                        crate::ChainError::InsufficientBalance,
                    ))?;
                }
                balance - transfer_amount
            }
        };
        Ok(cost_main)
    }

    // check transaction_fee
    pub fn check_sol_transaction_fee(
        balance: alloy::primitives::U256,
        fee: u64,
    ) -> Result<(), crate::ServiceError> {
        let fee = alloy::primitives::U256::from(fee);

        if balance < fee {
            return Err(crate::BusinessError::Chain(
                crate::ChainError::InsufficientFeeBalance,
            ))?;
        }
        Ok(())
    }

    pub fn handle_btc_fee_error(err: wallet_chain_interact::Error) -> crate::ServiceError {
        match err {
            wallet_chain_interact::Error::UtxoError(
                wallet_chain_interact::UtxoError::InsufficientBalance,
            ) => crate::BusinessError::Chain(crate::ChainError::InsufficientBalance).into(),
            wallet_chain_interact::Error::UtxoError(
                wallet_chain_interact::UtxoError::InsufficientFee(_fee),
            ) => crate::BusinessError::Chain(crate::ChainError::InsufficientFeeBalance).into(),
            wallet_chain_interact::Error::UtxoError(
                wallet_chain_interact::UtxoError::ExceedsMaximum,
            ) => crate::BusinessError::Chain(crate::ChainError::ExceedsMaximum).into(),
            wallet_chain_interact::Error::UtxoError(wallet_chain_interact::UtxoError::DustTx) => {
                crate::BusinessError::Chain(crate::ChainError::DustTransaction).into()
            }
            wallet_chain_interact::Error::UtxoError(
                wallet_chain_interact::UtxoError::ExceedsMaxFeeRate,
            ) => crate::BusinessError::Chain(crate::ChainError::ExceedsMaxFeerate).into(),
            _ => err.into(),
        }
    }

    // 转账金额不能小于对应币种精度的最小金额,并返回最小金额的u256表示
    pub fn check_min_transfer(
        value: &str,
        decimal: u8,
    ) -> Result<alloy::primitives::U256, crate::ServiceError> {
        let min = alloy::primitives::U256::from(1);
        let transfer_amount = unit::convert_to_u256(value, decimal)?;

        if transfer_amount < min {
            return Err(crate::BusinessError::Chain(
                crate::ChainError::AmountLessThanMin,
            ))?;
        }
        Ok(transfer_amount)
    }

    // 如果传入了signer 则使用signer的私钥
    pub async fn get_key(
        from: &str,
        chain_code: &str,
        password: &str,
        signer: &Option<Signer>,
    ) -> Result<ChainPrivateKey, crate::ServiceError> {
        let address = if let Some(signer) = signer {
            signer.address.clone()
        } else {
            from.to_string()
        };

        let key = crate::domain::account::open_subpk_with_password(chain_code, &address, password)
            .await?;

        Ok(key)
    }

    // 后期加入缓存
    pub async fn account_resource(
        chain: &TronChain,
        owner_address: &str,
    ) -> Result<AccountResourceDetail, crate::ServiceError> {
        let resource = chain.account_resource(owner_address).await?;
        Ok(resource)
    }
}
