use wallet_transport_backend::response_vo::chain::ChainUrlInfo;

use crate::domain::app::config::ConfigDomain;

/*
{
    "clientId": "wenjing",
    "sn": "wenjing",
    "deviceType": "ANDROID",
    "bizType": "INIT",
    "body": [
        {
            "address": "TGyw6wH5UT5GVY5v6MTWedabScAwF4gffQ",
            "balance": 4000002,
            "chainCode": "tron",
            "code": "sadsadsad",
              "tokenAddress": "",
              "decimals": 6
        }
    ]
}
*/

// biz_type = RPC_ADDRESS_CHANGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChainChange(Vec<ChainUrlInfo>);

// biz_type = RPC_ADDRESS_CHANGE

impl ChainChange {
    pub(crate) async fn exec(self) -> Result<(), crate::ServiceError> {
        let ChainChange(body) = &self;
        ConfigDomain::set_block_browser_url(body).await?;

        let data = crate::notify::NotifyEvent::ChainChange(self);
        crate::notify::FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use crate::mqtt::payload::incoming::Message;

    #[test]
    fn test_() {
        let raw = r#"
        {
            "appId": "13065ffa4e8f6958bd6",
            "bizType": "RPC_ADDRESS_CHANGE",
            "body": [{
                "chainCode": "sol",
                "rpcAddressInfoBodyList": [{
                    "chainId": 1,
                    "id": "66c597d2c4aa1c8385046116",
                    "name": "sol",
                    "url": "http://rpc.88ai.fun/sol"
                }]
            }, {
                "chainCode": "eth",
                "rpcAddressInfoBodyList": [{
                    "chainId": 1,
                    "id": "675c02a8f4d96273e8cd9653",
                    "name": "eth8",
                    "url": "http://rpc.88ai.fun/eth"
                }, {
                    "id": "675c02a8f4d96273e8cd9654",
                    "name": "eth022",
                    "url": "http://rpc.88ai.fun/eth"
                }]
            }, {
                "chainCode": "tron",
                "rpcAddressInfoBodyList": [{
                    "id": "676162e51350347bf4774d1b",
                    "name": "tron2",
                    "url": "http://www.222.com"
                }, {
                    "id": "676162e51350347bf4774d1a",
                    "name": "tron1",
                    "url": "http://www.1111.com"
                }, {
                    "id": "676162fe1350347bf4774d1c",
                    "name": "tron3",
                    "url": "http://www.333.com"
                }]
            }],
            "clientId": "b205d2716d87d73af508ff2375149487",
            "deviceType": "ANDROID",
            "sn": "ebe42b137abb313f0d0012f588080395c3742e7eac77e60f43fac0afb363e67c",
            "msgId": "6761634c9020540c37dc343f"
        }
        "#;
        let res = serde_json::from_str::<Message>(&raw);
        println!("res: {res:?}");
    }

    #[test]
    fn test_decimal() {
        let balance = wallet_types::Decimal::from_str("1996.733").unwrap();
        let balance = wallet_utils::unit::convert_to_u256(&balance.to_string(), 6).unwrap();
        println!("balance: {balance}");
        println!(
            "balance: {}",
            wallet_utils::unit::format_to_string(balance, 6).unwrap()
        );
        // let balance = wallet_utils::unit::u256_from_str(&balance.to_string()).unwrap();
    }
}
