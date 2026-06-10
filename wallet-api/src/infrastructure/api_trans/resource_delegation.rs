use crate::{
    domain::{api_wallet::trans::ApiTransDomain, chain::adapter::ChainAdapterFactory},
    error::{service::ServiceError, system::SystemError},
    infrastructure::api_trans::{
        resource_amount::parse_resource_delegation_native_trx_units,
        resource_authorization::{
            ResourceDelegationSigner, new_tron_delegate_args, new_tron_undelegate_args,
            resolve_resource_delegation_signer,
        },
        resource_rpc_auth,
    },
};
use wallet_chain_interact::{
    BillResourceConsume,
    tron::operations::{RawTransactionParams, TronTxOperation},
};
use wallet_database::entities::{
    api_resource_delegation::{ApiResourceDelegationEntity, ApiResourceDelegationOperationType},
    api_resource_type::ApiResourceType,
};
use wallet_utils::RetryableError as _;

/// 资源代理重试退避：指数退避上限 1 小时。
/// - 第 0 次：60s
/// - 之后每次翻倍，最终上限 3600s
pub(crate) fn resource_delegation_retry_wait_secs(retry_count: i64) -> i64 {
    let exponent = retry_count.clamp(0, 6) as u32;
    (60_i64 * (1_i64 << exponent)).min(3600)
}

/// 资源代理失败 fact 的统一映射。
/// - 网络问题归入 ERR_6005
/// - 其他失败归入 ERR_6008
pub(crate) fn resource_delegation_failure_fact(err: &ServiceError) -> (String, String) {
    let err_code = if err.is_network_error() { "ERR_6005" } else { "ERR_6008" };
    (err_code.to_string(), err.to_string())
}

/// 使用统一 RPC-auth 策略执行资源代理广播。
///
/// 注意：
/// 1) 这里只做“执行一个 resource delegation 任务本身”，
/// 2) 不包含 collect/withdraw 主链推进逻辑。
/// 3) 是否立即触发下一步，由各自 shadow/stage 决定。
pub(crate) async fn execute_resource_delegation(
    delegation: &ApiResourceDelegationEntity,
    rpc_purpose: &'static str,
) -> Result<String, ServiceError> {
    resource_rpc_auth::run_with_rpc_auth_retry(
        &delegation.chain_code,
        rpc_purpose,
        &delegation.resource_trade_no,
        || execute_resource_delegation_once(delegation),
    )
    .await
}

// 资源任务执行内部实现：支持 Delegate / Undelegate 的 TRON 交易构建、签名与广播。
async fn execute_resource_delegation_once(
    delegation: &ApiResourceDelegationEntity,
) -> Result<String, ServiceError> {
    if !delegation.chain_code.eq_ignore_ascii_case("tron") {
        return Err(ServiceError::Parameter(format!(
            "resource delegation only supports tron, got {}",
            delegation.chain_code
        )));
    }

    let trx_amount = parse_resource_delegation_native_trx_units(&delegation.native_amount)?;
    let resource = match delegation.resource_type {
        ApiResourceType::Bandwidth => "bandwidth",
        _ => "energy",
    };
    let chain = ChainAdapterFactory::get_tron_adapter().await?;
    let _chain_rpc_guard =
        crate::infrastructure::chain_rpc_guard::acquire_if_guarded(&delegation.chain_code).await;
    let signer = resolve_resource_delegation_signer(delegation).await?;

    let raw = match delegation.operation_type {
        ApiResourceDelegationOperationType::Delegate => {
            let args = new_tron_delegate_args(
                &delegation.owner_address,
                &delegation.receiver_address,
                trx_amount,
                resource,
                signer.permission_id,
            )?;
            args.build_raw_transaction(chain.get_provider()).await?
        }
        ApiResourceDelegationOperationType::Undelegate => {
            let args = new_tron_undelegate_args(
                &delegation.owner_address,
                &delegation.receiver_address,
                trx_amount,
                resource,
                signer.permission_id,
            )?;
            args.build_raw_transaction(chain.get_provider()).await?
        }
    };

    let (tx_hash, raw_tx) = sign_tron_resource_delegation(delegation, &signer, raw).await?;
    let tx_resp =
        ApiTransDomain::broadcast_transfer(&delegation.chain_code, raw_tx, Some(&tx_hash)).await?;

    let Some(tx) = tx_resp else {
        return Err(ServiceError::Parameter(
            "resource delegation broadcast result uncertain".to_string(),
        ));
    };
    if tx.tx_hash != tx_hash {
        return Err(ServiceError::System(SystemError::Internal(
            "resource delegation tx_hash mismatch between build and broadcast".to_string(),
        )));
    }

    Ok(tx_hash)
}

async fn sign_tron_resource_delegation(
    delegation: &ApiResourceDelegationEntity,
    signer: &ResourceDelegationSigner,
    mut raw: RawTransactionParams,
) -> Result<(String, crate::domain::api_wallet::adapter::tx::RawTx), ServiceError> {
    let chain = ChainAdapterFactory::get_tron_adapter().await?;
    let provider = chain.get_provider();
    let consumer =
        provider.transfer_fee(&delegation.owner_address, None, &raw.raw_data_hex, 1).await?;
    let balance = chain.balance(&delegation.owner_address, None).await?;
    if balance.to::<i64>() < consumer.transaction_fee_i64() {
        return Err(ServiceError::Parameter(format!(
            "resource delegation balance is insufficient for tx fee: balance={}, need={}",
            balance,
            consumer.transaction_fee_i64()
        )));
    }

    let handles = crate::context::get_context()?.get_handles_arc().await?;
    let private_key_manager = handles.get_global_private_key_manager();
    let private_key =
        private_key_manager.get_private_key(&signer.signer_address, &delegation.chain_code).await?;
    let sign = wallet_utils::sign::sign_tron(&raw.tx_id, &private_key, None)?;
    raw.signature.push(sign);

    let tx_hash = raw.tx_id.clone();
    let raw_tx = crate::domain::api_wallet::adapter::tx::RawTx::Tron(
        raw,
        BillResourceConsume::new_tron(consumer.act_bandwidth() as u64, 0),
        consumer.transaction_fee(),
    );

    Ok((tx_hash, raw_tx))
}
