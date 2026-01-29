use crate::{
    domain::{
        api_wallet::{
            account::ApiAccountDomain, adapter::tx::RawTx, adapter_factory::ApiChainAdapterFactory,
        },
        chain::TransferResp,
    },
    error::service::ServiceError,
    request::api_wallet::trans::ApiTransferReq,
};
use std::time::Instant;
use wallet_chain_interact::types::ChainPrivateKey;

pub(crate) mod collect;
pub(crate) mod fee;
pub(crate) mod withdraw;

pub(crate) struct ApiTransDomain {}

impl ApiTransDomain {
    /// transfer
    pub async fn transfer(
        params: ApiTransferReq,
        preloaded_private_key: Option<ChainPrivateKey>,
    ) -> Result<TransferResp, ServiceError> {
        let start_time = Instant::now();
        tracing::info!(
            "transfer (开始): 请求ID: {:?}, 链: {}, 时间: {:?}",
            params.base.request_resource_id,
            params.base.chain_code,
            start_time
        );

        tracing::info!("transfer: 获取私钥");
        let private_key_time = Instant::now();
        let private_key = match preloaded_private_key {
            Some(pk) => pk,
            None => {
                ApiAccountDomain::get_private_key(&params.base.from, &params.base.chain_code)
                    .await?
            }
        };
        tracing::info!("transfer: 获取私钥完成, 耗时: {:?}", private_key_time.elapsed());

        tracing::info!("transfer: 原始链代码: {}", params.base.chain_code);
        let chain_code_time = Instant::now();
        let chain_code = params.base.chain_code.as_str();
        tracing::info!(
            "transfer: 转换后链代码: {}, 耗时: {:?}",
            chain_code,
            chain_code_time.elapsed()
        );

        let adapter_time = Instant::now();
        let adapter = ApiChainAdapterFactory::get_transaction_adapter(chain_code).await?;
        tracing::info!("transfer (适配器创建): 完成, 耗时: {:?}", adapter_time.elapsed());

        tracing::info!("transfer: 执行转账");
        // TODO：可优化
        let transfer_time = Instant::now();
        let resp = adapter.transfer(&params, private_key).await?;
        tracing::info!("transfer: 转账操作完成, 耗时: {:?}", transfer_time.elapsed());

        if let Some(request_id) = params.base.request_resource_id {
            tracing::info!("transfer (委托完成): 开始, request_id: {}", request_id);
            let delegate_time = Instant::now();
            let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
            let _ = backend.delegate_complete(&request_id).await;
            tracing::info!("transfer (委托完成): 结束, 耗时: {:?}", delegate_time.elapsed());
        }

        tracing::info!("transfer (结束): 总耗时: {:?}", start_time.elapsed());
        Ok(resp)
    }

    /// 构建原始交易
    ///
    /// 职责：
    /// - 只负责构建交易，生成raw_tx和tx_hash
    /// - 不处理Recover逻辑
    /// - 不负责从链上恢复交易状态
    pub async fn build_transfer_raw(
        params: ApiTransferReq,
        preloaded_private_key: Option<ChainPrivateKey>,
    ) -> Result<(String, RawTx, String), ServiceError> {
        let start_time = Instant::now();
        tracing::info!(
            "transfer (开始): 请求ID: {:?}, 链: {}, 时间: {:?}",
            params.base.request_resource_id,
            params.base.chain_code,
            start_time
        );

        tracing::info!("transfer: 获取私钥");
        let private_key_time = Instant::now();
        let private_key = match preloaded_private_key {
            Some(pk) => pk,
            None => {
                ApiAccountDomain::get_private_key(&params.base.from, &params.base.chain_code)
                    .await?
            }
        };
        tracing::info!("transfer: 获取私钥完成, 耗时: {:?}", private_key_time.elapsed());

        tracing::info!("transfer: 原始链代码: {}", params.base.chain_code);
        let chain_code_time = Instant::now();
        let chain_code = params.base.chain_code.as_str();
        tracing::info!(
            "transfer: 转换后链代码: {}, 耗时: {:?}",
            chain_code,
            chain_code_time.elapsed()
        );

        let adapter_time = Instant::now();
        let adapter = ApiChainAdapterFactory::get_transaction_adapter(chain_code).await?;
        tracing::info!("transfer (适配器创建): 完成, 耗时: {:?}", adapter_time.elapsed());

        tracing::info!("transfer: 执行转账");
        // TODO：可优化
        let transfer_time = Instant::now();
        tracing::info!(?params, "transfer: 构建原始交易");
        let resp = adapter.build_transfer_raw(&params, private_key).await?;
        tracing::info!("transfer: 转账操作完成, 耗时: {:?}", transfer_time.elapsed());
        tracing::info!(?resp, "transfer: 构建原始交易完成");

        Ok(resp)
    }

