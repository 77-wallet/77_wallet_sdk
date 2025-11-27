use std::collections::HashSet;

use wallet_database::{
    entities::task_queue::{KnownTaskName, TaskName, TaskQueueEntity},
    repositories::{api_wallet::account::ApiAccountRepo, task_queue::TaskQueueRepo},
};
use wallet_transport_backend::request::api_wallet::{
    address::ExpandAddressCompleteReq, msg::MsgAckReq,
};

use crate::{
    domain::api_wallet::{account::ApiAccountDomain, wallet::ApiWalletDomain},
    infrastructure::task_queue::mqtt_api::ApiMqttStruct,
};
use once_cell::sync::Lazy;
use tokio::sync::Mutex;

pub(crate) static EXPAND_INDEX_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExpandStatus {
    pub uid: String,
    pub chain_code: String,
    pub needed_indices: HashSet<i32>,
    pub created_indices: HashSet<i32>,
    pub completed_indices: HashSet<i32>,
    /// 已完成
    pub status: bool,
    /// 扩容数量
    pub number: u32,
    pub serial_no: String,
    pub batch_id: String,
    /// 是否已调用expand_address_complete接口通知后端
    pub notified_complete: bool,
}

impl ExpandStatus {
    pub fn new(
        uid: &str,
        chain_code: &str,
        needed_indices: &[i32],
        completed_indices: HashSet<i32>,
        status: bool,
        number: u32,
        serial_no: &str,
        batch_id: &str,
    ) -> Self {
        let needed_indices_set: HashSet<i32> = needed_indices.into_iter().cloned().collect();
        Self {
            uid: uid.to_string(),
            chain_code: chain_code.to_string(),
            needed_indices: needed_indices_set.clone(),
            created_indices: HashSet::new(),
            completed_indices,
            status,
            number,
            serial_no: serial_no.to_string(),
            batch_id: batch_id.to_string(),
            notified_complete: false,
        }
    }

    pub(crate) fn symmetric_diff(&self) -> HashSet<i32> {
        self.needed_indices.symmetric_difference(&self.completed_indices).cloned().collect()
    }

