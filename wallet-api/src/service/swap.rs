use crate::{
    domain::{
        bill::BillDomain,
        chain::{
            adapter::ChainAdapterFactory, swap::evm_swap::SwapParams, transaction::ChainTransDomain,
        },
        coin::TokenCurrencyGetter,
        swap_client::{QuoteRequest, SwapClient},
    },
    request::transaction::{ApproveParams, DepositParams, QuoteReq, SwapReq},
    response_vo::{
        account::BalanceInfo,
        swap::{ApiQuoteResp, SupportChain, SwapTokenInfo},
    },
};
use wallet_database::entities::bill::NewBillEntity;

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
        // 查询后端,获取报价(调用合约查路径)
        let params = QuoteRequest::from(&req);

        let quote_resp = self.client.get_quote(params).await?;

        let adapter = ChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;

        // swap 参数
        let swap_params = SwapParams {
            recipient: wallet_utils::address::parse_eth_address(&req.from)?,
            token_in: wallet_utils::address::parse_eth_address(&req.token_in)?,
            token_out: wallet_utils::address::parse_eth_address(&req.token_out)?,
            dex_router: quote_resp.dex_route_list.clone(),
            allow_partial_fill: false,
        };

        // 模拟执行获得手续费
        let result = adapter.swap_quote(swap_params, &req.from).await?;

        let fee =
            TokenCurrencyGetter::get_balance_info(&req.chain_code, &req.from_symbol, result.fee)
                .await?;

        let res = ApiQuoteResp {
            supplier: "77_DexAggreagre".to_string(),
            // 预估的手续费
            fee,
            // 转换后的值
            from_token_price: "price".to_string(),
            // 滑点
            slippage: req.slippage,
            // 最小获得数量
            min_amount: result.amount_out.to_string(),
            // 兑换路径
            dex_route_list: quote_resp.dex_route_list,

            liquidity: "".to_string(),
        };

        Ok(res)
    }

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

    pub async fn token_list(&self) -> Result<Vec<SwapTokenInfo>, crate::ServiceError> {
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
        // // get coin
        // let pool = crate::manager::Context::get_global_sqlite_pool()?;

        // let coin = CoinEntity::get_coin_by_chain_code_token_address(
        //     pool.as_ref(),
        //     &req.chain_code,
        //     &req.contract,
        // )
        // .await?
        // .ok_or(crate::BusinessError::Coin(crate::CoinError::NotFound(
        //     format!(
        //         "coin not found: chain_code: {}, symbol: {}",
        //         req.chain_code, req.contract
        //     ),
        // )))?;

        let private_key =
            ChainTransDomain::get_key(&req.from, &req.chain_code, &password, &None).await?;
        let adapter = ChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;

        //
        let hash = adapter.approve(&req, 18, private_key).await?;

        let mut new_bill = NewBillEntity::from(req);
        new_bill.hash = hash.clone();
        new_bill.symbol = "WETH".to_string();

        BillDomain::create_bill(new_bill).await?;
        Ok(hash)
    }

    pub async fn deposit(
        &self,
        req: DepositParams,
        password: String,
    ) -> Result<String, crate::ServiceError> {
        // get coin
        // let pool = crate::manager::Context::get_global_sqlite_pool()?;

        // let coin = CoinEntity::get_coin_by_chain_code_token_address(
        //     pool.as_ref(),
        //     &req.chain_code,
        //     &req.contract,
        // )
        // .await?
        // .ok_or(crate::BusinessError::Coin(crate::CoinError::NotFound(
        //     format!(
        //         "coin not found: chain_code: {}, symbol: {}",
        //         req.chain_code, req.contract
        //     ),
        // )))?;

        let private_key =
            ChainTransDomain::get_key(&req.from, &req.chain_code, &password, &None).await?;
        let adapter = ChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;

        //
        let hash = adapter.deposit(&req, 18, private_key).await?;

        Ok(hash)
    }
}
