use rust_decimal::Decimal;
use wallet_database::{
    ApiWalletDbPool,
    entities::{api_business_notification::NewApiBusinessNotification, task_queue::WalletType},
    repositories::api_business_notification::ApiBusinessNotificationRepo,
};

use crate::{
    error::service::ServiceError,
    messaging::mqtt::message::{BizType, Message},
};

/// Confirmed backend fields; amounts are coin quantities, not smallest units.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AwmAddressInsufficientBody {
    pub(crate) remaining: Decimal,
    pub(crate) token: String,
    pub(crate) symbol: String,
    pub(crate) merchant_name: String,
    #[serde(rename = "waddressType")]
    pub(crate) w_address_type: String,
    pub(crate) threshold_value: Decimal,
    pub(crate) chain_code: String,
    #[serde(rename = "targetAddress")]
    pub(crate) address: String,
}

/// Carry the outer UID into the durable task body before the MQTT envelope is dropped.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(crate) struct AwmAddressInsufficient {
    pub(crate) uid: String,
    pub(crate) body: AwmAddressInsufficientBody,
    #[serde(default)]
    pub(crate) received_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl AwmAddressInsufficient {
    pub(crate) fn from_message(message: &Message) -> Result<Self, ServiceError> {
        if !matches!(message.biz_type, BizType::AwmAddressInsufficient)
            || message.wallet_type != Some(WalletType::ApiWaw)
        {
            return Err(ServiceError::Parameter(
                "balance alert must target an API withdrawal wallet".into(),
            ));
        }
        let uid = message
            .uid
            .as_deref()
            .filter(|uid| !uid.trim().is_empty())
            .ok_or_else(|| ServiceError::Parameter("balance alert UID is missing".into()))?
            .to_owned();
        let body: AwmAddressInsufficientBody = serde_json::from_value(message.body.clone())
            .map_err(|e| ServiceError::Parameter(format!("invalid balance alert body: {e}")))?;
        if body.chain_code.trim().is_empty()
            || body.address.trim().is_empty()
            || body.symbol.trim().is_empty()
        {
            return Err(ServiceError::Parameter(
                "balance alert chain, address, or symbol is empty".into(),
            ));
        }
        Ok(Self { uid, body, received_at: Some(chrono::Utc::now()) })
    }

    pub(crate) async fn exec(
        &self,
        ctx: &'static crate::context::Context,
    ) -> Result<(), ServiceError> {
        let pool = ctx.api_wallet_pool()?;
        self.persist(&pool).await?;
        let key = format!(
            "{}:{}:{}:{}",
            self.uid, self.body.chain_code, self.body.address, self.body.token
        );
        if let Some(row) = ApiBusinessNotificationRepo::find(&pool, &self.uid, &key).await? {
            crate::application::api_wallet_balance_check::enqueue(ctx, &row, None).await?;
        }
        Ok(())
    }

    async fn persist(&self, pool: &ApiWalletDbPool) -> Result<(), ServiceError> {
        let key = format!(
            "{}:{}:{}:{}",
            self.uid, self.body.chain_code, self.body.address, self.body.token
        );
        let mut payload_body = serde_json::to_value(&self.body)
            .map_err(|e| ServiceError::Parameter(format!("invalid balance alert payload: {e}")))?;
        payload_body["notifiedAt"] =
            serde_json::json!(self.received_at.unwrap_or_else(chrono::Utc::now));
        let payload = serde_json::to_string(&payload_body)
            .map_err(|e| ServiceError::Parameter(format!("invalid balance alert payload: {e}")))?;
        ApiBusinessNotificationRepo::ingest(
            pool,
            NewApiBusinessNotification {
                id: &key,
                uid: &self.uid,
                kind: "AWM_ADDRESS_INSUFFICIENT",
                business_key: &key,
                payload: &payload,
            },
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use wallet_database::{
        SqliteContext, repositories::api_business_notification::ApiBusinessNotificationRepo,
    };

    use crate::messaging::mqtt::{
        message::Message, topics::api_wallet::address_insufficient::AwmAddressInsufficient,
    };

    fn message(body: &str, uid: Option<&str>, wallet_type: &str) -> Message {
        let mut payload = serde_json::json!({
            "msgId": "balance-alert-1",
            "bizType": "AWM_ADDRESS_INSUFFICIENT",
            "body": serde_json::from_str::<serde_json::Value>(body).unwrap(),
            "clientId": "client",
            "deviceType": "ANDROID",
            "sn": "device",
            "walletType": wallet_type,
        });
        if let Some(uid) = uid {
            payload["uid"] = uid.into();
        }
        serde_json::from_value(payload).unwrap()
    }

    #[test]
    fn confirmed_backend_fields_parse_and_serialize_without_aliases() {
        let body = r#"{"thresholdValue":3000,"remaining":2,"token":"","symbol":"trx","merchantName":"fixture","targetAddress":"TAddress","chainCode":"tron","waddressType":"2"}"#;
        let parsed =
            AwmAddressInsufficient::from_message(&message(body, Some("owner"), "API_WAW")).unwrap();
        assert_eq!(parsed.body.threshold_value, rust_decimal::Decimal::new(3000, 0));
        assert_eq!(parsed.body.remaining, rust_decimal::Decimal::new(2, 0));
        assert_eq!(parsed.body.symbol, "trx");
        assert_eq!(parsed.body.token, "");
        let encoded = serde_json::to_value(&parsed.body).unwrap();
        assert_eq!(encoded["targetAddress"], "TAddress");
        assert_eq!(encoded["waddressType"], "2");
        assert!(encoded.get("address").is_none());
        assert!(encoded.get("wAddressType").is_none());
        let legacy =
            body.replace("targetAddress", "address").replace("waddressType", "wAddressType");
        assert!(
            AwmAddressInsufficient::from_message(&message(&legacy, Some("owner"), "API_WAW"))
                .is_err()
        );
    }

    #[test]
    fn envelope_parses_balance_alert_and_keeps_outer_uid_for_replay() {
        let body = r#"{
            "remaining": 12.50,
            "token": "",
            "symbol": "TRX",
            "merchantName": "merchant",
            "waddressType": "1",
            "thresholdValue": "20.00",
            "chainCode": "tron",
            "targetAddress": "TAddress"
        }"#;
        let parsed =
            AwmAddressInsufficient::from_message(&message(body, Some("owner"), "API_WAW")).unwrap();
        assert_eq!(parsed.uid, "owner");
        assert_eq!(parsed.body.address, "TAddress");
        assert_eq!(parsed.body.remaining, rust_decimal::Decimal::new(125, 1));
        assert_eq!(parsed.body.threshold_value.to_string(), "20.00");

        let replayed: AwmAddressInsufficient =
            serde_json::from_str(&serde_json::to_string(&parsed).unwrap()).unwrap();
        assert_eq!(replayed.uid, "owner");
        assert_eq!(replayed.received_at, parsed.received_at);
        assert!(replayed.received_at.is_some());
        // Existing durable tasks without the new timestamp still decode.
        let mut legacy = serde_json::to_value(&parsed).unwrap();
        legacy.as_object_mut().unwrap().remove("received_at");
        let legacy: AwmAddressInsufficient = serde_json::from_value(legacy).unwrap();
        assert!(legacy.received_at.is_none());
    }

    #[test]
    fn balance_alert_rejects_missing_identity_or_wrong_wallet() {
        let valid_body = r#"{
            "remaining": 1,
            "token": "",
            "symbol": "TRX",
            "merchantName": "merchant",
            "waddressType": "1",
            "thresholdValue": 20,
            "chainCode": "tron",
            "targetAddress": "TAddress"
        }"#;
        assert!(
            AwmAddressInsufficient::from_message(&message(valid_body, None, "API_WAW")).is_err()
        );
        assert!(
            AwmAddressInsufficient::from_message(&message(
                valid_body,
                Some("owner"),
                "NORMAL_WALLET"
            ))
            .is_err()
        );
        let missing_address =
            valid_body.replace("\"targetAddress\": \"TAddress\"", "\"targetAddress\": \"\"");
        assert!(
            AwmAddressInsufficient::from_message(&message(
                &missing_address,
                Some("owner"),
                "API_WAW"
            ))
            .is_err()
        );
    }

    #[tokio::test]
    async fn repeated_alert_updates_one_uid_scoped_reminder() {
        let dir = std::env::temp_dir()
            .join(format!("balance_alert_{}", wallet_utils::snowflake::get_uid().unwrap()));
        std::fs::create_dir_all(&dir).unwrap();
        let pool = SqliteContext::new(dir.to_str().unwrap(), Some("api_wallet.db"))
            .await
            .unwrap()
            .into_api_wallet_db_pool()
            .unwrap();
        let body = r#"{"remaining":10,"token":"","symbol":"TRX","merchantName":"merchant","waddressType":"1","thresholdValue":20,"chainCode":"tron","targetAddress":"TAddress"}"#;
        let first =
            AwmAddressInsufficient::from_message(&message(body, Some("owner"), "API_WAW")).unwrap();
        first.persist(&pool).await.unwrap();
        ApiBusinessNotificationRepo::mark_resolved(&pool, "owner", "owner:tron:TAddress:", true)
            .await
            .unwrap();
        let updated = AwmAddressInsufficient::from_message(&message(
            &body.replace("\"remaining\":10", "\"remaining\":5"),
            Some("owner"),
            "API_WAW",
        ))
        .unwrap();
        updated.persist(&pool).await.unwrap();

        let rows = ApiBusinessNotificationRepo::list(&pool, "owner", true, 0, 20).await.unwrap();
        assert_eq!(rows.total_count, 1);
        assert!(rows.data[0].payload.contains("\"remaining\":\"5\""));
        assert_eq!(
            ApiBusinessNotificationRepo::list(&pool, "other", false, 0, 20)
                .await
                .unwrap()
                .total_count,
            0
        );
    }
}
