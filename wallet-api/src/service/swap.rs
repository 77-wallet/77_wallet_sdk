use crate::{
    domain::{
        bill::BillDomain,
        chain::{adapter::ChainAdapterFactory, transaction::ChainTransDomain},
        coin::{CoinDomain, TokenCurrencyGetter},
        task_queue::TaskQueueDomain,
    },
    infrastructure::swap_client::{
        AggQuoteRequest, AggQuoteResp, ChainDex, DefaultQuoteResp, SwapClient,
    },
    messaging::notify::other::{Process, TransactionProcessFrontend},
    request::transaction::{ApproveReq, QuoteReq, SwapReq, SwapTokenListReq},
    response_vo::{
        account::BalanceInfo,
        swap::{ApiQuoteResp, ApproveList, SwapTokenInfo},
        CommonFeeDetails,
    },
    FrontendNotifyEvent, NotifyEvent,
};
use alloy::primitives::U256;
use wallet_database::{
    entities::{
        assets::{AssetsEntity, AssetsId},
        bill::NewBillEntity,
    },
    pagination::Pagination,
    repositories::{account::AccountRepo, coin::CoinRepo, exchange_rate::ExchangeRateRepo},
};
use wallet_transport_backend::{
    api::swap::{ApproveCancelReq, ApproveSaveParams},
    consts::endpoint::{SWAP_APPROVE_CANCEL, SWAP_APPROVE_SAVE},
    request::SwapTokenQueryReq,
};
use wallet_types::chain::chain::ChainCode;
use wallet_utils::{address::AccountIndexMap, unit};

pub struct SwapServer {
    pub client: SwapClient,
}

impl SwapServer {
    pub fn new() -> Result<Self, crate::ServiceError> {
        let url = crate::manager::Context::get_aggregate_api()?;
        let swap_client = SwapClient::new(&url);

        Ok(Self {
            client: swap_client?,
        })
    }
}

impl SwapServer {
    pub async fn default_quote(
        &self,
        chain_code: String,
        token_in: String,
    ) -> Result<DefaultQuoteResp, crate::ServiceError> {
        let code = ChainCode::try_from(chain_code.as_str())?;
        let token_addr = CoinDomain::get_stable_coin(code)?;

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let stable_coin = CoinRepo::coin_by_chain_address(&chain_code, &token_addr, &pool).await?;

        let (from_token, out_token) = if token_in.is_empty() {
            let token = CoinRepo::main_coin(&chain_code, &pool).await?;
            (token, stable_coin)
        } else if token_in == token_addr {
            // 传入的是稳定币
            let token = CoinRepo::main_coin(&chain_code, &pool).await?;

            (stable_coin, token)
        } else {
            let token = CoinRepo::coin_by_chain_address(&chain_code, &token_in, &pool).await?;

            (token, stable_coin)
        };

        //   pub symbol: String,
        //     pub decimals: u32,
        //     pub chain_code: String,
        //     pub name: String,
        //     pub token_addr: String,
        //     pub balance: BalanceInfo,

        let res = DefaultQuoteResp {
            token_in: SwapTokenInfo {
                token_addr: from_token.token_address().unwrap_or_default(),
                decimals: from_token.decimals as u32,
                symbol: from_token.symbol,
                chain_code: from_token.chain_code.to_string(),
                name: from_token.name,
                balance: BalanceInfo::default(),
            },
            token_out: SwapTokenInfo {
                token_addr: out_token.token_address().unwrap_or_default(),
                decimals: out_token.decimals as u32,
                symbol: out_token.symbol,
                chain_code: out_token.chain_code.to_string(),
                name: out_token.name,
                balance: BalanceInfo::default(),
            },
        };

        Ok(res)
    }

    pub async fn quote(&self, req: QuoteReq) -> Result<ApiQuoteResp, crate::ServiceError> {
        use wallet_utils::unit::{convert_to_u256, format_to_f64, format_to_string, string_to_f64};
        // 查询后端,获取报价(调用合约查路径)
        let params = AggQuoteRequest::try_from(&req)?;
        let quote_resp = self.client.get_quote(params).await?;

        // tracing::warn!("quote = {:#?}", quote_resp);

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

        // 获取滑点
        let slippage = req.get_slippage(quote_resp.default_slippage);

        // 构建响应
        let mut res = ApiQuoteResp::new(
            quote_resp.chain_code.clone(),
            slippage,
            quote_resp.dex_route_list.clone(),
            bal_in,
            bal_out,
        );
        // 先使用报价返回的amount_out,如果可以进行模拟，那么后续使用模拟的值覆盖
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
            res.need_approve_amount = format_to_string(diff, req.token_in.decimals as u8)?;
            res.approve_amount = format_to_string(allowance, req.token_in.decimals as u8)?;
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
        // 模拟报价
        let result = adapter.swap_quote(req, quote_resp).await?;

        // 手续费
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let main_coin = CoinRepo::main_coin(&req.chain_code, &pool).await?;
        let amount = wallet_utils::unit::format_to_f64(result.fee, main_coin.decimals)?;
        res.fee = TokenCurrencyGetter::get_balance_info(&req.chain_code, &main_coin.symbol, amount)
            .await?;
        res.consumer = result.consumer;

        // 重新覆盖amount_out,使用模拟的值
        res.set_amount_out(result.amount_out, req.token_out.decimals);

        Ok(())
    }

