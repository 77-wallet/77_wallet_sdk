use wallet_database::repositories::{
    api_wallet::account::ApiAccountRepo, task_queue::TaskQueueRepo,
};
use wallet_transport_backend::request::api_wallet::msg::MsgAckReq;

use crate::{
    domain::api_wallet::account::ApiAccountDomain,
    infrastructure::expand_address::facade::ExpandAddressFacade,
};
use once_cell::sync::Lazy;
use tokio::sync::Mutex;

pub(crate) static EXPAND_INDEX_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

// biz_type = AWM_CMD_ADDR_EXPAND
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AwmCmdAddrExpandMsg {
    /// 扩容类型： CHA_ALL / CHA_INDEX
    #[serde(rename = "type")]
    pub typ: AddressAllockType,
    #[serde(rename = "chain")]
    pub chain_code: String,
    pub index: Option<i32>,
    pub uid: String,
    /// 扩容编号  
    pub serial_no: String,
    /// 扩容数量（可空，CHA_BATCH 类型时有效）
    #[serde(
        deserialize_with = "wallet_utils::serde_func::string_to_u32",
        serialize_with = "wallet_utils::serde_func::u32_to_string"
    )]
    pub number: u32,
    /// 批次编号
    pub batch_id: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AddressAllockType {
    ChaBatch,
    ChaIndex,
}

// 地址池扩容
impl AwmCmdAddrExpandMsg {
    pub(crate) async fn exec(
        &self,
        msg_id: &str,
    ) -> Result<(), crate::error::service::ServiceError> {
        tracing::info!(uid=%self.uid, chain_code=%self.chain_code, number=%self.number, index=?self.index, batch_id=%self.batch_id, msg_id=%msg_id, "开始处理地址扩容请求");

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 1️⃣ 并发地址查询保护（保留）
        let tasks = TaskQueueRepo::get_tasks_with_request_body(
            &pool,
            wallet_transport_backend::consts::endpoint::api_wallet::QUERY_ADDRESS_LIST,
            &[0, 1, 3],
        )
        .await?;

        tracing::debug!(uid=%self.uid, task_count=%tasks.len(), "检查并发任务状态");
        if !tasks.is_empty() {
            tracing::warn!(uid=%self.uid, task_count=%tasks.len(), "有其他地址查询任务正在执行，无法进行扩容");
            return Err(crate::error::service::ServiceError::Business(
                crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::account::AccountError::CanNotExpand.into(),
                ),
            ));
        }

        // 计算需要扩容的索引
        tracing::debug!(uid=%self.uid, chain_code=%self.chain_code, "开始计算需要扩容的索引");
        let needed_indices = AwmCmdAddrExpandMsg::get_needed_indices(
            &self.typ,
            &self.chain_code,
            self.number,
            self.index,
            &self.uid,
            Some(msg_id),
        )
        .await?;
        tracing::info!(uid=%self.uid, chain_code=%self.chain_code, needed_count=%needed_indices.len(), needed_indices=?needed_indices, "计算完成，需要扩容的索引数量");

        // 确认消息
        tracing::debug!(msg_id=%msg_id, "确认收到消息");
        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let mut msg_ack_req = MsgAckReq::default();
        msg_ack_req.push(msg_id);
        backend.msg_ack(msg_ack_req).await?;
        tracing::debug!(msg_id=%msg_id, "消息确认成功");

        // 提交扩容任务
        if !needed_indices.is_empty() {
            tracing::info!(msg_id=%msg_id, uid=%self.uid, chain_code=%self.chain_code, needed_count=%needed_indices.len(), "提交扩容任务给Actor管理器");

            // let task = TaskQueueRepo::task_detail(&pool, msg_id).await?;
            // if let Some(task) = task {
            //     if task.remark.is_some() {
            //         // 如果remark不为None，说明是恢复任务
            //         tracing::info!(msg_id=%msg_id, "恢复扩容任务，remark存在");
            //         crate::infrastructure::expand_address::submit_recover_task(
            //             msg_id.to_string(),
            //             self.clone(),
            //         )
            //         .await?;
            //     } else {
            //         // 如果remark为None，是首次处理的新任务
            //         tracing::info!(msg_id=%msg_id, "处理新扩容任务，remark不存在");
            //         crate::infrastructure::expand_address::submit_expand_task(
            //             msg_id.to_string(),
            //             self.clone(),
            //         )
            //         .await?;
            //     }
            // }
            ExpandAddressFacade::submit_expand_task(msg_id.to_string(), self.clone()).await?;
            tracing::info!(
                uid=%self.uid,
                chain_code=%self.chain_code,
                msg_id=%msg_id,
                "扩容任务已提交给 Actor"
            );
        } else {
            tracing::info!(uid=%self.uid, chain_code=%self.chain_code, "无需扩容，没有需要处理的索引");
        }

