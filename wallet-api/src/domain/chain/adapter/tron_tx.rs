use crate::{domain::multisig::MultisigQueueDomain, response_vo::TransferParams};
use wallet_chain_interact::{
    tron::{
        operations::{
            transfer::{ContractTransferOpt, TransferOpt},
            TronConstantOperation as _,
        },
        TronChain,
    },
    types::MultisigTxResp,
};

// 构建多签交易
pub(super) async fn build_build_tx(
    req: &TransferParams,
    token: Option<String>,
    value: alloy::primitives::U256,
    chain: &TronChain,
    threshold: i64,
    permission_id: Option<i64>,
) -> Result<MultisigTxResp, crate::ServiceError> {
    let expiration = MultisigQueueDomain::sub_expiration(req.expiration.unwrap_or(1));

    if let Some(token) = token {
        let mut params =
            ContractTransferOpt::new(&token, &req.from, &req.to, value, req.notes.clone())?;

        params.permission_id = permission_id;

        let provider = chain.get_provider();
        let constant = params.constant_contract(provider).await?;
        let consumer = provider
            .contract_fee(constant, threshold as u8, &req.from)
            .await?;
        params.set_fee_limit(consumer);

        Ok(chain
            .build_multisig_transaction(params, expiration as u64)
            .await?)
    } else {
        let mut params = TransferOpt::new(&req.from, &req.to, value, req.notes.clone())?;
        params.permission_id = permission_id;

        Ok(chain
            .build_multisig_transaction(params, expiration as u64)
            .await?)
    }
}
