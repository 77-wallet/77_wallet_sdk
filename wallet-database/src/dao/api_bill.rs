use crate::entities;
use crate::entities::api_bill::ApiBillUpdateEntity;
use crate::{
    any_in_collection,
    entities::api_bill::{ApiBillEntity, ApiBillStatus, ApiRecentBillListVo},
    pagination::Pagination,
};
use chrono::Utc;
use sqlx::{Executor, Sqlite};
use std::collections::HashSet;

pub(crate) struct ApiBillDao;

impl ApiBillDao {
    pub async fn page<'a, E>(
        exec: E,
        sql: &str,
        page_size: i64,
        page: i64,
    ) -> Result<(i64, Vec<ApiBillEntity>), crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + Send + Copy,
    {
        let count_sql = "SELECT count(*) FROM";
        let start = sql.find("FROM").unwrap_or(0) + 4;
        let count_sql1 = format!("{} {}", count_sql, &sql[start..]);

        let count = sqlx::query_scalar::<_, i64>(&count_sql1)
            .fetch_one(exec.clone())
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        let sql1 = format!("{} LIMIT {} OFFSET {}", sql, page_size, page * page_size);

        let data = sqlx::query_as::<_, ApiBillEntity>(&sql1)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok((count, data))
    }

    pub async fn get_one_by_hash<'a, E>(
        hash: &str,
        exec: E,
    ) -> Result<Option<ApiBillEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "select * from bill where hash = $1";

        sqlx::query_as::<_, ApiBillEntity>(sql)
            .bind(hash)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn get_by_hash_and_type<'a, E>(
        executor: E,
        hash: &str,
        transfer_type: i64,
    ) -> Result<Option<ApiBillEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "select * from bill where hash = $1 and transfer_type = $2";

        sqlx::query_as::<_, ApiBillEntity>(sql)
            .bind(hash)
            .bind(transfer_type)
            .fetch_optional(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn get_by_hash_and_owner<'a, E>(
        executor: E,
        hash: &str,
        owner: &str,
    ) -> Result<Option<ApiBillEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "select * from bill where hash = $1 and owner = $2";

        sqlx::query_as::<_, ApiBillEntity>(sql)
            .bind(hash)
            .bind(owner)
            .fetch_optional(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn find_by_id<'a, E>(
        executor: E,
        id: &str,
    ) -> Result<Option<ApiBillEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "select * from bill where id = ?";

        sqlx::query_as::<_, ApiBillEntity>(sql)
            .bind(id)
            .fetch_optional(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    // 查询某种类型的最后一笔交易
    pub async fn last_kind_bill<'a, E>(
        exec: E,
        owner_address: &str,
        bill_kind: Vec<i8>,
    ) -> Result<Option<ApiBillEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let kinds = crate::any_in_collection(bill_kind, "','");
        let sql = format!(
            "select * from bill where owner = '{}' and tx_kind in ('{}') ORDER BY datetime(transaction_time, 'unixepoch') DESC limit 1",
            owner_address, kinds
        );
        sqlx::query_as::<_, ApiBillEntity>(&sql)
            .fetch_optional(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    pub async fn lists_by_hashs<'a, E>(
        pool: E,
        owner: &str,
        hashs: Vec<String>,
    ) -> Result<Vec<ApiBillEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let hashs_str = any_in_collection(hashs, "','");

        let sql = format!(
            "select * from bill where owner = '{}' and hash in ('{}')",
            owner, hashs_str
        );

        let res = sqlx::query_as::<_, ApiBillEntity>(&sql)
            .fetch_all(pool)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res)
    }

    pub async fn bill_count<'a, E>(pool: E) -> Result<i64, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "SELECT COUNT(*) FROM bill";

        let res = sqlx::query_scalar::<_, i64>(sql)
            .fetch_one(pool)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(res)
    }

    pub async fn bill_lists<'a, E>(
        pool: E,
        addr: &[String],
        chain_code: Option<&str>,
        symbol: Option<&str>,
        is_multisig: Option<i64>,
        min_value: Option<f64>,
        start: Option<i64>,
        end: Option<i64>,
        transfer_type: Vec<i32>,
        page: i64,
        page_size: i64,
    ) -> Result<Pagination<ApiBillEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite> + Copy,
    {
        let mut sql = String::from("SELECT * FROM bill WHERE owner IN (");
        let placeholders: Vec<String> = addr.iter().map(|item| format!("'{}'", item)).collect();
        sql.push_str(&placeholders.join(","));
        sql.push(')');

        if !transfer_type.is_empty() {
            let kinds_str = any_in_collection(transfer_type, "','");
            sql.push_str(format!(" AND tx_kind  in ('{}')", kinds_str).as_str());
        }

        if let Some(chain_code) = chain_code {
            sql.push_str(format!(" AND chain_code = '{}'", chain_code).as_str());
        }
        if let Some(is_multisig) = is_multisig {
            sql.push_str(format!(" AND is_multisig = '{}'", is_multisig).as_str());
        }

        if let Some(symbol) = symbol {
            sql.push_str(format!(" AND symbol = '{}'", symbol).as_str());
        }
        if let Some(start) = start {
            sql.push_str(format!(" AND transaction_time >= '{}'", start).as_str());
        }
        if let Some(end) = end {
            sql.push_str(format!(" AND transaction_time <= '{}'", end).as_str());
        }

        if let Some(min_value) = min_value {
            sql.push_str(format!(" AND CAST(value as REAL) > {}", min_value).as_str());
        }

        sql.push_str(" ORDER BY datetime(transaction_time, 'unixepoch') DESC");

        let res = ApiBillDao::page(pool, &sql, page, page_size).await?;
        Ok(Pagination {
            page,
            page_size,
            total_count: res.0,
            data: res.1,
        })
    }

    // 最近转列
    pub async fn recent_bill<'a, E>(
        symbol: &str,
        addr: &str,
        chain_code: &str,
        min_value: Option<f64>,
        page: i64,
        page_size: i64,
        pool: &E,
    ) -> Result<Pagination<ApiRecentBillListVo>, crate::Error>
    where
        for<'c> &'c E: sqlx::Executor<'c, Database = sqlx::Sqlite>,
    {
        let min_value_condition = if let Some(value) = min_value {
            format!("AND CAST(value as REAL) >= {}", value)
        } else {
            String::new()
        };

        let sql = format!(
            r#"SELECT b.* FROM bill b
            INNER JOIN (
                SELECT from_addr, to_addr, MAX(transaction_time) AS max_time
                FROM bill
                WHERE owner = '{}'
                AND chain_code = '{}'
                AND to_addr <> '{}'
                AND to_addr <>  ""
                AND symbol = '{}'
                {}
                AND transfer_type = 1
                GROUP BY to_addr
            ) latest
            ON b.from_addr = latest.from_addr
            AND b.to_addr = latest.to_addr
            AND b.transaction_time = latest.max_time
            AND b.transfer_type = 1
            ORDER BY b.transaction_time DESC
            "#,
            addr, chain_code, addr, symbol, min_value_condition
        );

        let count_sql = format!(r#" SELECT count(*) FROM ({}) AS subquery;"#, sql);

        // 查询1000条数据作为过滤重复的数据
        let mut paginate = Pagination::<ApiBillEntity>::init(0, 1000);
        paginate.total_count = paginate.group_count(&count_sql, pool).await?;
        paginate.data = paginate.data(&sql, pool).await?;

        let res = paginate
            .data
            .iter()
            .map(ApiRecentBillListVo::from)
            .collect::<Vec<ApiRecentBillListVo>>();

        let mut unique = HashSet::new();
        let mut result = vec![];
        for r in res {
            if !unique.contains(&r.address) {
                unique.insert(r.address.clone());
                result.push(r);
            }
        }
        let total_count = result.len() as i64;
        let start = page * page_size;
        let end = ((page + 1) * page_size).min(total_count);

        let res = Pagination {
            page: paginate.page,
            page_size: paginate.page_size,
            total_count,
            data: result[start as usize..end as usize].to_vec(),
        };
        Ok(res)
    }

    /// Fetches all bill details from the database with the given status.
    /// A `Vec` of `BillDetail` instances representing the fetched bill details.
    pub async fn fetch_all_by_status<'a, E>(
        executor: E,
        status: i8,
    ) -> Result<Vec<ApiBillEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "select * from bill where status = $1";

        sqlx::query_as::<_, ApiBillEntity>(sql)
            .bind(status)
            .fetch_all(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))
    }

    // 包括需要更新交易的hash(ton链的in_msg 字段处理)
    pub async fn update_all<'a, E>(
        executor: E,
        tx: ApiBillEntity,
        id: i32,
    ) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let extra_str = tx.extra;
        let time = sqlx::types::chrono::Utc::now().timestamp();

        let mut sql = String::from(
            r#"
        UPDATE bill SET
            transaction_fee = $2,
            transaction_time = $3,
            status = $4,
            block_height = $5,
            updated_at = $6,
            resource_consume = $7,
            hash = $8
        "#,
        );

        // 判断 extra 是否为空，决定是否追加字段
        if !extra_str.is_empty() {
            sql.push_str(", extra = $9");
        }

        sql.push_str(" WHERE id = $1 RETURNING *");

        let mut query = sqlx::query(&sql)
            .bind(id)
            .bind(tx.transaction_fee.clone())
            .bind(tx.transaction_time.to_string())
            .bind(tx.status)
            .bind(tx.block_height.to_string())
            .bind(time)
            .bind(tx.resource_consume.clone())
            .bind(tx.hash);

        if !extra_str.is_empty() {
            query = query.bind(extra_str);
        }

        query
            .execute(executor)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(())
    }

    /// Creates a new bill record in the database.
    pub async fn create<'a, E>(tx: ApiBillEntity, exec: E) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let owner = tx.owner;
        let time = Utc::now().timestamp();
        let signer = tx.signer;
        let (symbol, to) = (tx.symbol, tx.to_addr);
        let transaction_time = if tx.transaction_time.timestamp() == 0 {
            time
        } else {
            tx.transaction_time.timestamp()
        };
        let token = tx.token.clone().unwrap_or_default();
        let multisig_tx = tx.is_multisig;
        let extra = tx.extra;
        let values = format!(
            "('{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}')",
            tx.hash,
            tx.chain_code,
            symbol,
            tx.transfer_type,
            tx.tx_kind.to_i8(),
            owner,
            tx.from_addr,
            to,
            token,
            tx.value,
            tx.transaction_fee,
            tx.resource_consume,
            transaction_time,
            tx.status,
            multisig_tx,
            tx.block_height,
            tx.queue_id,
            tx.notes,
            time,
            time,
            signer,
            extra
        );

        let sql = format!(
                "insert into bill
                (
                    hash,chain_code,symbol,transfer_type,tx_kind,owner,from_addr,to_addr,token,value,
                    transaction_fee,resource_consume,transaction_time,status,is_multisig,block_height,queue_id,notes,
                    created_at,updated_at,signer,extra
                )
                    values {}
                    on conflict (hash,transfer_type,owner)
                    do update set
                        status = excluded.status,
                        resource_consume =excluded.resource_consume,
                        block_height = excluded.block_height,
                        transaction_time = excluded.transaction_time,
                        transaction_fee = excluded.transaction_fee,
                        symbol =
                            CASE WHEN bill.symbol = '' THEN excluded.symbol ELSE bill.symbol END,
                        to_addr =
                            CASE WHEN bill.to_addr = '' THEN excluded.to_addr ELSE bill.to_addr END,
                        value =
                            CASE WHEN bill.value = '0' THEN excluded.value ELSE bill.value END,
                        extra =
                            CASE WHEN excluded.extra != '' THEN excluded.extra ELSE bill.extra END,
                        updated_at = EXCLUDED.updated_at;
                ",
                values
            );

        sqlx::query(&sql)
            .execute(exec)
            .await
            .map(|_| ())
            .map_err(|e| crate::Error::Database(e.into()))
    }

    // 修改交易
    pub async fn update<'a, E>(
        transaction: &ApiBillUpdateEntity,
        tx: E,
    ) -> Result<Option<ApiBillEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"
            update bill set
                transaction_fee = $2,
                transaction_time = $3,
                status = $4,
                block_height = $5,
                updated_at = $6,
                resource_consume = $7
            where hash = $1
            RETURNING *
            "#;

        let time = sqlx::types::chrono::Utc::now().timestamp();
        let mut res = sqlx::query_as::<_, ApiBillEntity>(sql)
            .bind(transaction.hash.clone())
            .bind(transaction.format_fee.clone())
            .bind(transaction.transaction_time.to_string())
            .bind(transaction.status)
            .bind(transaction.block_height.to_string())
            .bind(time)
            .bind(transaction.resource_consume.clone())
            .fetch_all(tx)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(res.pop())
    }

    // if transaction not on chain update status false
    pub async fn update_fail<'a, E>(tx_hash: &str, exec: E) -> Result<(), crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = r#"update bill set status = $2,updated_at = $3 where hash = $1 RETURNING *"#;
        let time = sqlx::types::chrono::Utc::now().timestamp();

        sqlx::query(sql)
            .bind(tx_hash)
            .bind(ApiBillStatus::Failed.to_i8())
            .bind(time)
            .execute(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;
        Ok(())
    }

    pub async fn on_going_bill<'a, E>(
        chain_code: &str,
        address: &str,
        exec: E,
    ) -> Result<Vec<ApiBillEntity>, crate::Error>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        // 一个 半小时
        let time = wallet_utils::time::now().timestamp() - (90 * 60);

        let sql = "select * from bill where owner = $1 and status = $2 and chain_code = $3 and created_at > $4";

        let rs = sqlx::query_as::<_, ApiBillEntity>(sql)
            .bind(address)
            .bind(ApiBillStatus::Pending.to_i8())
            .bind(chain_code)
            .bind(time)
            .fetch_all(exec)
            .await
            .map_err(|e| crate::Error::Database(e.into()))?;

        Ok(rs)
    }

    pub async fn last_bill<'a, E>(
        chain_code: &str,
        address: &str,
        exec: E,
    ) -> Result<Option<ApiBillEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "select * from bill where owner = $1 and chain_code = $2 order by block_height desc limit 1";

        let rs = sqlx::query_as::<_, ApiBillEntity>(sql)
            .bind(address)
            .bind(chain_code)
            .fetch_optional(exec)
            .await?;
        Ok(rs)
    }

    pub async fn first_transfer<'a, E>(
        address: &str,
        chain_code: &str,
        exec: E,
    ) -> Result<Option<ApiBillEntity>, crate::DatabaseError>
    where
        E: Executor<'a, Database = Sqlite>,
    {
        let sql = "select * from bill where to_addr = ? and chain_code = ?";

        let rs = sqlx::query_as::<_, ApiBillEntity>(sql)
            .bind(address)
            .bind(chain_code)
            .fetch_optional(exec)
            .await?;
        Ok(rs)
    }
}