        Ok(())
    }

    pub(crate) async fn get_needed_indices(
        typ: &AddressAllockType,
        chain_code: &str,
        number: u32,
        index: Option<i32>,
        uid: &str,
        current_task_id: Option<&str>,
    ) -> Result<Vec<i32>, crate::error::service::ServiceError> {
        tracing::debug!(uid=%uid, chain_code=%chain_code, number=%number, index=?index, current_task_id=?current_task_id, "开始计算需要扩容的索引");

        // 使用锁来防止并发问题
        tracing::info!(uid=%uid, chain_code=%chain_code, "尝试获取扩容索引计算锁");
        let _guard = EXPAND_INDEX_LOCK.lock().await;
        tracing::info!(uid=%uid, chain_code=%chain_code, "已获取扩容索引计算锁");

        let needed_indices = match typ {
            AddressAllockType::ChaBatch => {
                tracing::debug!(uid=%uid, chain_code=%chain_code, "处理批量扩容类型");
                let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

                // 查询已有的账户
                tracing::debug!(uid=%uid, chain_code=%chain_code, "查询数据库中的现有账户索引");
                let already_account_indices =
                    ApiAccountRepo::get_all_account_indices(pool.clone(), uid, chain_code).await?;
                tracing::info!(uid=%uid, chain_code=%chain_code, existing_count=%already_account_indices.len(), existing_indices=?already_account_indices, "已获取现有账户索引");

                // 计算下一批索引
                tracing::debug!(uid=%uid, chain_code=%chain_code, requested_number=%number, "计算下一批需要扩容的索引");
                let next = ApiAccountDomain::next_account_indices(already_account_indices, number);
                tracing::info!(uid=%uid, chain_code=%chain_code, calculated_count=%next.len(), calculated_account_indices=?next, "已计算下一批账户索引");

                // 转换为输入索引格式
                let mut input_indices = Vec::with_capacity(next.len());
                for account_id in next {
                    let input_index =
                        wallet_utils::address::AccountIndexMap::from_account_id(account_id)?
                            .input_index;
                    input_indices.push(input_index);
                }

                tracing::info!(uid=%uid, chain_code=%chain_code, final_count=%input_indices.len(), final_indices=?input_indices, "完成索引计算，最终需要扩容的索引");
                input_indices
            }
            AddressAllockType::ChaIndex => {
                tracing::debug!(uid=%uid, chain_code=%chain_code, "处理单索引扩容类型");
                if let Some(index) = index {
                    tracing::info!(uid=%uid, chain_code=%chain_code, target_index=%index, "指定了单索引进行扩容");
                    vec![index]
                } else {
                    tracing::warn!(uid=%uid, chain_code=%chain_code, "单索引扩容但未指定索引值");
                    vec![]
                }
            }
        };

        tracing::info!(uid=%uid, chain_code=%chain_code, result_count=%needed_indices.len(), result_indices=?needed_indices, "索引计算完成");
        Ok(needed_indices)
    }
}

#[cfg(test)]
mod test {

    use crate::{
        infrastructure::task_queue::mqtt_api::ApiMqttStruct,
        messaging::mqtt::{Message, topics::api_wallet::cmd::address_allock::AwmCmdAddrExpandMsg},
        test::env::get_manager,
    };

