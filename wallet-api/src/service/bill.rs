use crate::{
    domain::{self, app::config::ConfigDomain},
    response_vo::CoinCurrency,
};
use sqlx::{Pool, Sqlite};
use wallet_chain_interact::BillResourceConsume;
use wallet_database::{
    dao::{bill::BillDao, multisig_account::MultisigAccountDaoV1},
    entities::{
        account::AccountEntity,
        bill::{BillEntity, BillKind, NewBillEntity},
        multisig_account::MultiAccountOwner,
    },
    pagination::Pagination,
    repositories::{account::AccountRepoTrait, bill::BillRepoTrait},
};

pub struct BillService<T: BillRepoTrait + AccountRepoTrait> {
    repo: T,
}

impl<T: BillRepoTrait + AccountRepoTrait> BillService<T> {
    pub fn new(repo: T) -> Self {
        Self { repo }
    }

    pub async fn bill_lists(
        &mut self,
        root_addr: Option<String>,
        account_id: Option<u32>,
        addr: Option<String>,
        chain_code: Option<&str>,
        symbol: Option<&str>,
        is_multisig: Option<i64>,
        filter_min_value: Option<bool>,
        start: Option<i64>,
        end: Option<i64>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<BillEntity>, crate::ServiceError> {
        // TODO transaction don't need
        let adds = if let Some(addr) = addr {
            vec![addr]
        } else {
            let account = self
                .repo
                .account_list_by_wallet_address_and_account_id_and_chain_codes(
                    root_addr.as_deref(),
                    account_id,
                    Vec::new(),
                )
                .await?;

            account.iter().map(|item| item.address.clone()).collect()
        };

        // 过滤最小金额
        let min_value = match (chain_code, symbol, filter_min_value) {
            (Some(chain_code), Some(symbol), Some(true)) => {
                ConfigDomain::get_config_min_value(chain_code, symbol).await?
            }
            _ => None,
        };

        let mut lists = self
            .repo
            .bill_lists(
                &adds,
                chain_code,
                symbol,
                is_multisig,
                min_value,
                start,
                end,
                page,
                page_size,
            )
            .await
            .map_err(crate::SystemError::Database)?;

        lists
            .data
            .iter_mut()
            .for_each(|item| item.value = wallet_utils::unit::truncate_to_8_decimals(&item.value));

        Ok(lists)
    }

    pub async fn sync_bills(
        &self,
        chain_code: &str,
        address: &str,
    ) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let time = self
            .get_last_bill_time(chain_code, address, pool.clone())
            .await?;

        let backend = crate::manager::Context::get_global_backend_api()?;
        let resp = backend.record_lists(chain_code, address, time).await;
        match resp {
            Ok(resp) => {
                for item in resp.list {
                    let multisig_tx = item.is_multisig > 0;

                    let status = if item.status { 2 } else { 3 };

                    let tx_kind = BillKind::try_from(item.tx_kind)?;

                    let fee = item
                        .transaction_fee
                        .map_or_else(|| "0".to_string(), |fee| fee.to_string());

                    // BillResourceConsume::
                    let consumer = BillResourceConsume {
                        net_used: item.net_used.unwrap_or_default(),
                        energy_used: item.energy_used.unwrap_or_default(),
                    };

                    let time = wallet_utils::time::datetime_to_timestamp(&item.transaction_time);

                    let new_entity = NewBillEntity {
                        hash: item.tx_hash,
                        from: item.from_addr,
                        to: item.to_addr,
                        token: item.token,
                        chain_code: item.chain_code,
                        symbol: item.symbol.to_uppercase(),
                        status,
                        value: item.value,
                        transaction_fee: fee,
                        resource_consume: consumer.to_json_str()?,
                        transaction_time: time,
                        multisig_tx,
                        tx_type: item.transfer_type,
                        tx_kind,
                        queue_id: item.queue_id,
                        block_height: item.block_height.to_string(),
                        notes: item.notes,
                    };

                    domain::bill::BillDomain::create_bill(new_entity).await?;
                }
            }
            Err(e) => {
                tracing::warn!("record lists error:{e:?}");
            }
        }
        Ok(())
    }

    pub async fn coin_currency_price(
        &self,
        chain_code: String,
        symbol: String,
    ) -> Result<CoinCurrency, crate::ServiceError> {
        let currency = crate::config::CONFIG.read().await;
        let currency = currency.currency();

        let token =
            domain::coin::TokenCurrencyGetter::get_currency(currency, &chain_code, &symbol).await?;

        Ok(CoinCurrency {
            currency: currency.to_string(),
            unit_price: token.currency_price,
        })
    }

    // For non-TRON networks, directly subtract 5 days from the time in the order.
    // For TRON network:
    // 1. Non-multisig accounts: the logic is the same as for non-TRON networks.
    // 2. Multisig accounts:
    //    1. If the account is a participant: only synchronize the order data created after the account was created.
    //    2. If the account is the creator: the logic is the same as for non-TRON networks.
    async fn get_last_bill_time(
        &self,
        chain_code: &str,
        address: &str,
        pool: std::sync::Arc<Pool<Sqlite>>,
    ) -> Result<Option<String>, crate::ServiceError> {
        let bill = BillDao::last_bill(chain_code, address, pool.as_ref())
            .await
            .map_err(|e| crate::SystemError::Database(e.into()))?;

        let adjusted_time = |bill: Option<BillEntity>| {
            bill.map(|bill| {
                let time = bill.transaction_time - std::time::Duration::from_secs(86400 * 5);
                time.format("%Y-%m-%d %H:%M:%S").to_string()
            })
            .unwrap_or_else(|| sqlx::types::chrono::NaiveDateTime::default().to_string())
        };

        // Non-Tron chains
        if chain_code != "tron" {
            return Ok(Some(adjusted_time(bill)));
        }

        // Tron-specific logic
        let account =
            AccountEntity::find_one_by_address_chain_code(address, chain_code, pool.as_ref())
                .await?;

        if account.is_some() {
            return Ok(Some(adjusted_time(bill)));
        }

        // Check multisig account if regular account not found
        let condition = vec![
            ("address", address),
            ("chain_code", chain_code),
            ("is_del", "0"),
        ];
        let account = MultisigAccountDaoV1::find_by_conditions(condition, pool.as_ref())
            .await
            .map_err(|e| crate::SystemError::Database(e.into()))?;

        match account {
            Some(account) => {
                if account.owner == MultiAccountOwner::Participant.to_i8() {
                    // If participant, compare bill and account creation time
                    if let Some(bill) = bill {
                        let crate_time =
                            account.created_at + std::time::Duration::from_secs(86400 * 5);
                        if bill.transaction_time > crate_time {
                            // tracing::warn!("参与方，订单的时间大于多签账号创建的时间");
                            return Ok(Some(
                                bill.transaction_time
                                    .format("%Y-%m-%d %H:%M:%S")
                                    .to_string(),
                            ));
                        }
                    }
                    // tracing::warn!("参与方，但是最后的订单时间小于多签账号的创建时间");
                    let time = account.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
                    Ok(Some(time))
                } else {
                    Ok(Some(adjusted_time(bill)))
                }
            }
            None => Err(crate::BusinessError::MultisigAccount(
                crate::MultisigAccountError::NotFound,
            )
            .into()),
        }
    }
}