    async fn check_allowance(&self, req: &QuoteReq) -> Result<U256, crate::ServiceError> {
        let adapter = ChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;
        adapter
            .allowance(
                &req.recipient,
                &req.token_in.token_addr,
                &req.aggregator_addr,
            )
            .await
    }

    // 执行兑换
    pub async fn swap(
        &self,
        req: SwapReq,
        fee: String,
        password: String,
    ) -> Result<String, crate::ServiceError> {
        // 构建事件
        let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new(
            wallet_database::entities::bill::BillKind::Swap,
            Process::Building,
        ));
        FrontendNotifyEvent::new(data).send().await?;

        let key =
            ChainTransDomain::get_key(&req.recipient, &req.chain_code, &password, &None).await?;

        // 广播事件
        let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new(
            wallet_database::entities::bill::BillKind::Swap,
            Process::Broadcast,
        ));
        FrontendNotifyEvent::new(data).send().await?;
        let adapter = ChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;

        // 执行swap 交易
        let hash = adapter.swap(&req, fee, key).await?;

        // 写入本地交易记录表
        let mut new_bill = NewBillEntity::try_from(req)?;
        new_bill.hash = hash.clone();
        BillDomain::create_bill(new_bill).await?;

        Ok(hash)
    }

    pub async fn token_list(
        &self,
        req: SwapTokenListReq,
    ) -> Result<Pagination<SwapTokenInfo>, crate::ServiceError> {
        let backend = crate::manager::Context::get_global_backend_api()?;
        let req = SwapTokenQueryReq::from(req);

        let result = backend.swap_token_list(req).await?;

        let mut resp = Pagination::<SwapTokenInfo> {
            page: result.page_index,
            page_size: result.page_size,
            total_count: result.total_count,
            data: vec![],
        };

        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let currency = {
            let state = crate::app_state::APP_STATE.read().await;
            state.currency().to_string() // 或复制 enum 值，取决于类型
        };
        for item in result.list.into_iter() {
            // 查询资产
            let assets_id = AssetsId {
                address: item.token_address.clone().unwrap_or_default(),
                symbol: item.aname.clone().unwrap_or_default(),
                chain_code: item.chain_code.clone(),
            };

            let assets = AssetsEntity::assets_by_id(pool.as_ref(), &assets_id).await?;
            let balance = if let Some(assets) = assets {
                let unit_price = if currency.eq_ignore_ascii_case("usdt") {
                    item.price
                } else {
                    let pool = crate::manager::Context::get_global_sqlite_pool()?;
                    let exchange = ExchangeRateRepo::exchange_rate(&currency, &pool).await?;

                    exchange.rate * item.price
                };

                let amount = wallet_utils::unit::string_to_f64(&assets.balance)?;
                BalanceInfo::new(amount, Some(unit_price), &currency)
            } else {
                BalanceInfo::default()
            };

            // 构建响应
            let resp_item = SwapTokenInfo {
                token_addr: item.token_address.unwrap_or_default(),
                symbol: item.aname.unwrap_or_default(),
                decimals: item.unit.unwrap_or_default() as u32,
                balance,
                chain_code: item.chain_code,
                name: item.name.unwrap_or_default(),
            };
            resp.data.push(resp_item);
        }

        // Ok(self.client.token_list(req).await?)

        Ok(resp)
    }

    pub async fn chain_list(&self) -> Result<Vec<ChainDex>, crate::ServiceError> {
        Ok(self.client.chain_list().await?.chain_dexs)
    }

    // pub async fn dex_list(
    //     &self,
    //     chain_code: String,
    // ) -> Result<Vec<SupportDex>, crate::ServiceError> {
    //     Ok(self.client.dex_list(&chain_code).await?)
    // }

    pub async fn approve_fee(
        &self,
        req: ApproveReq,
    ) -> Result<CommonFeeDetails, crate::ServiceError> {
        let adapter = ChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;

        let value = alloy::primitives::U256::MAX;
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let main_coin = CoinRepo::main_coin(&req.chain_code, &pool).await?;

        let fee = adapter.approve_fee(&req, value, &main_coin.symbol).await?;

        // let fee_resp = EstimateFeeResp::new(main_coin.symbol, main_coin.chain_code.clone(), fee);
        Ok(fee)
    }

    pub async fn approve(
        &self,
        req: ApproveReq,
        password: String,
    ) -> Result<String, crate::ServiceError> {
        // get coin
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let coin = CoinRepo::coin_by_chain_address(&req.chain_code, &req.contract, &pool).await?;

        // 构建交易事件
        let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new(
            wallet_database::entities::bill::BillKind::Approve,
            Process::Building,
        ));
        FrontendNotifyEvent::new(data).send().await?;

        let adapter = ChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;
        // check already approved
        let allowance = adapter
            .allowance(&req.from, &req.contract, &req.spender)
            .await?;
        if allowance >= alloy::primitives::U256::MAX {
            return Err(crate::BusinessError::Chain(
                crate::ChainError::ApproveRepeated,
            ))?;
        }

        let private_key =
            ChainTransDomain::get_key(&req.from, &req.chain_code, &password, &None).await?;

        // 广播交易事件
        let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new(
            wallet_database::entities::bill::BillKind::Approve,
            Process::Broadcast,
        ));
        FrontendNotifyEvent::new(data).send().await?;

        let value = if req.approve_type == ApproveReq::UN_LIMIT {
            alloy::primitives::U256::MAX
        } else {
            wallet_utils::unit::convert_to_u256(&req.value, coin.decimals)?
        };
        let hash = adapter.approve(&req, private_key, value).await?;

        let account = AccountRepo::account_with_wallet(&req.from, &req.chain_code, &pool).await?;

        // 上报后端
        let backend_req = ApproveSaveParams::new(
            account.get_index()?,
            &account.uid,
            &req.chain_code,
            &req.spender,
            &req.from,
            &req.contract,
            req.value.clone(),
            &hash,
            &req.approve_type,
        );
        TaskQueueDomain::send_or_to_queue(backend_req, SWAP_APPROVE_SAVE).await?;

        // 写入本地交易
        let mut new_bill = NewBillEntity::from(req);
        new_bill.hash = hash.clone();
        new_bill.symbol = coin.symbol;
        BillDomain::create_bill(new_bill).await?;

        Ok(hash)
    }

    pub async fn approve_list(
        &self,
        uid: String,
        account_id: u32,
    ) -> Result<Vec<ApproveList>, crate::ServiceError> {
        let index_map = AccountIndexMap::from_account_id(account_id)?;

        let backend = crate::manager::Context::get_global_backend_api()?;

        let resp = backend.approve_list(uid, index_map.input_index).await?;

        let mut res = vec![];

        let mut used_ids = vec![];
        for item in resp.list.into_iter() {
            if item.limit_type == ApproveReq::UN_LIMIT {
                res.push(ApproveList::from(item))
            } else {
                // 获取allowance 情况
                let adapter =
                    ChainAdapterFactory::get_transaction_adapter(&item.chain_code).await?;

                let allowance = adapter
                    .allowance(&item.owner_address, &item.token_addr, &item.spender)
                    .await?;
                if allowance == alloy::primitives::U256::ZERO {
                    used_ids.push(item.id);
                } else {
                    let pool = crate::manager::Context::get_global_sqlite_pool()?;
                    let a =
                        CoinRepo::coin_by_chain_address(&item.chain_code, &item.token_addr, &pool)
                            .await?;
                    let mut c = ApproveList::from(item);
                    c.amount = wallet_utils::unit::format_to_string(allowance, a.decimals as u8)?;
                }
            }
        }

        // 通知后端哪些已经被使用
        if !used_ids.is_empty() {
            backend.update_used_approve(used_ids).await?;
        }

        Ok(res)
    }

    pub async fn approve_cancel(
        &self,
        req: ApproveReq,
        password: String,
    ) -> Result<String, crate::ServiceError> {
        let private_key =
            ChainTransDomain::get_key(&req.from, &req.chain_code, &password, &None).await?;
        let adapter = ChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let coin = CoinRepo::coin_by_chain_address(&req.chain_code, &req.contract, &pool).await?;

        let value = alloy::primitives::U256::ZERO;
        let hash = adapter.approve(&req, private_key, value).await?;

        let backend = ApproveCancelReq {
            spender: req.spender.clone(),
            token_addr: req.contract.clone(),
            owner_address: req.from.clone(),
        };
        TaskQueueDomain::send_or_to_queue(backend, SWAP_APPROVE_CANCEL).await?;

        // 写入本地交易
        let mut new_bill = NewBillEntity::from(req);
        new_bill.hash = hash.clone();
        new_bill.symbol = coin.symbol;
        BillDomain::create_bill(new_bill).await?;

        Ok(hash)
    }

    // pub async fn supplier(
    //     &self,
    //     chain_code: String,
    // ) -> Result<serde_json::Value, crate::ServiceError> {
    //     Ok(self.client.swap_contract(chain_code).await?)
    // }

    // pub async fn allowance(
    //     &self,
    //     from: String,
    //     token: String,
    //     chain_code: String,
    //     spender: String,
    // ) -> Result<String, crate::ServiceError> {
    //     let adapter = ChainAdapterFactory::get_transaction_adapter(&chain_code).await?;

    //     let result = adapter.allowance(&from, &token, &spender).await?;

    //     Ok(result.to_string())
    // }
}