    /// ⚠️ 注意：
    /// 网络错误不等于广播失败。
    /// broadcast_transfer 在网络异常时返回 Ok(None)，
    /// 由 scanner / recover 决定最终状态。
    pub async fn broadcast_transfer(
        chain_code: &str,
        raw: RawTx,
    ) -> Result<Option<TransferResp>, ServiceError> {
        let start_time = Instant::now();
        tracing::info!("broadcast_transfer (开始): 链: {}, 时间: {:?}", chain_code, start_time);

        let adapter_time = Instant::now();
        let adapter = match ApiChainAdapterFactory::get_transaction_adapter(chain_code).await {
            Ok(adapter) => adapter,
            Err(e) => {
                if e.is_network_error() {
                    tracing::error!("broadcast_transfer: 网络错误, 适配器创建失败: {}", e);
                    return Ok(None);
                }
                return Err(e);
            }
        };
        tracing::info!("broadcast_transfer (适配器创建): 完成, 耗时: {:?}", adapter_time.elapsed());

        let resp = match adapter.broadcast_transfer(raw).await {
            Ok(resp) => resp,
            Err(e) => {
                if e.is_network_error() {
                    tracing::error!("broadcast_transfer: 网络错误, 交易广播失败: {}", e);
                    return Ok(None);
                }
                return Err(e);
            }
        };
        tracing::info!("broadcast_transfer: 转账操作完成, 耗时: {:?}", start_time.elapsed());

        Ok(Some(resp))
    }

    pub async fn nonce(from_addr: &str, chain_code: &str) -> Result<u64, ServiceError> {
        let start_time = Instant::now();
        tracing::info!(
            "nonce (开始): from_addr: {}, chain_code: {}, 时间: {:?}",
            from_addr,
            chain_code,
            start_time
        );

        let chain_code_time = Instant::now();
        tracing::info!("nonce (链代码转换): 完成, 耗时: {:?}", chain_code_time.elapsed());

        let adapter_time = Instant::now();
        let adapter = ApiChainAdapterFactory::get_transaction_adapter(chain_code).await?;
        tracing::info!("nonce (适配器创建): 完成, 耗时: {:?}", adapter_time.elapsed());

        let resp = adapter.nonce(from_addr).await;
        tracing::info!("nonce (结束): 总耗时: {:?}", start_time.elapsed());

        resp
    }

