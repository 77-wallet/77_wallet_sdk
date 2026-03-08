use crate::{
    CoreDbPool,
    dao::{multisig_account::MultisigAccountDaoV1, multisig_member::MultisigMemberDaoV1},
    entities::{
        account::AccountEntity,
        assets::AssetsEntity,
        coin::CoinMultisigStatus,
        multisig_account::{
            MultisigAccountData, MultisigAccountEntity, MultisigAccountStatus,
            NewMultisigAccountEntity,
        },
        multisig_member::MultisigMemberEntities,
        wallet::WalletEntity,
    },
    pagination::Pagination,
};

pub struct MultisigAccountRepo {
    pool: CoreDbPool,
}

impl MultisigAccountRepo {
    pub fn new(db_pool: crate::CoreDbPool) -> Self {
        Self { pool: db_pool }
    }
}

impl MultisigAccountRepo {
    pub async fn account_count(&self, chain_code: &str) -> i64 {
        let account =
            MultisigAccountDaoV1::account_count(chain_code, self.pool.clone().into_inner()).await;
        account.unwrap_or_default()
    }

    pub async fn account_count_with_pool(pool: &CoreDbPool, chain_code: &str) -> i64 {
        let account = MultisigAccountDaoV1::account_count(chain_code, pool.clone().into_inner()).await;
        account.unwrap_or_default()
    }

    pub async fn update_name(&self, id: &str, name: &str) -> Result<(), crate::Error> {
        Ok(MultisigAccountDaoV1::update_name(id, name, self.pool.as_ref()).await?)
    }

    pub async fn cancel_multisig(
        &self,
        account: &MultisigAccountEntity,
    ) -> Result<(), crate::Error> {
        let mut tx = self
            .pool
            .as_ref()
            .begin()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        // delete account
        MultisigAccountDaoV1::logic_del_multisig_account(&account.id, tx.as_mut()).await?;

        // recover assets
        if account.chain_code == "tron" {
            AssetsEntity::update_tron_multisig_assets(
                &account.address,
                &account.chain_code,
                CoinMultisigStatus::NotMultisig.to_i8(),
                tx.as_mut(),
            )
            .await?;
        }

        tx.commit().await.map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        Ok(())
    }

    pub async fn found_by_id(
        &self,
        id: &str,
    ) -> Result<Option<MultisigAccountEntity>, crate::Error> {
        let conditions = vec![("id", id)];
        Ok(MultisigAccountDaoV1::find_by_conditions(conditions, self.pool.as_ref()).await?)
    }

    pub async fn found_by_id_with_pool(
        pool: &CoreDbPool,
        id: &str,
    ) -> Result<Option<MultisigAccountEntity>, crate::Error> {
        let conditions = vec![("id", id)];
        Ok(MultisigAccountDaoV1::find_by_conditions(conditions, pool.as_ref()).await?)
    }

    pub async fn found_one_id(
        id: &str,
        pool: &CoreDbPool,
    ) -> Result<Option<MultisigAccountEntity>, crate::Error> {
        let conditions = vec![("id", id)];
        Ok(MultisigAccountDaoV1::find_by_conditions(conditions, pool.as_ref()).await?)
    }

    pub async fn found_by_address(
        &self,
        address: &str,
    ) -> Result<Option<MultisigAccountEntity>, crate::Error> {
        let conditions = vec![("address", address)];
        Ok(MultisigAccountDaoV1::find_by_conditions(conditions, self.pool.as_ref()).await?)
    }

    pub async fn found_by_address_with_pool(
        pool: &CoreDbPool,
        address: &str,
    ) -> Result<Option<MultisigAccountEntity>, crate::Error> {
        let conditions = vec![("address", address)];
        Ok(MultisigAccountDaoV1::find_by_conditions(conditions, pool.as_ref()).await?)
    }

    pub async fn member_by_account_id(
        &self,
        id: &str,
    ) -> Result<MultisigMemberEntities, crate::Error> {
        Ok(MultisigMemberDaoV1::list_by_account_id(id, self.pool.as_ref()).await?)
    }