    #[test]
    fn deserialize() {
        // 69115152444c0b49fc7b9f3c	AwmCmdAddrExpand	{"data":{"batchId":"fefsdfdsfdsfdsf","chain":"tron","index":null,"number":"3","serialNo":"tron_88a06da151b1d51c3f9e751ba398be4abb67e816359c849ef66ac0c7bbbd0640","type":"CHA_BATCH","uid":"703dc9ffe712d3ced169cee62c3c9c8118ce822bd00d49650e02df80ba0fcc30"},"eventNo":"1987712693663371264","eventType":"3","secret":"jnRkLB2TnTDOLsfqsOGsFlnMyoL4qJcKNeNuaFejctA=","sign":"rajb0qK3NJNnwfhgYvGiT1jw1nL8cREURz4M+d3QZW8fhJRVNb2YknT8qLu2jbfw3FqIrV27Nc6t7dPqz6IqDg==","time":1762742610}	2	111	3	2025-11-10T02:43:31Z	2025-11-13T05:47:42Z	Business error: api wallet error: Api Account error: Expand address not done yet
        let data = "{\"bizType\":\"AWM_CMD_ADDR_EXPAND\",\"body\":{\"data\":{\"chain\":\"tron\",\"index\":null,\"number\":\"50\",\"serialNo\":\"tron_88a06da151b1d51c3f9e751ba398be4abb67e816359c849ef66ac0c7bbbd0640\",\"type\":\"CHA_BATCH\",\"uid\":\"88a06da151b1d51c3f9e751ba398be4abb67e816359c849ef66ac0c7bbbd0640\"},\"eventNo\":\"1987712693663371264\",\"eventType\":\"3\",\"secret\":\"jnRkLB2TnTDOLsfqsOGsFlnMyoL4qJcKNeNuaFejctA=\",\"sign\":\"rajb0qK3NJNnwfhgYvGiT1jw1nL8cREURz4M+d3QZW8fhJRVNb2YknT8qLu2jbfw3FqIrV27Nc6t7dPqz6IqDg==\",\"time\":1762742610},\"clientId\":\"df1b2982f3240f55fa8769e38e747010\",\"deviceType\":\"ANDROID\",\"sn\":\"5a748300e76e023cea05523c103763a7976bdfb085c24f9713646ae2faa5949d\",\"msgId\":\"68d4fdcdab00e34b73ef17a0\"}";

        let msg: Message = serde_json::from_str(data).unwrap();
        println!("{:#?}", msg);

        let msg: ApiMqttStruct = serde_json::from_value(msg.body).unwrap();
        println!("result: {:#?}", msg);
    }

    async fn init_manager() {
        wallet_utils::init_test_log();
        // 修改返回类型为Result<(), anyhow::Error>
        let (_, _) = get_manager().await.unwrap();
    }

    #[tokio::test]
    async fn address_allock() -> anyhow::Result<()> {
        init_manager().await;

        let change = r#"
            {
                "chain": "tron",
                "index": null,
                "number": "50",
                "serialNo": "tron_88a06da151b1d51c3f9e751ba398be4abb67e816359c849ef66ac0c7bbbd0640",
                "type": "CHA_BATCH",
                "uid": "88a06da151b1d51c3f9e751ba398be4abb67e816359c849ef66ac0c7bbbd0640"
            }
        "#;
        let change = serde_json::from_str::<AwmCmdAddrExpandMsg>(&change).unwrap();
        let _res = change.exec("2").await.unwrap();

        Ok(())
    }

    fn sort_vec<T: Ord>(items: &mut Vec<T>) {
        items.sort();
    }

    #[test]
    fn test_sort_vec() {
        let mut items = vec![
            83, 80, 92, 101, 82, 90, 107, 102, 55, 50, 54, 56, 88, 87, 78, 74, 108, 100, 71, 61,
            62, 105, 60, 91, 104, 73, 67, 66, 64, 69, 70, 53, 103, 86, 77, 106, 76, 51, 94, 109,
            79, 93, 52, 57, 96, 72, 63, 65, 95, 68,
        ];

        sort_vec(&mut items);
        println!("{:#?}", items);
        let len = items.len();
        assert_eq!(len, 50);

        let mut items_2 = vec![
            65, 83, 74, 106, 82, 94, 72, 55, 69, 86, 108, 57, 105, 103, 70, 67, 87, 88, 54, 64, 95,
            80, 109, 66, 61, 102, 63, 101, 91, 100, 96, 76, 62, 77, 90, 53, 104, 107, 92, 73, 56,
            60, 68, 78, 79, 93, 71,
        ];
        sort_vec(&mut items_2);
        println!("{:#?}", items_2);
        let len = items_2.len();
        assert_eq!(len, 47);
    }
}