    pub(crate) async fn load_or_fix_remark(
        task: &TaskQueueEntity,
    ) -> Result<ExpandStatus, crate::error::service::ServiceError> {
        tracing::info!(task_id=%task.id, "开始加载或修复任务备注");
        // 从 request_body 中获取消息信息
        let msg: ApiMqttStruct = wallet_utils::serde_func::serde_from_str(&task.request_body)?;
        let msg = wallet_utils::serde_func::serde_from_value::<AwmCmdAddrExpandMsg>(msg.data)?;
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        // let api_wallet = ApiWalletRepo::find_by_uid(&pool, &msg.uid).await?.ok_or(
        //     crate::error::business::BusinessError::ApiWallet(
        //         crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
        //     ),
        // )?;

        // 尝试从remark中解析，但总是验证索引的可用性和唯一性
        if let Some(r) = &task.remark {
            match wallet_utils::serde_func::serde_from_str::<ExpandStatus>(r) {
                Ok(rem) => {
                    // 验证needed_indices中的索引是否都可用
                    let existing_indices =
                        ApiAccountRepo::get_all_account_indices(&pool, &msg.uid, &msg.chain_code)
                            .await?;

                    // 获取所有正在进行的任务中的索引，按链代码过滤
                    let tasks = TaskQueueRepo::list_tasks_with_task_name(
                        &pool,
                        TaskName::Known(KnownTaskName::AwmCmdAddrExpand),
                        &[],
                    )
                    .await?;

                    let mut all_used_indices = std::collections::HashSet::new();
                    // 添加已存在的索引
                    for idx in existing_indices {
                        all_used_indices.insert(idx);
                    }

                    // 添加其他任务中的索引（排除当前任务）
                    for t in &tasks {
                        if t.id != task.id {
                            if let Some(remark) = &t.remark {
                                if let Ok(other_rem) =
                                    wallet_utils::serde_func::serde_from_str::<ExpandStatus>(remark)
                                {
                                    // 只考虑相同链的任务
                                    if other_rem.chain_code == msg.chain_code
                                        && other_rem.uid == msg.uid
                                    {
                                        for &idx in &other_rem.needed_indices {
                                            let account_id = wallet_utils::address::AccountIndexMap::from_input_index(idx)?;
                                            all_used_indices.insert(account_id.account_id);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // 重要：保持任务的原始needed_indices不变，不重新计算
                    // 即使某些索引可能已被使用，也应该让任务继续处理这些索引
                    // 系统会在实际创建时检测并跳过已存在的索引
                    tracing::info!("保持任务原始needed_indices不变，确保任务职责范围稳定");

                    return Ok(rem);
                }
                Err(_) => {
                    tracing::warn!("remark 解析失败，自动修复");
                }
            }
        } else {
            tracing::warn!("remark 为空，自动修复");
        }

        // 重新计算needed_indices
        // 移除了重新计算needed_indices的逻辑
        // 重要：保持任务的原始needed_indices不变，不重新计算
        // 即使某些索引可能已被使用，也应该让任务继续处理这些索引
        // 系统会在实际创建时检测并跳过已存在的索引
        let mut needed_indices = AwmCmdAddrExpandMsg::get_needed_indices(
            &msg.typ,
            &msg.chain_code,
            msg.number,
            msg.index,
            &msg.uid,
            Some(task.id.as_str()),
        )
        .await?;

        // 对索引进行排序，确保按升序显示
        needed_indices.sort();

        let result = ExpandStatus::new(
            &msg.uid,
            &msg.chain_code,
            &needed_indices,
            Default::default(),
            false,
            msg.number,
            &msg.serial_no,
            &msg.batch_id,
        );

        tracing::info!(task_id=%task.id, uid=%result.uid, chain_code=%result.chain_code, needed_count=%result.needed_indices.len(), "任务备注加载或修复完成");
        Ok(result)
    }

    pub(crate) async fn sync_completed_from_db(
        &mut self,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        // 查询所有已存在的账户索引，不只是is_init=1的
        let all_accounts =
            ApiAccountRepo::get_all_account_indices(&pool, &self.uid, &self.chain_code).await?;

        // 将account_id转换为input_index
        let mut db_existing = HashSet::new();
        for account_id in all_accounts {
            let input_index =
                wallet_utils::address::AccountIndexMap::from_account_id(account_id)?.input_index;
            db_existing.insert(input_index);
        }

        // 查找已完成的索引
        self.completed_indices = self.needed_indices.intersection(&db_existing).copied().collect();

        // 记录调试日志
        tracing::info!(
            "sync_completed_from_db: needed_indices={:?}, db_existing={:?}, completed_indices={:?}",
            self.needed_indices,
            db_existing,
            self.completed_indices
        );

        // 更新状态
        self.status = self.needed_indices.iter().all(|i| self.completed_indices.contains(i));

        Ok(())
    }
}

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
        // tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

        // 检查是否有其他正在执行的地址查询任务
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
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

            let task = TaskQueueRepo::task_detail(&pool, msg_id).await?;
            if let Some(task) = task {
                if task.remark.is_some() {
                    // 如果remark不为None，说明是恢复任务
                    tracing::info!(msg_id=%msg_id, "恢复扩容任务，remark存在");
                    crate::infrastructure::expand_address::submit_recover_task(
                        msg_id.to_string(),
                        self.clone(),
                    )
                    .await?;
                } else {
                    // 如果remark为None，是首次处理的新任务
                    tracing::info!(msg_id=%msg_id, "处理新扩容任务，remark不存在");
                    crate::infrastructure::expand_address::submit_expand_task(
                        msg_id.to_string(),
                        self.clone(),
                    )
                    .await?;
                }
            }

            tracing::info!(uid=%self.uid, chain_code=%self.chain_code, serial_no=%self.serial_no, msg_id=%msg_id, "地址扩容任务已成功提交给Actor管理器");
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
                let mut already_account_indices =
                    ApiAccountRepo::get_all_account_indices(&pool, uid, chain_code).await?;
                tracing::info!(uid=%uid, chain_code=%chain_code, existing_count=%already_account_indices.len(), existing_indices=?already_account_indices, "已获取现有账户索引");

                // 查询相关任务
                tracing::debug!(uid=%uid, chain_code=%chain_code, "查询进行中的扩容任务");
                let tasks = TaskQueueRepo::list_tasks_with_task_name(
                    &pool,
                    TaskName::Known(KnownTaskName::AwmCmdAddrExpand),
                    &[],
                )
                .await?;
                tracing::debug!(uid=%uid, chain_code=%chain_code, task_count=%tasks.len(), "已获取扩容任务列表");

                // 处理任务中的索引信息
                let mut additional_indices_count = 0;
                for task in tasks {
                    // 排除当前任务，避免重复计算索引
                    if let Some(current_id) = current_task_id
                        && task.id == current_id
                    {
                        tracing::debug!(uid=%uid, chain_code=%chain_code, task_id=%task.id, "排除当前任务，避免重复计算");
                        continue;
                    }

                    if let Some(remark) = task.remark {
                        tracing::debug!(uid=%uid, chain_code=%chain_code, task_id=%task.id, "解析任务备注信息");
                        let remark =
                            wallet_utils::serde_func::serde_from_str::<ExpandStatus>(&remark)?;

                        // 只考虑相同用户和相同链的任务
                        if remark.chain_code == chain_code && remark.uid == uid {
                            tracing::info!(uid=%uid, chain_code=%chain_code, task_id=%task.id, task_needed_indices=?remark.needed_indices, "处理相同用户和链的任务索引");
                            for i in remark.needed_indices {
                                let account_id =
                                    wallet_utils::address::AccountIndexMap::from_input_index(i)?
                                        .account_id;
                                already_account_indices.push(account_id);
                                additional_indices_count += 1;
                            }
                        }
                    }
                }

                tracing::info!(uid=%uid, chain_code=%chain_code, additional_count=%additional_indices_count, total_count=%already_account_indices.len(), "已合并任务中的索引信息");

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
}
