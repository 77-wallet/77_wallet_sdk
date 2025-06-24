use crate::request::transaction::ApproveParams;
use wallet_chain_interact::{
    eth::{operations::erc::Approve, EthChain, FeeSetting},
    types::ChainPrivateKey,
};

pub(super) async fn approve(
    chain: &EthChain,
    req: &ApproveParams,
    value: alloy::primitives::U256,
    key: ChainPrivateKey,
) -> Result<String, crate::ServiceError> {
    let approve = Approve::new(&req.from, &req.spender, value, &req.contract)?;

    // 使用默认的手续费配置
    let gas_price = chain.provider.gas_price().await?;
    let fee_setting = FeeSetting::new_with_price(gas_price);

    // exec tx
    let hash = chain.exec_transaction(approve, fee_setting, key).await?;

    Ok(hash)
}