    /// 处理已生成raw_tx的交易恢复逻辑
    ///
    /// 职责：
    /// - 只负责从链上恢复交易状态
    /// - 不负责构建交易
    /// - 使用链上时间设置 transaction_time_ms
    ///
    /// 显式不变量：
    /// - transaction_time MUST come from on-chain confirmation (chain timestamp)
    /// - last_broadcast_at MUST be backfilled with the same value as transaction_time
    /// - This ensures both fields reflect the same chain-based timestamp
    ///
    /// 返回语义约定：
    /// Ok(Some(resp))  -> 链上已确认成功，立即落成
    /// Err(_)          -> 已确认不可能成功，可推进失败
    /// Ok(None)        -> 不确定态（RPC/网络/索引问题），等待 scanner/recover
    pub async fn process_recovered_tx(
        chain_code: &str,
        from_addr: &str,
        tx_hash: &str,
        // raw_tx: &str,
        nonce: i64,
        transaction_fee: &str,
    ) -> Result<Option<TransferResp>, ServiceError> {
        tracing::info!(trade_no=?tx_hash, "检测到已有raw_tx和tx_hash，执行恢复检查");

        let adapter = match ApiChainAdapterFactory::get_transaction_adapter(chain_code).await {
            Ok(adapter) => adapter,
            Err(e) => {
                if e.is_network_error() {
                    tracing::error!(trade_no=?tx_hash, "获取链适配器失败: {}", e);
                    return Ok(None);
                }
                return Err(e);
            }
        };

        // 1. 查链上是否存在
        match adapter.query_tx_res(tx_hash).await {
            // === A. 链上查到了 ===
            Ok(Some(tx_result)) => {
                // 修复状态类型问题：将i8转换为bool
                let is_success = match tx_result.status {
                    2 => true,
                    3 => false,
                    _ => {
                        tracing::warn!(trade_no=?tx_hash, "链上交易状态异常: {}", tx_result.status);
                        false
                    }
                };

                let time = tx_result.transaction_time;

                if is_success {
                    tracing::info!(trade_no=?tx_hash, "链上确认成功，直接落成");
                    // 直接标记成功
                    let mut mock_resp = TransferResp {
                        tx_hash: tx_hash.to_string(),
                        fee: transaction_fee.to_string(),
                        consumer: None,
                        transaction_time_ms: None,
                    };
                    // 使用链上时间设置 transaction_time_ms
                    mock_resp.with_transaction_time(time);
                    return Ok(Some(mock_resp));
                } else {
                    tracing::warn!(trade_no=?tx_hash, "链上失败，直接标记失败");
                    return Err(ServiceError::System(crate::error::system::SystemError::Internal(
                        "broadcasted tx failed".into(),
                    )));
                }
            }

            // === B. 链上没有该hash ===
            Ok(None) => {
                // None 表示交易结果尚不可确认（pending / rpc 未返回），
                // 不应在恢复流程中推进任何状态，
                // 交由下一轮 Scanner 重试即可。
                return Ok(None);
                // tracing::info!(trade_no=?tx_hash, "链上未找到该交易，准备判断是否需要重发");

                // // B1. 非EVM链 -> 直接重发
                // // 简化处理：基于链码判断是否为EVM链
                // let is_evm_chain = chain_code
                //     == wallet_types::chain::chain::ChainCode::Ethereum.to_string()
                //     || chain_code
                //         == wallet_types::chain::chain::ChainCode::BnbSmartChain.to_string();

                // if !is_evm_chain {
                //     tracing::info!(trade_no=?tx_hash, "非EVM链，未找到链上记录，尝试直接广播raw_tx");

                //     if raw_tx.is_empty() {
                //         // 没raw_tx就只能放弃重新构建
                //         tracing::error!(trade_no=?tx_hash, "非EVM链，raw_tx为空，无法重发");
                //         return Err(ServiceError::System(
                //             crate::error::system::SystemError::Internal("raw_tx is empty".into()),
                //         ));
                //     }

                //     // 不再广播，直接返回失败
                //     tracing::error!(trade_no=?tx_hash, "非EVM链不再广播raw_tx，直接标记为失败");
                //     return Err(ServiceError::System(crate::error::system::SystemError::Internal(
                //         "no longer broadcast raw_tx for non-evm chain".into(),
                //     )));
                // }

                // // B2. EVM 链 -> 判断 nonce
                // let chain_nonce = match Self::nonce(from_addr, chain_code).await {
                //     Ok(nonce) => nonce,
                //     Err(e) => {
                //         if e.is_network_error() {
                //             tracing::warn!(trade_no=?tx_hash, "网络错误，无法获取链上nonce，等待下一轮");
                //             return Ok(None);
                //         }
                //         tracing::error!(trade_no=?tx_hash, "获取链上nonce失败: {}", e);
                //         return Err(e);
                //     }
                // };
                // tracing::info!(trade_no=?tx_hash, "EVM链 nonce链上={}, 本地={}", chain_nonce, nonce);

                // // 1️⃣ 链上 nonce 更大 = 本地 tx 已经过期/被覆盖
                // if chain_nonce > nonce as u64 {
                //     tracing::warn!(trade_no=?tx_hash, "nonce已被占用但链上无此hash，判定丢失");
                //     return Err(ServiceError::System(crate::error::system::SystemError::Internal(
                //         "lost pending tx".into(),
                //     )));
                // }

                // // 2️⃣ 链上 nonce 相等 = raw_tx 未上链 → 应该重发 raw_tx
                // if chain_nonce == nonce as u64 {
                //     tracing::info!(trade_no=?tx_hash, "nonce一致，本地交易尚未上链，准备重发 raw_tx");

                //     if raw_tx.is_empty() {
                //         tracing::error!(trade_no=?tx_hash, "raw_tx为空，无法重发");
                //         return Err(ServiceError::System(
                //             crate::error::system::SystemError::Internal("raw_tx is empty".into()),
                //         ));
                //     }

                //     // 不再广播，直接返回失败
                //     tracing::error!(trade_no=?tx_hash, "EVM链不再广播raw_tx，直接标记为失败");
                //     return Err(ServiceError::System(crate::error::system::SystemError::Internal(
                //         "no longer broadcast raw_tx for evm chain".into(),
                //     )));
                // }

                // // 3️⃣ 链上 nonce 小于本地 = 理论上不该发生
                // // 表示你本地曾经构建过高nonce交易，现在轮到它老的还没处理
                // // 最安全：等待下一轮
                // tracing::error!(trade_no=?tx_hash, "本地nonce比链上大，可能出现nonce漂移，等待下一轮观察");
                // return Ok(None);
            }

            // === C. RPC 异常 ===
            Err(err) => {
                tracing::error!(trade_no=?tx_hash, "查询链上状态失败: {}", err);
                return Ok(None); // 容错，下轮再查
            }
        }
    }
}
