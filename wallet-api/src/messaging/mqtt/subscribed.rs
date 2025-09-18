use crate::infrastructure::mqtt::init::MQTT_PROCESSOR;

#[derive(Debug, Clone)]
pub struct TopicData {
    pub qos: rumqttc::v5::mqttbytes::QoS,
    pub last_updated: std::time::SystemTime,
    #[allow(dead_code)]
    pub is_active: bool,
}

// 用于排序的结构体，包含 topic 和 last_updated
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct TopicEntry {
    topic: String,
    last_updated: std::time::SystemTime,
}

// 实现 TopicEntry 的排序逻辑
impl Ord for TopicEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.last_updated.cmp(&other.last_updated)
    }
}

impl PartialOrd for TopicEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Topics {
    pub(crate) data: std::collections::HashMap<String, TopicData>,
    pub(crate) entry: std::collections::BTreeSet<TopicEntry>,
}

impl Topics {
    pub fn new() -> Self {
        Topics::default()
    }

    pub async fn subscribe(
        &mut self,
        topics: Vec<String>,
        qos: Option<u8>,
    ) -> Result<(), crate::error::service::ServiceError> {
        let qos = match qos {
            Some(0) => rumqttc::v5::mqttbytes::QoS::AtMostOnce,
            Some(1) => rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
            Some(2) => rumqttc::v5::mqttbytes::QoS::ExactlyOnce,
            _ => rumqttc::v5::mqttbytes::QoS::AtMostOnce,
        };

        // 获取全局 topics
        let subscribed_topics: std::collections::HashSet<String> =
            self.data.keys().cloned().collect();

        // 过滤出未订阅的主题
        let unique_topics: Vec<String> =
            topics.into_iter().filter(|topic| !subscribed_topics.contains(topic)).collect();

        let mqtt_processor =
            MQTT_PROCESSOR.get().ok_or(crate::error::service::ServiceError::System(
                crate::error::system::SystemError::MqttClientNotInit,
            ))?;

        // let filters: Vec<rumqttc::v5::mqttbytes::v5::Filter> = unique_topics
        //     .iter()
        //     .map(|topic| rumqttc::v5::mqttbytes::v5::Filter::new(topic, qos))
        //     .collect();
        // match mqtt_processor.client().subscribe_many(filters).await {
        //     Ok(_) => {
        //         tracing::info!("订阅主题成功: {:?}", unique_topics);
        //         let now = std::time::SystemTime::now();
        //         // 插入新的订阅数据到 HashMap
        //         for topic in unique_topics {
        //             self.data.insert(
        //                 topic.clone(),
        //                 TopicData {
        //                     qos,
        //                     last_updated: now,
        //                     is_active: true,
        //                 },
        //             );

        //             // 更新 BTreeSet，进行排序
        //             self.entry.insert(TopicEntry {
        //                 topic: topic.clone(),
        //                 last_updated: now,
        //             });
        //         }
        //     }
        //     Err(e) => {
        //         tracing::error!("订阅主题失败: {:?}, 错误信息：{:?}", unique_topics, e);
        //     }
        // }
        for topic in unique_topics.iter() {
            match mqtt_processor.client().try_subscribe(topic, qos) {
                Ok(_) => {
                    tracing::debug!("订阅主题成功: {}", topic);
                    let now = std::time::SystemTime::now();
                    // 插入新的订阅数据到 HashMap
                    self.data.insert(
                        topic.clone(),
                        TopicData { qos, last_updated: now, is_active: true },
                    );

                    // 更新 BTreeSet，进行排序
                    self.entry.insert(TopicEntry { topic: topic.clone(), last_updated: now });
                }
                Err(e) => {
                    tracing::error!("订阅主题失败: {}, 错误信息：{:?}", topic, e);
                }
            }
        }

        Ok(())
    }

    pub async fn unsubscribe(
        &mut self,
        topics: Vec<String>,
    ) -> Result<(), crate::error::service::ServiceError> {
        // 将已订阅的主题转换为 HashSet，便于查找
        let subscribed_topics: std::collections::HashSet<String> =
            self.data.keys().cloned().collect();

        // 过滤出需要取消的订阅
        let unique_topics: Vec<String> =
            topics.into_iter().filter(|topic| subscribed_topics.contains(topic)).collect();

        if unique_topics.is_empty() {
            return Ok(());
        }

        let mqtt_processor =
            MQTT_PROCESSOR.get().ok_or(crate::error::service::ServiceError::System(
                crate::error::system::SystemError::MqttClientNotInit,
            ))?;
        tracing::debug!("取消订阅的主题: {}", unique_topics.join(", "));
        for topic in unique_topics.iter() {
            match mqtt_processor.client().try_unsubscribe(topic) {
                Ok(_) => {
                    tracing::debug!("取消订阅成功: {}", topic);
                    // 移除 HashMap 中的订阅数据
                    if let Some(topic_data) = self.data.remove(topic) {
                        // 从 BTreeSet 中移除对应的 TopicEntry
                        self.entry.remove(&TopicEntry {
                            topic: topic.clone(),
                            last_updated: topic_data.last_updated,
                        });
                    }
                }
                Err(e) => {
                    tracing::error!("取消订阅失败: {}, 错误信息：{:?}", topic, e);
                }
            }
        }
        tracing::debug!("取消订阅完成");
        Ok(())
    }

    pub async fn resubscribe(&self) -> Result<(), crate::error::service::ServiceError> {
        let mqtt_processor =
            MQTT_PROCESSOR.get().ok_or(crate::error::service::ServiceError::System(
                crate::error::system::SystemError::MqttClientNotInit,
            ))?;

        // 遍历 HashMap 中的所有主题，重新订阅
        for (topic, topic_data) in self.data.iter() {
            match mqtt_processor.client().try_subscribe(topic, topic_data.qos) {
                Ok(_) => {
                    tracing::debug!("重新订阅成功: {}", topic);
                }
                Err(e) => {
                    tracing::error!("重新订阅失败: {}, 错误信息：{:?}", topic, e);
                }
            }
        }
        Ok(())
    }
}
