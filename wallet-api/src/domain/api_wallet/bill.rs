use crate::domain::api_wallet::adapter;
use chrono::Utc;
use tracing::Instrument;
use wallet_chain_interact::{BillResourceConsume, QueryTransactionResult};
use wallet_database::repositories::{api_account::ApiAccountRepo, api_bill::ApiBillRepo};
use wallet_database::{
    DbPool,
    entities::api_bill::{ApiBillEntity, ApiBillKind, ApiBillStatus},
};
use wallet_transport_backend::response_vo::transaction::SyncBillResp;
use wallet_types::constant::chain_code;

pub struct ApiBillDomain;

impl ApiBillDomain {
    pub async fn create_bill(params: ApiBillEntity) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        Ok(ApiBillRepo::create(params, &pool).await?)
    }

    // 对于swap的交易，先判断有没有对应的交易
    pub async fn create_check_swap(
        tx: ApiBillEntity,
        pool: &DbPool,
    ) -> Result<(), crate::ServiceError> {
        match ApiBillRepo::get_one_by_hash(&tx.hash, &pool).await? {
            Some(bill) if bill.tx_kind == ApiBillKind::Swap => {
                ApiBillRepo::update_all(pool, tx, bill.id).await?;
            }
            _ => {
                ApiBillRepo::create(tx, pool).await?;
            }
        }

        Ok(())
    }

    // query tx resource consume
    pub async fn get_bill_resource_consumer(
        tx_hash: &str,
        chain_code: &str,
    ) -> Result<String, crate::ServiceError> {
        let adapter = adapter::ChainAdapterFactory::get_transaction_adapter(chain_code).await?;
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
        let adapter = adapter::ChainAdapterFactory::get_transaction_adapter(chain_code).await?;

        Ok(adapter.query_tx_res(tx_hash).await?)
    }

    pub async fn handle_sync_bill(item: SyncBillResp) -> Result<(), crate::ServiceError> {
        if item.value == 0.0 {
            return Ok(());
        }

        let status = if item.status {
            ApiBillStatus::Success.to_i8()
        } else {
            ApiBillStatus::Failed.to_i8()
        };

        let transaction_fee = item.transaction_fee();

        let new_entity = ApiBillEntity {
            id: 0,
            hash: item.tx_hash,
            from_addr: item.from_addr,
            to_addr: item.to_addr,
            token: item.token,
            chain_code: item.chain_code,
            symbol: item.symbol,
            status,
            value: item.value.to_string(),
            transaction_fee,
            resource_consume: BillResourceConsume {
                net_used: item.net_used.unwrap_or_default(),
                energy_used: item.energy_used.unwrap_or_default(),
            }
            .to_json_str()?,
            // transaction_time: item.transaction_time,
            transaction_time: Utc::now().into(),
            tx_kind: ApiBillKind::try_from(item.tx_kind)?,
            owner: "".to_string(),
            queue_id: item.queue_id.unwrap_or("".to_string()),
            block_height: item.block_height.to_string(),
            notes: item.notes,
            signer: item.signer.join(","),
            // extra: item.extra.ok_or(""),
            extra: "".to_string(),
            created_at: Default::default(),
            transfer_type: 0,
            is_multisig: 0,
            updated_at: None,
        };

        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        if new_entity.chain_code == chain_code::TON {
            Self::handle_ton_bill(new_entity, &pool).await?;
        } else {
            Self::create_check_swap(new_entity, &pool).await?;
        }

        Ok(())
    }

    pub(crate) async fn sync_bills(
        chain_code: &str,
        address: &str,
    ) -> Result<(), crate::ServiceError> {
        let start_time = ApiBillDomain::get_last_bill_time(chain_code, address).await?;
        // let start_time = None;

        let backend = crate::manager::Context::get_global_backend_api()?;
        let resp = backend
            .record_lists(chain_code, address, start_time)
            .await?;

        for item in resp.list {
            if let Err(e) = ApiBillDomain::handle_sync_bill(item).await {
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

        let bill = ApiBillRepo::last_bill(&pool, chain_code, address)
            .await
            .map_err(|e| crate::ServiceError::Database(e.into()))?;

        let adjusted_time = |bill: Option<ApiBillEntity>| {
            bill.map(|bill| {
                let time = bill.transaction_time - std::time::Duration::from_secs(86400 * 5);
                time.format("%Y-%m-%d %H:%M:%S").to_string()
            })
            .unwrap_or_else(|| sqlx::types::chrono::NaiveDateTime::default().to_string())
        };

        // Non-Tron chains
        if chain_code != chain_code::TRON {
            return Ok(Some(adjusted_time(bill)));
        }

        // Tron-specific logic
        let account = ApiAccountRepo::find_one_by_address_chain_code(address, chain_code, &pool).await?;


        if account.is_some() {
            return Ok(Some(adjusted_time(bill)));
        }


        Err(crate::ServiceError::Business(crate::AssetsError::NotFound.into()))
    }

    pub async fn handle_ton_bill(
        mut tx: ApiBillEntity,
        pool: &DbPool,
    ) -> Result<(), crate::ServiceError> {
        let origin_hash = tx.hash.clone();
        let hashs = origin_hash.split(":").collect::<Vec<_>>();

        if hashs.len() == 2 {
            tx.hash = hashs[0].to_string();
            let in_hash = hashs[1];
            if let Some(bill) =
                ApiBillRepo::get_by_hash_and_type(pool, in_hash, tx.transfer_type as i64).await?
            {
                ApiBillRepo::update_all(pool, tx, bill.id).await?;
            } else {
                ApiBillRepo::create(tx, pool).await?;
            }
        } else {
            ApiBillRepo::create(tx, pool).await?;
        }

        Ok(())
    }
}
