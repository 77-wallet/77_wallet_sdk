use crate::{
    domain::{
        bill::BillDomain,
        chain::{adapter::ChainAdapterFactory, transaction::ChainTransDomain},
    },
    request::transaction::ApproveParams,
    response_vo::swap::{SupportChain, SwapTokenInfo},
};
use wallet_database::entities::{bill::NewBillEntity, coin::CoinEntity};

pub struct SwapServer {
    // pub client: HttpClient,
}

impl SwapServer {
    pub fn new() -> Self {
        Self {}
    }
}

impl SwapServer {
    pub async fn quote(&self) -> Result<(), crate::ServiceError> {
        // 查询后端,获取报价
        // 模拟执行获得手续费
        Ok(())
    }

    pub async fn token_list(&self) -> Result<Vec<SwapTokenInfo>, crate::ServiceError> {
        // self.client.post()

        let data = SwapTokenInfo {
            name: String::from("usdt"),
            symbol: String::from("USDT"),
            decimals: 0,
            chain_code: String::from("tron"),
            contract_address: String::from("xxx"),
        };

        Ok(vec![data])
    }

    pub async fn chain_list(&self) -> Result<Vec<SupportChain>, crate::ServiceError> {
        let data = SupportChain {
            name: String::from("tron"),
            chain_code: String::from("tron"),
        };

        Ok(vec![data])
    }

    pub async fn approve(
        &self,
        req: ApproveParams,
        password: String,
    ) -> Result<String, crate::ServiceError> {
        // get coin
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let coin = CoinEntity::get_coin_by_chain_code_token_address(
            pool.as_ref(),
            &req.chain_code,
            &req.contract,
        )
        .await?
        .ok_or(crate::BusinessError::Coin(crate::CoinError::NotFound(
            format!(
                "coin not found: chain_code: {}, symbol: {}",
                req.chain_code, req.contract
            ),
        )))?;

        let private_key =
            ChainTransDomain::get_key(&req.from, &req.chain_code, &password, &None).await?;
        let adapter = ChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;

        //
        let hash = adapter.approve(&req, &coin, private_key).await?;

        let mut new_bill = NewBillEntity::from(req);
        new_bill.hash = hash.clone();
        new_bill.symbol = coin.symbol.clone();

        BillDomain::create_bill(new_bill).await?;
        Ok(hash)
    }

    pub async fn swap(&self) {}
}