    pub async fn member_by_account_id_with_pool(
        pool: &CoreDbPool,
        id: &str,
    ) -> Result<MultisigMemberEntities, crate::Error> {
        Ok(MultisigMemberDaoV1::list_by_account_id(id, pool.as_ref()).await?)
    }

    pub async fn self_address_by_id(
        &self,
        id: &str,
    ) -> Result<MultisigMemberEntities, crate::Error> {
        Ok(MultisigMemberDaoV1::get_self_by_id(id, self.pool.as_ref()).await?)
    }

    pub async fn self_address_by_id_with_pool(
        pool: &CoreDbPool,
        id: &str,
    ) -> Result<MultisigMemberEntities, crate::Error> {
        Ok(MultisigMemberDaoV1::get_self_by_id(id, pool.as_ref()).await?)
    }

    pub async fn update_confirm_status(
        &self,
        account_id: &str,
        chain_code: &str,
        self_address: &mut MultisigMemberEntities,
    ) -> Result<(), crate::Error> {
        let mut tx = self
            .pool
            .as_ref()
            .begin()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        for item in self_address.0.iter_mut() {
            // let req = entities::account::QueryReq::new_address_chain(&item.address, chain_code);

            let account = AccountEntity::detail(
                tx.as_mut(),
                None,
                Some(&item.address),
                None,
                Some(chain_code),
            )
            .await?
            .ok_or(crate::DatabaseError::ReturningNone)?;
            // let pubkey = account.map_or_else(|| "".to_string(), |account| account.pubkey);
            let wallet = WalletEntity::detail(tx.as_mut(), &account.wallet_address).await?;
            let uid = wallet.map_or_else(|| "".to_string(), |wallet| wallet.uid);

            MultisigMemberDaoV1::sync_confirmed_and_pubkey_status(
                account_id,
                &item.address,
                &account.pubkey,
                1,
                &uid,
                tx.as_mut(),
            )
            .await?;
            item.uid = uid;
            item.pubkey = account.pubkey;
        }

        let member = MultisigMemberDaoV1::find_records_by_id(account_id, tx.as_mut()).await?;
        if member.all_confirmed() {
            MultisigAccountDaoV1::sync_status(
                account_id,
                MultisigAccountStatus::Confirmed,
                tx.as_mut(),
            )
            .await?;
        }

        tx.commit().await.map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        Ok(())
    }

    pub async fn create_with_member(
        &self,
        params: &NewMultisigAccountEntity,
    ) -> Result<(), crate::Error> {
        let mut tx = self
            .pool
            .as_ref()
            .begin()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        MultisigAccountDaoV1::insert(params, tx.as_mut()).await?;

        MultisigMemberDaoV1::batch_add(&params.member_list, tx.as_mut()).await?;

        if params.chain_code == "tron" {
            AssetsEntity::update_tron_multisig_assets(
                &params.address,
                &params.chain_code,
                CoinMultisigStatus::Deploying.to_i8(),
                tx.as_mut(),
            )
            .await?;
        }

        tx.commit().await.map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;
        Ok(())
    }

