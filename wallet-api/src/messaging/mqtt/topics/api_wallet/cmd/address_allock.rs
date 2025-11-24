use std::collections::HashSet;

use wallet_database::{
    entities::task_queue::{KnownTaskName, TaskName, TaskQueueEntity},
    repositories::{
        api_wallet::{account::ApiAccountRepo, wallet::ApiWalletRepo},
        task_queue::TaskQueueRepo,
    },
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
    pub completed_indices: HashSet<i32>,
    /// 已完成
    pub status: bool,
    /// 扩容数量
    pub number: u32,
    pub serial_no: String,
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
    ) -> Self {
        let needed_indices = needed_indices.into_iter().cloned().collect();
        Self {
            uid: uid.to_string(),
            chain_code: chain_code.to_string(),
            needed_indices,
            completed_indices,
            status,
            number,
            serial_no: serial_no.to_string(),
        }
    }

    pub(crate) fn symmetric_diff(&self) -> HashSet<i32> {
        self.needed_indices.symmetric_difference(&self.completed_indices).cloned().collect()
    }

    pub(crate) async fn load_or_fix_remark(
        task: &TaskQueueEntity,
    ) -> Result<ExpandStatus, crate::error::service::ServiceError> {
        // let _guard = EXPAND_INDEX_LOCK.lock().await;

        if let Some(r) = &task.remark {
            match wallet_utils::serde_func::serde_from_str::<ExpandStatus>(r) {
                Ok(rem) => return Ok(rem),
                Err(_) => {
                    tracing::warn!("remark 解析失败，自动修复");
                }
            }
        } else {
            tracing::warn!("remark 为空，自动修复");
        }

        // 从 request_body 中重新获取 needed_indices
        let msg: ApiMqttStruct = wallet_utils::serde_func::serde_from_str(&task.request_body)?;
        let msg = wallet_utils::serde_func::serde_from_value::<AwmCmdAddrExpandMsg>(msg.data)?;
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let api_wallet = ApiWalletRepo::find_by_uid(&pool, &msg.uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            ),
        )?;

        let needed_indices = AwmCmdAddrExpandMsg::get_needed_indices(
            &msg.typ,
            &msg.chain_code,
            msg.number,
            msg.index,
            &api_wallet.address,
        )
        .await?;
        Ok(ExpandStatus::new(
            &msg.uid,
            &msg.chain_code,
            &needed_indices,
            Default::default(),
            false,
            msg.number,
            &msg.serial_no,
        ))
    }

    pub(crate) async fn sync_completed_from_db(
        &mut self,
    ) -> Result<(), crate::error::service::ServiceError> {
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;

        let api_wallet = ApiWalletRepo::find_by_uid(&pool, &self.uid).await?.ok_or(
            crate::error::business::BusinessError::ApiWallet(
                crate::error::business::api_wallet::wallet::WalletError::NotFound.into(),
            ),
        )?;
        let db_inited: HashSet<i32> =
            ApiAccountRepo::list_inited_indices(&pool, &api_wallet.address)
                .await?
                .into_iter()
                .map(|index| index.0)
                .collect();

        self.completed_indices = self.needed_indices.intersection(&db_inited).copied().collect();

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
        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let tasks = TaskQueueRepo::get_tasks_with_request_body(
            &pool,
            wallet_transport_backend::consts::endpoint::api_wallet::QUERY_ADDRESS_LIST,
            &[0, 1, 3],
        )
        .await?;

        if !tasks.is_empty() {
            return Err(crate::error::service::ServiceError::Business(
                crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::account::AccountError::CanNotExpand.into(),
                ),
            ));
        }

        ApiWalletDomain::expand_address(
            msg_id,
            &self.typ,
            self.index,
            &self.uid,
            &self.chain_code,
            self.number,
            &self.serial_no,
            &self.batch_id,
        )
        .await?;

        let backend = crate::context::CONTEXT.get().unwrap().get_global_backend_api();
        let mut msg_ack_req = MsgAckReq::default();
        msg_ack_req.push(msg_id);
        backend.msg_ack(msg_ack_req).await?;

        let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
        let task = TaskQueueRepo::task_detail(&pool, msg_id).await?;
        if let Some(task) = task
            && let Some(reamrk) = task.remark
            && wallet_utils::serde_func::serde_from_str::<ExpandStatus>(&reamrk)?.status
        {
            let req = ExpandAddressCompleteReq::new(&self.uid, &self.serial_no, true, None);
            backend.expand_address_complete(req).await?;
            return Ok(());
        } else {
            tracing::warn!("address allock not done yet");
            return Err(crate::error::service::ServiceError::Business(
                crate::error::business::BusinessError::ApiWallet(
                    crate::error::business::api_wallet::account::AccountError::ExpandAddressNotDoneYet.into(),
                ),
            ));
        }
    }

    pub(crate) async fn get_needed_indices(
        typ: &AddressAllockType,
        chain_code: &str,
        number: u32,
        index: Option<i32>,
        wallet_address: &str,
    ) -> Result<Vec<i32>, crate::error::service::ServiceError> {
        let needed_indices = match typ {
            AddressAllockType::ChaBatch => {
                let pool = crate::context::CONTEXT.get().unwrap().get_global_sqlite_pool()?;
                // 查询已有的账户
                let mut already_account_indices =
                    ApiAccountRepo::get_all_account_indices(&pool, wallet_address, chain_code)
                        .await?;
                let tasks = TaskQueueRepo::list_tasks_with_task_name(
                    &pool,
                    TaskName::Known(KnownTaskName::AwmCmdAddrExpand),
                    &[],
                )
                .await?;

                for task in tasks {
                    if let Some(remark) = task.remark {
                        let remark =
                            wallet_utils::serde_func::serde_from_str::<ExpandStatus>(&remark)?;

                        if remark.chain_code == chain_code {
                            for i in remark.needed_indices {
                                already_account_indices.push(
                                    wallet_utils::address::AccountIndexMap::from_input_index(i)?
                                        .account_id,
                                );
                            }
                        }
                    }
                }

                let next = ApiAccountDomain::next_account_indices(already_account_indices, number);
                let mut input_indices = Vec::with_capacity(next.len());
                for account_id in next {
                    input_indices.push(
                        wallet_utils::address::AccountIndexMap::from_account_id(account_id)?
                            .input_index,
                    );
                }
                input_indices
            }
            AddressAllockType::ChaIndex => {
                if let Some(index) = index {
                    vec![index]
                } else {
                    vec![]
                }
            }
        };
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
