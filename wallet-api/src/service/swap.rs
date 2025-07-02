use crate::{
    domain::{
        bill::BillDomain,
        chain::{
            adapter::ChainAdapterFactory, swap::evm_swap::SwapParams, transaction::ChainTransDomain,
        },
        coin::TokenCurrencyGetter,
        swap_client::{AggQuoteRequest, AggQuoteResp, SupportChain, SupportDex, SwapClient},
    },
    request::transaction::{ApproveParams, QuoteReq, SwapReq, SwapTokenListReq},
    response_vo::swap::ApiQuoteResp,
};
use alloy::primitives::U256;
use wallet_database::{entities::bill::NewBillEntity, repositories::coin::CoinRepo};
use wallet_utils::unit;

pub struct SwapServer {
    pub client: SwapClient,
}

impl SwapServer {
    pub fn new() -> Result<Self, crate::ServiceError> {
        let url = String::from("http://127.0.0.1:28888/api");
        let swap_client = SwapClient::new(&url);

        Ok(Self {
            client: swap_client?,
        })
    }
}

impl SwapServer {
    pub async fn quote(&self, req: QuoteReq) -> Result<ApiQuoteResp, crate::ServiceError> {
        use wallet_utils::unit::{convert_to_u256, format_to_f64, format_to_string, string_to_f64};
        // 查询后端,获取报价(调用合约查路径)
        let params = AggQuoteRequest::try_from(&req)?;
        let quote_resp = self.client.get_quote(params).await?;

        let amount_out = unit::u256_from_str(&quote_resp.amount_out)?;

        let bal_in = TokenCurrencyGetter::get_bal_by_backend(
            &req.chain_code,
            &req.token_in.token_addr,
            string_to_f64(&req.amount_in)?,
        )
        .await?;

        let bal_out = TokenCurrencyGetter::get_bal_by_backend(
            &req.chain_code,
            &req.token_out.token_addr,
            format_to_f64(amount_out, req.token_out.decimals as u8)?,
        )
        .await?;

        // TODO
        let mut res = ApiQuoteResp::new(
            "".to_string(),
            req.slippage,
            quote_resp.dex_route_list.clone(),
            bal_in,
            bal_out,
        );
        res.set_amount_out(amount_out, req.token_out.decimals);

        // 主币处理
        if req.token_in.token_addr.is_empty() {
            self.simulate_and_fill(&req, &quote_resp, &mut res).await?;
            return Ok(res);
        }

        // 代币处理
        let allowance = self.check_allowance(&req).await?;
        let amount_in = convert_to_u256(&req.amount_in, req.token_in.decimals as u8)?;

        if allowance >= amount_in {
            self.simulate_and_fill(&req, &quote_resp, &mut res).await?;
            return Ok(res);
        } else {
            let diff = amount_in - allowance;
            res.approve_amount = format_to_string(diff, req.token_in.decimals as u8)?;
            return Ok(res);
        }
    }

    // 模拟交易以及填充响应
    async fn simulate_and_fill(
        &self,
        req: &QuoteReq,
        quote_resp: &AggQuoteResp,
        res: &mut ApiQuoteResp,
    ) -> Result<(), crate::ServiceError> {
        let adapter = ChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;
        let result = adapter.swap_quote(req, quote_resp).await?;

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let main_coin = CoinRepo::main_coin(&req.chain_code, &pool).await?;

        let amount = wallet_utils::unit::format_to_f64(result.fee, main_coin.decimals)?;

        res.fee = TokenCurrencyGetter::get_balance_info(&req.chain_code, &main_coin.symbol, amount)
            .await?;
        res.consumer = result.consumer;
        res.set_amount_out(result.amount_out, req.token_out.decimals);

        Ok(())
    }

    async fn check_allowance(&self, req: &QuoteReq) -> Result<U256, crate::ServiceError> {
        let adapter = ChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;
        adapter
            .allowance(&req.recipient, &req.token_in.token_addr)
            .await
    }

    // 执行兑换
    pub async fn swap(
        &self,
        req: SwapReq,
        fee: String,
        password: String,
    ) -> Result<String, crate::ServiceError> {
        let key =
            ChainTransDomain::get_key(&req.recipient, &req.chain_code, &password, &None).await?;

        let adapter = ChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;

        let swap_params = SwapParams::try_from(&req)?;

        // 执行swap 交易
        let hash = adapter.swap(swap_params, &req.recipient, fee, key).await?;

        // 写入本地交易记录表
        let mut new_bill = NewBillEntity::from(req);
        new_bill.hash = hash.clone();
        new_bill.symbol = "WETH".to_string();

        Ok(hash)
    }

    pub async fn token_list(
        &self,
        req: SwapTokenListReq,
    ) -> Result<serde_json::Value, crate::ServiceError> {
        let res = self.client.token_list(req).await?;

        Ok(res)
    }

    pub async fn chain_list(&self) -> Result<Vec<SupportChain>, crate::ServiceError> {
        Ok(self.client.chain_list().await?)
    }

    pub async fn dex_list(&self, chain_id: i64) -> Result<Vec<SupportDex>, crate::ServiceError> {
        Ok(self.client.dex_list(chain_id).await?)
    }

    pub async fn approve(
        &self,
        req: ApproveParams,
        password: String,
    ) -> Result<String, crate::ServiceError> {
        // // get coin
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let coin = CoinRepo::coin_by_chain_address(&req.chain_code, &req.contract, &pool).await?;

        let private_key =
            ChainTransDomain::get_key(&req.from, &req.chain_code, &password, &None).await?;
        let adapter = ChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;

        let hash = adapter.approve(&req, coin.decimals, private_key).await?;

        let mut new_bill = NewBillEntity::from(req);
        new_bill.hash = hash.clone();
        new_bill.symbol = coin.symbol;

        BillDomain::create_bill(new_bill).await?;
        Ok(hash)
    }

    pub async fn allowance(
        &self,
        from: String,
        token: String,
        chain_code: String,
    ) -> Result<String, crate::ServiceError> {
        let adapter = ChainAdapterFactory::get_transaction_adapter(&chain_code).await?;

        let result = adapter.allowance(&from, &token).await?;

        Ok(result.to_string())
    }
}
