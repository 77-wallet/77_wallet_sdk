use crate::messaging::mqtt::topics::AcctChange;
use wallet_chain_interact::{BillResourceConsume, QueryTransactionResult};
use wallet_database::{
    dao::{bill::BillDao, multisig_account::MultisigAccountDaoV1},
    entities::{
        self,
        account::AccountEntity,
        bill::{BillEntity, BillKind, BillStatus, NewBillEntity},
        multisig_account::MultiAccountOwner,
    },
    DbPool,
};
use wallet_transport_backend::response_vo::transaction::SyncBillResp;
use wallet_types::constant::chain_code;

pub struct ApiBillDomain;

impl ApiBillDomain {
    pub async fn create_bill<T>(
        params: entities::api_bill::ApiBillEntity<T>,
    ) -> Result<(), crate::ServiceError>
    where
        T: serde::Serialize,
    {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        Ok(BillDao::create(params, &*pool).await?)
    }

    // 对于swap的交易，先判断有没有对应的交易
    pub async fn create_check_swap<T>(
        tx: entities::bill::NewBillEntity<T>,
        pool: &DbPool,
    ) -> Result<(), crate::ServiceError>
    where
        T: serde::Serialize,
    {
        match BillDao::get_one_by_hash(&tx.hash, pool.as_ref()).await? {
            Some(bill) if bill.tx_kind == BillKind::Swap.to_i8() => {
                BillDao::update_all(pool.clone(), tx, bill.id).await?;
            }
            _ => {
                BillDao::create(tx, pool.as_ref()).await?;
            }
        }

        Ok(())
    }

    // query tx resource consume
    pub async fn get_bill_resource_consumer(
        tx_hash: &str,
        chain_code: &str,
    ) -> Result<String, crate::ServiceError> {
        let adapter =
            super::chain::adapter::ChainAdapterFactory::get_transaction_adapter(chain_code).await?;
        let res = adapter.query_tx_res(tx_hash).await?;
        match res {
            Some(res) => Ok(res.resource_consume),
            None => Ok("".to_string()),
        }
    }

    pub async fn get_onchain_bill(
        tx_hash: &str,
        chain_code: &str,
    ) -> Result<Option<QueryTransactionResult>, crate::ServiceError> {
        let adapter =
            super::chain::adapter::ChainAdapterFactory::get_transaction_adapter(chain_code).await?;

        Ok(adapter.query_tx_res(tx_hash).await?)
    }

    pub async fn handle_sync_bill(item: SyncBillResp) -> Result<(), crate::ServiceError> {
        if item.value == 0.0 {
            return Ok(());
        }

        let status = if item.status {
            BillStatus::Success.to_i8()
        } else {
            BillStatus::Failed.to_i8()
        };

        let transaction_fee = item.transaction_fee();

        let new_entity = NewBillEntity {
            hash: item.tx_hash,
            from: item.from_addr,
            to: item.to_addr,
            token: item.token,
            chain_code: item.chain_code,
            symbol: item.symbol,
            status,
            value: item.value,
            transaction_fee,
            resource_consume: BillResourceConsume {
                net_used: item.net_used.unwrap_or_default(),
                energy_used: item.energy_used.unwrap_or_default(),
            }
                .to_json_str()?,
            transaction_time: wallet_utils::time::datetime_to_timestamp(&item.transaction_time),
            multisig_tx: item.is_multisig > 0,
            tx_type: item.transfer_type,
            tx_kind: BillKind::try_from(item.tx_kind)?,
            queue_id: item.queue_id.unwrap_or("".to_string()),
            block_height: item.block_height.to_string(),
            notes: item.notes,
            signer: item.signer,
            extra: item.extra,
        };

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        if new_entity.chain_code == chain_code::TON {
            AcctChange::handle_ton_bill(new_entity, &pool).await?;
        } else {
            BillDomain::create_check_swap(new_entity, &pool).await?;
        }

        Ok(())
    }

    pub(crate) async fn sync_bills(
        chain_code: &str,
        address: &str,
    ) -> Result<(), crate::ServiceError> {
        let start_time = BillDomain::get_last_bill_time(chain_code, address).await?;
        // let start_time = None;

        let backend = crate::manager::Context::get_global_backend_api()?;
        let resp = backend
            .record_lists(chain_code, address, start_time)
            .await?;

        for item in resp.list {
            if let Err(e) = BillDomain::handle_sync_bill(item).await {
                tracing::warn!("[bill::sync_bills] failed {}", e);
            }
        }

        Ok(())
    }

    pub(crate) fn handle_hash(hash: &str) -> String {
        match hash.split_once(':') {
            Some((before, _)) => before.to_owned(),
            None => hash.to_owned(),
        }
    }

    // For non-TRON networks, directly subtract 5 days from the time in the order.
    // For TRON network:
    // 1. Non-multisig accounts: the logic is the same as for non-TRON networks.
    // 2. Multisig accounts:
    //    1. If the account is a participant: only synchronize the order data created after the account was created.
    //    2. If the account is the creator: the logic is the same as for non-TRON networks.
    pub(crate) async fn get_last_bill_time(
        chain_code: &str,
        address: &str,
    ) -> Result<Option<String>, crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;

        let bill = BillDao::last_bill(chain_code, address, pool.as_ref())
            .await
            .map_err(|e| crate::ServiceError::Database(e.into()))?;

        let adjusted_time = |bill: Option<BillEntity>| {
            bill.map(|bill| {
                let time = bill.transaction_time - std::time::Duration::from_secs(86400 * 5);
                time.format("%Y-%m-%d %H:%M:%S").to_string()
            })
                .unwrap_or_else(|| sqlx::types::chrono::NaiveDateTime::default().to_string())
        };

        // Non-Tron chains
        if chain_code != wallet_types::constant::chain_code::TRON {
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
            .map_err(|e| crate::ServiceError::Database(e.into()))?;

        if let Some(account) = account {
            if account.owner == MultiAccountOwner::Participant.to_i8() {
                // If participant, compare bill and account creation time
                if let Some(bill) = bill {
                    let crate_time = account.created_at + std::time::Duration::from_secs(86400 * 5);
                    if bill.transaction_time > crate_time {
                        return Ok(Some(
                            bill.transaction_time
                                .format("%Y-%m-%d %H:%M:%S")
                                .to_string(),
                        ));
                    }
                }
                let time = account.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
                return Ok(Some(time));
            }
            return Ok(Some(adjusted_time(bill)));
        }

        Err(crate::BusinessError::MultisigAccount(crate::MultisigAccountError::NotFound).into())
    }
}