    pub async fn account_list(
        &self,
        owner: bool,
        chain_code: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<MultisigAccountEntity>, crate::Error> {
        let rs = MultisigAccountDaoV1::account_list(
            owner,
            chain_code,
            self.pool.clone().into_inner(),
            page,
            page_size,
        )
        .await?;
        Ok(rs)
    }

    pub async fn account_list_with_pool(
        pool: &CoreDbPool,
        owner: bool,
        chain_code: Option<&str>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<MultisigAccountEntity>, crate::Error> {
        let rs = MultisigAccountDaoV1::account_list(
            owner,
            chain_code,
            pool.clone().into_inner(),
            page,
            page_size,
        )
        .await?;
        Ok(rs)
    }

    // 钱包账户
    pub async fn wallet_account(
        &self,
        address: &str,
        chain_code: &str,
    ) -> Result<Option<AccountEntity>, crate::Error> {
        // let req = crate::entities::account::QueryReq::new_address_chain(address, chain_code);

        AccountEntity::detail(self.pool.as_ref(), None, Some(address), None, Some(chain_code)).await
        //     .ok_or(crate::DatabaseError::ReturningNone)?;

        // AccountEntity::detail(&*pool, &req).await
    }

    pub async fn wallet_account_with_pool(
        pool: &CoreDbPool,
        address: &str,
        chain_code: &str,
    ) -> Result<Option<AccountEntity>, crate::Error> {
        AccountEntity::detail(pool.as_ref(), None, Some(address), None, Some(chain_code)).await
    }

    pub async fn update_by_id(
        &self,
        id: &str,
        params: std::collections::HashMap<String, String>,
    ) -> Result<MultisigAccountEntity, crate::Error> {
        Ok(MultisigAccountDaoV1::update_by_id(id, params, self.pool.as_ref()).await?)
    }

    // get multisig account(include cancel account) and member information
    pub async fn multisig_data(
        &self,
        account_id: &str,
    ) -> Result<MultisigAccountData, crate::Error> {
        // get account
        let conditions = vec![("id", account_id)];

        let account = MultisigAccountDaoV1::find_by_conditions(conditions, self.pool.as_ref())
            .await?
            .ok_or(crate::DatabaseError::ReturningNone)?;

        let member =
            MultisigMemberDaoV1::find_records_by_id(account_id, self.pool.as_ref()).await?;

        Ok(MultisigAccountData::new(account, member))
    }

    pub async fn multisig_data_with_pool(
        pool: &CoreDbPool,
        account_id: &str,
    ) -> Result<MultisigAccountData, crate::Error> {
        let conditions = vec![("id", account_id)];

        let account = MultisigAccountDaoV1::find_by_conditions(conditions, pool.as_ref())
            .await?
            .ok_or(crate::DatabaseError::ReturningNone)?;

        let member = MultisigMemberDaoV1::find_records_by_id(account_id, pool.as_ref()).await?;

        Ok(MultisigAccountData::new(account, member))
    }

    pub async fn multisig_raw_data(
        account_id: &str,
        pool: CoreDbPool,
    ) -> Result<MultisigAccountData, crate::Error> {
        // get account
        let conditions = vec![("id", account_id)];

        let account = MultisigAccountDaoV1::find_by_conditions(conditions, pool.as_ref())
            .await?
            .ok_or(crate::DatabaseError::ReturningNone)?;

        let member = MultisigMemberDaoV1::find_records_by_id(account_id, pool.as_ref()).await?;

        Ok(MultisigAccountData::new(account, member))
    }

    pub async fn find_doing_account(
        &self,
        chain_code: &str,
        address: &str,
    ) -> Result<Option<MultisigAccountEntity>, crate::Error> {
        let a = MultisigAccountDaoV1::find_doing_account(chain_code, address, self.pool.as_ref())
            .await?;
        Ok(a)
    }

    pub async fn find_doing_account_with_pool(
        pool: &CoreDbPool,
        chain_code: &str,
        address: &str,
    ) -> Result<Option<MultisigAccountEntity>, crate::Error> {
        let a = MultisigAccountDaoV1::find_doing_account(chain_code, address, pool.as_ref()).await?;
        Ok(a)
    }

    pub async fn logic_delete(&self, id: &str) -> Result<(), crate::Error> {
        MultisigAccountDaoV1::logic_del_multisig_account(id, self.pool.as_ref()).await?;
        Ok(())
    }

    pub async fn pending_handle(
        pool: &CoreDbPool,
        status: MultisigAccountStatus,
    ) -> Result<Vec<MultisigAccountEntity>, crate::Error> {
        Ok(MultisigAccountDaoV1::pending_handle(pool.as_ref(), status).await?)
    }
}
