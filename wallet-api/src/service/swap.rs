use crate::{
    domain::{
        assets::AssetsDomain,
        bill::BillDomain,
        chain::{adapter::ChainAdapterFactory, swap::SLIPPAGE, transaction::ChainTransDomain},
        coin::{CoinDomain, TokenCurrencyGetter},
        task_queue::TaskQueueDomain,
    },
    infrastructure::swap_client::{
        AggQuoteRequest, AggQuoteResp, ChainDex, DefaultQuoteResp, SwapClient,
    },
    messaging::notify::other::{Process, TransactionProcessFrontend},
    request::transaction::{
        ApproveReq, DepositReq, DexRoute, QuoteReq, SwapInnerType, SwapReq, SwapTokenListReq,
        WithdrawReq,
    },
    response_vo::{
        account::{BalanceInfo, BalanceStr},
        swap::{ApiQuoteResp, ApproveList, SwapTokenInfo},
        EstimateFeeResp,
    },
    FrontendNotifyEvent, NotifyEvent,
};
use alloy::primitives::U256;
use std::time::{self};
use wallet_database::{
    entities::{account::AccountEntity, assets::AssetsEntity, bill::NewBillEntity},
    pagination::Pagination,
    repositories::{
        account::AccountRepo, assets::AssetsRepo, coin::CoinRepo, exchange_rate::ExchangeRateRepo,
    },
    DbPool,
};
use wallet_transport_backend::{
    api::swap::{ApproveCancelReq, ApproveSaveParams},
    consts::endpoint::{SWAP_APPROVE_CANCEL, SWAP_APPROVE_SAVE},
};
use wallet_types::chain::chain::ChainCode;
use wallet_utils::{
    address::AccountIndexMap,
    conversion,
    unit::{self, format_to_string},
};

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

    async fn get_bal_in_and_out(
        &self,
        req: &QuoteReq,
        amount_out: U256,
    ) -> Result<(BalanceStr, BalanceStr), crate::ServiceError> {
        // 查询两次后端
        let bal_in = TokenCurrencyGetter::get_bal_by_backend(
            &req.chain_code,
            &req.token_in.token_addr,
            &req.amount_in,
            req.token_in.decimals as u8,
        )
        .await?;

        let bal_out = TokenCurrencyGetter::get_bal_by_backend(
            &req.chain_code,
            &req.token_out.token_addr,
            &format_to_string(amount_out, req.token_out.decimals as u8)?,
            req.token_out.decimals as u8,
        )
        .await?;

        Ok((bal_in, bal_out))
    }

    pub async fn quote(&self, req: QuoteReq) -> Result<ApiQuoteResp, crate::ServiceError> {
        let chain_code = ChainCode::try_from(req.chain_code.as_str())?;

        let swap_inner_type = QuoteReq::swap_type(
            chain_code,
            &req.token_in.token_addr,
            &req.token_out.token_addr,
        )?;

        match swap_inner_type {
            SwapInnerType::Withdraw => self.common_quote(req, SwapInnerType::Withdraw).await,
            SwapInnerType::Deposit => self.common_quote(req, SwapInnerType::Deposit).await,
            SwapInnerType::Swap => self.swap_quote(req).await,
        }
    }

    fn check_bal(&self, val: &str, bal: &str) -> Result<bool, crate::ServiceError> {
        Ok(conversion::decimal_from_str(val)? <= conversion::decimal_from_str(bal)?)
    }

    async fn token0_assets(
        &self,
        pool: &DbPool,
        chain_code: &str,
        token_addr: &str,
        recipient: &str,
    ) -> Result<AssetsEntity, crate::ServiceError> {
        Ok(
            AssetsRepo::get_by_addr_token_opt(&pool, chain_code, token_addr, recipient)
                .await?
                .ok_or(crate::BusinessError::Assets(
                    crate::AssetsError::NotFoundAssets,
                ))?,
        )
    }

    async fn common_quote(
        &self,
        req: QuoteReq,
        tx_type: SwapInnerType,
    ) -> Result<ApiQuoteResp, crate::ServiceError> {
        let amount_out =
            wallet_utils::unit::convert_to_u256(&req.amount_in, req.token_in.decimals as u8)?;

        let (bal_in, bal_out) = self.get_bal_in_and_out(&req, amount_out).await?;

        let dex_route_list = DexRoute::new(req.amount_in.clone(), &req.token_in, &req.token_out);
        let mut res = ApiQuoteResp::new_with_default_slippage(
            req.chain_code.clone(),
            req.token_in.symbol.clone(),
            req.token_out.symbol.clone(),
            vec![dex_route_list],
            bal_in,
            bal_out,
        );
        res.set_amount_out(amount_out, req.token_out.decimals);

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let assets = self
            .token0_assets(
                &pool,
                &req.chain_code,
                &req.token_in.token_addr,
                &req.recipient,
            )
            .await?;

        if self.check_bal(&req.amount_in, &assets.balance)? {
            let adapter = ChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;
            let main_coin = CoinRepo::main_coin(&req.chain_code, &pool).await?;

            let (consumer, content) = match tx_type {
                SwapInnerType::Withdraw => {
                    let params = WithdrawReq {
                        from: req.recipient.clone(),
                        token: req.token_in.token_addr.clone(),
                        amount: req.amount_in.clone(),
                        chain_code: req.chain_code.clone(),
                    };
                    adapter.withdraw_fee(params, &main_coin).await?
                }
                SwapInnerType::Deposit => {
                    let params = DepositReq {
                        from: req.recipient.clone(),
                        token: req.token_out.token_addr.clone(),
                        amount: req.amount_in.clone(),
                        chain_code: req.chain_code.clone(),
                    };
                    adapter.deposit_fee(params, &main_coin).await?
                }
                SwapInnerType::Swap => {
                    return Err(crate::ServiceError::Parameter(
                        "不支持 Swap 类型用于 common_quote".into(),
                    ));
                }
            };

            let fee_resp = EstimateFeeResp::new(main_coin.symbol, main_coin.chain_code, content);
            res.consumer = consumer;
            res.fee = fee_resp;
        }

        Ok(res)
    }

    async fn swap_quote(&self, req: QuoteReq) -> Result<ApiQuoteResp, crate::ServiceError> {
        use wallet_utils::unit::{convert_to_u256, format_to_string};
        // 查询后端,获取报价(调用合约查路径)
        let params = AggQuoteRequest::try_from(&req)?;

        let instance = time::Instant::now();
        let quote_resp = self.client.get_quote(params).await?;
        tracing::warn!("quote time: {}", instance.elapsed().as_secs_f64());

        let amount_out = unit::u256_from_str(&quote_resp.amount_out)?;

        let (bal_in, bal_out) = self.get_bal_in_and_out(&req, amount_out).await?;

        // 获取滑点
        let slippage = req.get_slippage(quote_resp.default_slippage);
        let default_slippage = quote_resp.default_slippage as f64 / SLIPPAGE;

        // 构建响应
        let mut res = ApiQuoteResp::new(
            quote_resp.chain_code.clone(),
            req.token_in.symbol.clone(),
            req.token_out.symbol.clone(),
            slippage,
            default_slippage,
            quote_resp.dex_route_list.clone(),
            bal_in,
            bal_out,
        );
        // 先使用报价返回的amount_out,如果可以进行模拟，那么后续使用模拟的值覆盖
        res.set_amount_out(amount_out, req.token_out.decimals);
        res.set_dex_amount_out()?;

        // 主币处理
        if req.token_in.token_addr.is_empty() {
            let pool = crate::manager::Context::get_global_sqlite_pool()?;
            let assets =
                AssetsRepo::get_by_addr_token(&pool, &req.chain_code, "", &req.recipient).await?;
            if self.check_bal(&req.amount_in, &assets.balance)? {
                self.simulate_and_fill(&req, &quote_resp, &mut res).await?;
            }

            return Ok(res);
        }

        // 代币处理
        let allowance = self.check_allowance(&req).await?;
        let amount_in = convert_to_u256(&req.amount_in, req.token_in.decimals as u8)?;

        if allowance > U256::from(alloy::primitives::U64::MAX) {
            res.approve_amount = "-1".to_string();
        } else {
            res.approve_amount = format_to_string(allowance, req.token_in.decimals as u8)?;
        }

        if allowance >= amount_in {
            let pool = crate::manager::Context::get_global_sqlite_pool()?;
            let assets = self
                .token0_assets(
                    &pool,
                    &req.chain_code,
                    &req.token_in.token_addr,
                    &req.recipient,
                )
                .await?;
            if self.check_bal(&req.amount_in, &assets.balance)? {
                self.simulate_and_fill(&req, &quote_resp, &mut res).await?;
            }

            return Ok(res);
        } else {
            let diff = amount_in - allowance;
            res.need_approve_amount = format_to_string(diff, req.token_in.decimals as u8)?;

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
        let instance = time::Instant::now();

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let main_coin = CoinRepo::main_coin(&req.chain_code, &pool).await?;

        let adapter = ChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;
        // 模拟报价
        let (amount_out, consumer, content) = adapter
            .swap_quote(req, quote_resp, &main_coin.symbol)
            .await?;

        let fee_resp = EstimateFeeResp::new(main_coin.symbol, main_coin.chain_code, content);
        res.consumer = consumer;
        res.fee = fee_resp;

        // 重新覆盖amount_out,使用模拟的值
        res.set_amount_out(amount_out, req.token_out.decimals);
        tracing::warn!("simulate time: {}", instance.elapsed().as_secs_f64());

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

        // 查询余额是否足够
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let token_in = AssetsRepo::get_by_addr_token(
            &pool,
            &req.chain_code,
            &req.token_in.token_addr,
            &req.recipient,
        )
        .await?;
        if !self.check_bal(&req.amount_in, &token_in.balance)? {
            return Err(crate::BusinessError::Chain(
                crate::ChainError::InsufficientBalance,
            ))?;
        }

        // 广播事件
        let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new(
            wallet_database::entities::bill::BillKind::Swap,
            Process::Broadcast,
        ));
        FrontendNotifyEvent::new(data).send().await?;
        let adapter = ChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;

        let chain_code = ChainCode::try_from(req.chain_code.as_str())?;
        let swap_inner_type = QuoteReq::swap_type(
            chain_code,
            &req.token_in.token_addr,
            &req.token_out.token_addr,
        )?;

        let resp = match swap_inner_type {
            SwapInnerType::Deposit => {
                let params = DepositReq {
                    from: req.recipient.clone(),
                    token: req.token_out.token_addr.clone(),
                    amount: req.amount_in.clone(),
                    chain_code: req.chain_code.clone(),
                };
                let value = wallet_utils::unit::convert_to_u256(
                    &req.amount_in,
                    req.token_in.decimals as u8,
                )?;
                adapter.deposit(&params, fee, key, value).await?
            }
            SwapInnerType::Withdraw => {
                let params = WithdrawReq {
                    from: req.recipient.clone(),
                    token: req.token_in.token_addr.clone(),
                    amount: req.amount_in.clone(),
                    chain_code: req.chain_code.clone(),
                };
                let value = wallet_utils::unit::convert_to_u256(
                    &req.amount_in,
                    req.token_in.decimals as u8,
                )?;

                adapter.withdraw(&params, fee, key, value).await?
            }
            SwapInnerType::Swap => adapter.swap(&req, fee, key).await?,
        };

        //  if token_out if new assets add it
        let token_out = req.token_out.clone();
        let out_assets = AssetsRepo::get_by_addr_token_opt(
            &pool,
            &req.chain_code,
            &token_out.token_addr,
            &req.recipient,
        )
        .await?;
        if out_assets.is_none() {
            AssetsDomain::swap_sync_assets(
                token_out,
                req.recipient.clone(),
                req.chain_code.clone(),
            )
            .await?;
        }

        // 写入本地交易记录表
        let mut new_bill = NewBillEntity::try_from(req)?;
        new_bill.hash = resp.tx_hash.clone();
        new_bill.resource_consume = resp.resource_consume()?;
        new_bill.transaction_fee = resp.fee;
        BillDomain::create_bill(new_bill).await?;

        Ok(resp.tx_hash)
    }

    pub async fn token_list(
        &self,
        req: SwapTokenListReq,
    ) -> Result<Pagination<SwapTokenInfo>, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let chain_code = (!req.chain_code.is_empty()).then(|| req.chain_code.clone());

        let list = AccountEntity::lists_by_wallet_address(
            &req.wallet_address,
            Some(req.account_id as u32),
            chain_code.as_deref(),
            pool.as_ref(),
        )
        .await?;
        let address = list
            .iter()
            .map(|x| x.address.clone())
            .collect::<Vec<String>>();

        let coins = CoinRepo::coin_list_with_assets(
            &req.search,
            req.exclude_token,
            req.chain_code.to_string(),
            address,
            req.page_num,
            req.page_size,
            pool.clone(),
        )
        .await?;

        let mut resp = Pagination::<SwapTokenInfo> {
            page: coins.page,
            page_size: coins.page_size,
            total_count: coins.total_count,
            data: vec![],
        };

        let currency = {
            let state = crate::app_state::APP_STATE.read().await;
            state.currency().to_string()
        };
        let exchange = ExchangeRateRepo::exchange_rate(&currency, &pool).await?;

        for coin in coins.data {
            let balance = if coin.balance != "0" {
                let unit_price = unit::string_to_f64(&coin.price)? * exchange.rate;

                let amount = unit::string_to_f64(&coin.balance)?;
                BalanceInfo::new(amount, Some(unit_price), &currency)
            } else {
                BalanceInfo::default()
            };

            let token_info = SwapTokenInfo {
                symbol: coin.symbol,
                decimals: coin.decimals as u32,
                token_addr: coin.token_address,
                name: coin.name,
                chain_code: coin.chain_code,
                balance,
            };
            resp.data.push(token_info);
        }

        Ok(resp)
    }

    pub async fn chain_list(&self) -> Result<Vec<ChainDex>, crate::ServiceError> {
        Ok(self.client.chain_list().await?.chain_dexs)
    }

    pub async fn approve_fee(
        &self,
        req: ApproveReq,
    ) -> Result<EstimateFeeResp, crate::ServiceError> {
        let adapter = ChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;

        let value = alloy::primitives::U256::MAX;
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let main_coin = CoinRepo::main_coin(&req.chain_code, &pool).await?;

        let fee = adapter.approve_fee(&req, value, &main_coin.symbol).await?;

        let fee_resp = EstimateFeeResp::new(main_coin.symbol, main_coin.chain_code.clone(), fee);
        Ok(fee_resp)
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

        // check balance
        let token_in = self
            .token0_assets(&pool, &req.chain_code, &req.contract, &req.from)
            .await?;
        if !self.check_bal(&req.value, &token_in.balance)? {
            return Err(crate::BusinessError::Chain(
                crate::ChainError::InsufficientBalance,
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
        let resp = adapter.approve(&req, private_key, value).await?;

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
            &&resp.tx_hash.clone(),
            &req.approve_type,
        );
        TaskQueueDomain::send_or_to_queue(backend_req, SWAP_APPROVE_SAVE).await?;

        // 写入本地交易
        let mut new_bill = NewBillEntity::from(req);
        new_bill.hash = resp.tx_hash.clone();
        new_bill.symbol = coin.symbol;
        new_bill.resource_consume = resp.resource_consume()?;
        new_bill.transaction_fee = resp.fee;
        BillDomain::create_bill(new_bill).await?;

        Ok(resp.tx_hash)
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

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let mut used_ids = vec![];
        for item in resp.list.into_iter() {
            let coin =
                CoinRepo::coin_by_chain_address(&item.chain_code, &item.token_addr, &pool).await?;
            if item.limit_type == ApproveReq::UN_LIMIT {
                let mut approve_info = ApproveList::from(item);
                approve_info.symbol = coin.symbol;
                res.push(approve_info)
            } else {
                // 获取allowance 情况
                let adapter =
                    ChainAdapterFactory::get_transaction_adapter(&item.chain_code).await?;
                let allowance = adapter
                    .allowance(&item.owner_address, &item.token_addr, &item.spender)
                    .await?;

                // 实际授权为0,丢弃
                if allowance == alloy::primitives::U256::ZERO {
                    used_ids.push(item.id);
                } else {
                    let unit = coin.decimals as u8;
                    let origin_allowance = wallet_utils::unit::convert_to_u256(&item.value, unit)?;
                    let mut approve_info = ApproveList::from(item);

                    approve_info.amount =
                        wallet_utils::unit::format_to_string(origin_allowance, unit)?;
                    let remain = origin_allowance - (origin_allowance - allowance);

                    approve_info.remaining_allowance =
                        wallet_utils::unit::format_to_string(remain, unit)?;
                    approve_info.symbol = coin.symbol;
                    res.push(approve_info);
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
        let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new(
            wallet_database::entities::bill::BillKind::UnApprove,
            Process::Building,
        ));
        FrontendNotifyEvent::new(data).send().await?;

        let private_key =
            ChainTransDomain::get_key(&req.from, &req.chain_code, &password, &None).await?;
        let adapter = ChainAdapterFactory::get_transaction_adapter(&req.chain_code).await?;

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let coin = CoinRepo::coin_by_chain_address(&req.chain_code, &req.contract, &pool).await?;

        let data = NotifyEvent::TransactionProcess(TransactionProcessFrontend::new(
            wallet_database::entities::bill::BillKind::Approve,
            Process::Broadcast,
        ));
        FrontendNotifyEvent::new(data).send().await?;
        let value = alloy::primitives::U256::ZERO;
        let resp = adapter.approve(&req, private_key, value).await?;

        let backend = ApproveCancelReq {
            spender: req.spender.clone(),
            token_addr: req.contract.clone(),
            owner_address: req.from.clone(),
            tx_hash: resp.tx_hash.clone(),
        };
        TaskQueueDomain::send_or_to_queue(backend, SWAP_APPROVE_CANCEL).await?;

        // 写入本地交易
        let mut new_bill = NewBillEntity::from(req);
        new_bill.hash = resp.tx_hash.clone();
        new_bill.symbol = coin.symbol;
        new_bill.tx_kind = wallet_database::entities::bill::BillKind::UnApprove;
        new_bill.resource_consume = resp.resource_consume()?;
        new_bill.transaction_fee = resp.fee;
        BillDomain::create_bill(new_bill).await?;

        Ok(resp.tx_hash)
    }
}
