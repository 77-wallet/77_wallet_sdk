use crate::{
    api::ReturnType, manager::WalletManager, service::api_wallet::fee::TransferFeeService,
};
use wallet_database::entities::api_fee::ApiFeeEntity;

impl WalletManager {
    pub async fn get_api_transfer_fee_order_list(&self) -> ReturnType<Vec<ApiFeeEntity>> {
        TransferFeeService::new().get_transfer_fee_order_list().await
    }

    // 测试
    pub async fn api_transfer_fee_order(
        &self,
        from: &str,
        to: &str,
        value: &str,
        chain_code: &str,
        token_address: Option<String>,
        symbol: &str,
        trade_no: &str,
        trade_type: u8,
        uid: &str,
    ) -> ReturnType<()> {
        TransferFeeService::new()
            .transfer_fee_order(
                from,
                to,
                value,
                chain_code,
                token_address,
                symbol,
                trade_no,
                trade_type,
                uid,
            )
            .await
    }
}

#[cfg(test)]
mod test {

    // #[tokio::test]
    // async fn test_create_api_account() -> Result<()> {
    //     wallet_utils::init_test_log();
    //     // 修改返回类型为Result<(), anyhow::Error>
    //     let (wallet_manager, _test_params) = get_manager().await?;

    //     let wallet_address = "0x6F0e4B9F7dD608A949138bCE4A29e076025b767F";
    //     let wallet_password = "q1111111";
    //     let index = None;
    //     let name = "666";
    //     let is_default_name = true;
    //     let api_wallet_type = ApiWalletType::SubAccount;

    //     let req = CreateApiAccountReq::new(
    //         wallet_address,
    //         wallet_password,
    //         index,
    //         name,
    //         is_default_name,
    //         api_wallet_type,
    //     );
    //     let res = wallet_manager.create_api_account(req).await;
    //     tracing::info!("res: {res:?}");
    //     Ok(())
    // }
}
