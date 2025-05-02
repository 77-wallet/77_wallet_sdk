use super::multisig_member::MultisigMemberDaoV1;
use crate::{
    entities::multisig_account::{
        MultiAccountOwner, MultisigAccountEntity, MultisigAccountPayStatus, MultisigAccountStatus,
        NewMultisigAccountEntity,
    },
    pagination::Pagination,
};
use chrono::SecondsFormat;
use sqlx::{Executor, Pool, Sqlite};
use std::sync::Arc;

pub struct MultisigAccountDaoV1;

impl MultisigAccountDaoV1 {
    pub async fn create_account_with_member(
        params: &NewMultisigAccountEntity,
        pool: Arc<Pool<Sqlite>>,
    ) -> Result<(), crate::Error> {
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;

        // 添加账户
        MultisigAccountDaoV1::insert(params, tx.as_mut()).await?;
        // 添加成员
        MultisigMemberDaoV1::batch_add(&params.member_list, tx.as_mut()).await?;

        tx.commit()
            .await
            .map_err(|e| crate::Error::Database(crate::DatabaseError::Sqlx(e)))?;
        Ok(())
    }

    /// create new multisig account
    pub async fn insert<'a, E>(
        params: &NewMultisigAccountEntity,
        exec: E,
    ) -> Result<(), crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let query = r#"
            INSERT INTO multisig_account (id,name,initiator_addr,address,address_type,authority_addr,status,pay_status,owner,chain_code,threshold,
            member_num,salt,deploy_hash,fee_hash,fee_chain,is_del,created_at, updated_at)
            VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ON CONFLICT (id)
            DO UPDATE SET
                is_del = EXCLUDED.is_del,
                status = EXCLUDED.status,
                name = EXCLUDED.name,
                address = EXCLUDED.address,
                address_type = EXCLUDED.address_type,
                deploy_hash = EXCLUDED.deploy_hash,
                fee_hash = EXCLUDED.fee_hash,
                authority_addr = EXCLUDED.authority_addr,
                fee_chain = EXCLUDED.fee_chain,
                salt = EXCLUDED.salt,
                pay_status = EXCLUDED.pay_status,
                owner = EXCLUDED.owner,
                updated_at = EXCLUDED.updated_at
        "#;

        sqlx::query(query)
            .bind(&params.id)
            .bind(&params.name)
            .bind(&params.initiator_addr)
            .bind(&params.address)
            .bind(&params.address_type)
            .bind(&params.authority_addr)
            .bind(params.status.to_i8())
            .bind(params.pay_status.to_i8())
            .bind(params.owner.to_i8())
            .bind(&params.chain_code)
            .bind(params.threshold)
            .bind(params.member_num)
            .bind(&params.salt)
            .bind(&params.deploy_hash)
            .bind(&params.fee_hash)
            .bind(&params.fee_chain)
            .bind(params.is_del)
            .bind(params.create_at.to_rfc3339_opts(SecondsFormat::Secs, true))
            .execute(exec)
            .await?;

        Ok(())
    }

    pub async fn find_by_conditions<'a, E>(
        conditions: Vec<(&str, &str)>,
        exec: E,
    ) -> Result<Option<MultisigAccountEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut query = r#"SELECT * FROM multisig_account"#.to_string();
        let mut query_where = Vec::new();

        for (key, value) in conditions.iter() {
            query_where.push(format!("{} = '{}'", key, value));
        }
        if !query_where.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&query_where.join(" AND "));
        }

        query.push_str(" ORDER BY created_at DESC LIMIT 1");

        let res = sqlx::query_as::<_, MultisigAccountEntity>(&query)
            .fetch_optional(exec)
            .await?;

        Ok(res)
    }

    pub async fn update_by_id<'a, E>(
        id: &str,
        params: std::collections::HashMap<String, String>,
        exec: E,
    ) -> Result<MultisigAccountEntity, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut sql = String::from("UPDATE multisig_account SET ");

        let mut first = true;
        let mut args = Vec::new();

        for (key, value) in params.iter() {
            if !first {
                sql.push_str(", ");
            }
            sql.push_str(&format!("{} = ?", key));
            args.push(value);
            first = false;
        }

        let id = id.to_string();
        sql.push_str(" WHERE id = ? RETURNING *");
        args.push(&id);

        let mut query = sqlx::query_as::<_, MultisigAccountEntity>(&sql);
        for arg in args {
            query = query.bind(arg);
        }

        let mut res = query.fetch_all(exec).await?;
        res.pop().ok_or(crate::DatabaseError::ReturningNone)
    }

    pub async fn list<'a, E>(
        conditions: Vec<(&str, &str)>,
        exec: E,
    ) -> Result<Vec<MultisigAccountEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut query = r#"SELECT * FROM multisig_account"#.to_string();
        let mut query_where = Vec::new();

        for (key, value) in conditions.iter() {
            query_where.push(format!("{} = '{}'", key, value));
        }
        if !query_where.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&query_where.join(" AND "));
        }

        let res = sqlx::query_as::<_, MultisigAccountEntity>(&query)
            .fetch_all(exec)
            .await?;
        Ok(res)
    }

    pub async fn pending_account<'a, E>(
        pool: E,
    ) -> Result<Vec<MultisigAccountEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT * FROM multisig_account WHERE status = ? or pay_status = ?";

        let result = sqlx::query_as(sql)
            .bind(MultisigAccountStatus::OnChianPending.to_i8())
            .bind(MultisigAccountPayStatus::PaidPending.to_i8())
            .fetch_all(pool)
            .await?;

        Ok(result)
    }

    pub async fn account_count(
        chain_code: &str,
        pool: Arc<Pool<Sqlite>>,
    ) -> Result<i64, crate::DatabaseError> {
        let sql = "SELECT count(*) FROM multisig_account WHERE chain_code = ? and is_del = 0";

        let count: (i64,) = sqlx::query_as(sql)
            .bind(chain_code)
            .fetch_one(pool.as_ref())
            .await?;

        Ok(count.0)
    }

    // 这个方法修改的交易必然是部署和多签费用都是完成的
    pub async fn update_multisig_address<'a, E>(
        account_id: &str,
        address: &str,
        salt: &str,
        authority_addr: &str,
        address_type: &str,
        deploy_hash: &str,
        fee_hash: &str,
        fee_chain: Option<String>,
        exec: E,
    ) -> Result<(), crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut sql = r#"
        UPDATE multisig_account 
        SET address = $2, salt = $3, authority_addr = $4, address_type = $5, status = 3, pay_status = 1,
        updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
        deploy_hash = case when deploy_hash != $6 then $6 else deploy_hash end,
        fee_hash = case when fee_hash != $7 then $7 else fee_hash end"#.to_string();

        if fee_chain.is_some() {
            sql.push_str(", fee_chain = $8");
        }

        sql.push_str(" WHERE id = $1");

        let mut query = sqlx::query(&sql)
            .bind(account_id)
            .bind(address)
            .bind(salt)
            .bind(authority_addr)
            .bind(address_type)
            .bind(deploy_hash)
            .bind(fee_hash);

        if let Some(fee_chain_value) = fee_chain {
            query = query.bind(fee_chain_value);
        }

        query.execute(exec).await?;

        Ok(())
    }

    pub async fn account_list(
        owner: bool,
        chain_code: Option<&str>,
        pool: Arc<Pool<Sqlite>>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<MultisigAccountEntity>, crate::DatabaseError> {
        let mut sql = String::from("SELECT * FROM multisig_account where is_del = 0 and ");
        let mut conditions = Vec::new();

        if owner {
            conditions.push("owner <> 0".to_string());
        } else {
            conditions.push("owner <> 1".to_string());
        }

        if let Some(chain) = chain_code {
            let sql = format!("chain_code = '{}'", chain);
            conditions.push(sql);
        }

        if !conditions.is_empty() {
            sql.push_str(&conditions.join(" AND "));
        }
        sql.push_str(" ORDER BY created_at DESC");

        let pagination = Pagination::<MultisigAccountEntity>::init(page, page_size);

        pagination.page(pool.as_ref(), &sql).await
    }

    pub async fn update_name<'a, E>(
        id: &str,
        name: &str,
        exec: E,
    ) -> Result<(), crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "UPDATE multisig_account SET name = $1 WHERE id = $2".to_string();
        sqlx::query(&sql).bind(name).bind(id).execute(exec).await?;
        Ok(())
    }

    pub async fn sync_status<'a, E>(
        account_id: &str,
        status: MultisigAccountStatus,
        exec: E,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "UPDATE multisig_account SET status = $1  WHERE id = $2".to_string();
        sqlx::query(&sql)
            .bind(status.to_i8())
            .bind(account_id)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }

    // make sure is_del = 0
    pub async fn find_by_address<'a, E>(
        address: &str,
        exec: E,
    ) -> Result<Option<MultisigAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let query = r#"SELECT * FROM multisig_account WHERE address = ? and is_del = 0"#;

        let rec = sqlx::query_as::<_, MultisigAccountEntity>(query)
            .bind(address)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(rec)
    }

    pub async fn find_by_id<'a, E>(
        id: &str,
        exec: E,
    ) -> Result<Option<MultisigAccountEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let query = r#"SELECT * FROM multisig_account WHERE id = ? and is_del = 0"#;

        let rec = sqlx::query_as::<_, MultisigAccountEntity>(query)
            .bind(id)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(rec)
    }

    pub async fn update_status<'a, E>(
        id: &str,
        status: Option<i8>,
        pay_status: Option<i8>,
        exec: E,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let mut sql =
            r#"UPDATE multisig_account SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')"#
                .to_string();
        let mut conditions = Vec::new();

        if status.is_some() {
            conditions.push("status = ?");
        }
        if pay_status.is_some() {
            conditions.push("pay_status = ?");
        }

        if !conditions.is_empty() {
            sql.push_str(", ");
            sql.push_str(&conditions.join(", "));
        }
        sql.push_str(" WHERE id = ?");

        let mut rs = sqlx::query(&sql);

        if let Some(status) = status {
            rs = rs.bind(status);
        }
        if let Some(pay_status) = pay_status {
            rs = rs.bind(pay_status);
        }

        rs.bind(id)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }

    pub async fn find_doing_account<'a, E>(
        chain_code: &str,
        address: &str,
        exec: E,
    ) -> Result<Option<MultisigAccountEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "select * from multisig_account where initiator_addr = ? and chain_code = ? and is_del = 0 and status in (?,?) limit 1";

        let result = sqlx::query_as(sql)
            .bind(address)
            .bind(chain_code)
            .bind(MultisigAccountStatus::Confirmed.to_i8())
            .bind(MultisigAccountStatus::Pending.to_i8())
            .fetch_optional(exec)
            .await?;

        Ok(result)
    }

    pub async fn find_done_account<'a, E>(
        address: &str,
        chain_code: &str,
        exec: E,
    ) -> Result<Option<MultisigAccountEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "select * from multisig_account where address = ? and chain_code = ? and is_del = 0 and status = ? and pay_status = ? limit 1";

        let result = sqlx::query_as(sql)
            .bind(address)
            .bind(chain_code)
            .bind(MultisigAccountStatus::OnChain.to_i8())
            .bind(MultisigAccountPayStatus::Paid.to_i8())
            .fetch_optional(exec)
            .await?;

        Ok(result)
    }

    pub async fn find_owner_on_chain_account<'a, E>(
        exec: E,
    ) -> Result<Vec<MultisigAccountEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "select * from multisig_account where 
            is_del = 0 and status = ? and pay_status = 1 and owner in (1, 2)";

        let result = sqlx::query_as(sql)
            .bind(MultisigAccountStatus::OnChain.to_i8())
            .fetch_all(exec)
            .await?;
        Ok(result)
    }

    pub async fn logic_del_multisig_account<'a, E>(
        id: &str,
        exec: E,
    ) -> Result<(), crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "UPDATE multisig_account SET is_del = 1 WHERE id = $1".to_string();
        sqlx::query(&sql).bind(id).execute(exec).await?;

        Ok(())
    }

    pub async fn delete_in_status<'a, E>(id: &str, exec: E) -> Result<(), crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "UPDATE multisig_account SET is_del = 1 WHERE id = $1 and pay_status in ($2,$3) and status not in ($4,$5)"
            .to_string();
        sqlx::query(&sql)
            .bind(id)
            .bind(MultisigAccountPayStatus::Unpaid.to_i8())
            .bind(MultisigAccountPayStatus::PaidFail.to_i8())
            .bind(MultisigAccountStatus::OnChain.to_i8())
            .bind(MultisigAccountStatus::OnChianPending.to_i8())
            .execute(exec)
            .await?;

        Ok(())
    }

    pub async fn physical_del_multisig_account<'a, E>(
        id: &str,
        exec: E,
    ) -> Result<Vec<MultisigAccountEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "DELETE FROM multisig_account WHERE id = $1 RETURNING *".to_string();

        Ok(sqlx::query_as::<sqlx::Sqlite, MultisigAccountEntity>(&sql)
            .bind(id)
            .fetch_all(exec)
            .await?)
    }

    pub async fn physical_del_multi_multisig_account<'a, E>(
        exec: E,
        ids: &[&str],
    ) -> Result<Vec<MultisigAccountEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = if ids.is_empty() {
            "DELETE FROM multisig_account RETURNING *".to_string()
        } else {
            let ids = crate::any_in_collection(ids, "','");
            format!(
                "DELETE FROM multisig_account WHERE id IN ('{}') RETURNING *",
                ids
            )
        };

        Ok(sqlx::query_as::<sqlx::Sqlite, MultisigAccountEntity>(&sql)
            .fetch_all(exec)
            .await?)
    }

    pub async fn pending_handle<'a, E>(
        exec: E,
        status: MultisigAccountStatus,
    ) -> Result<Vec<MultisigAccountEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = if status == MultisigAccountStatus::Confirmed {
            format!(
                "SELECT * FROM multisig_account WHERE status = {} and owner != {} and is_del = 0",
                status.to_i8(),
                MultiAccountOwner::Participant.to_i8()
            )
        } else {
            format!(
                "SELECT * FROM multisig_account a WHERE a.status = {}  AND a.owner = {} AND a.is_del = 0 
            AND EXISTS (SELECT 1 FROM multisig_member m WHERE m.account_id = a.id AND m.confirmed = 0 AND m.is_self = 1)",
                status.to_i8(),
                MultiAccountOwner::Participant.to_i8()
            )
        };

        let res = sqlx::query_as::<_, MultisigAccountEntity>(&sql)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::DatabaseError::Sqlx(e))?;

        Ok(res)
    }
}
