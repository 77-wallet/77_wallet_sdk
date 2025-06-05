use wallet_database::{
    dao::assets::CreateAssetsVo, entities::assets::AssetsId, repositories::assets::AssetsRepoTrait,
};
use wallet_transport_backend::request::{TokenQueryByContractAddressReq, TokenQueryPriceReq};

use crate::{
    infrastructure::task_queue::{CommonTask, Task, Tasks},
    messaging::notify::{event::NotifyEvent, FrontendNotifyEvent},
    service::asset::AssetsService,
};

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

// biz_type = INIT
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Init(Vec<InitBody>);

// biz_type = INIT
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InitBody {
    // 地址
    pub address: String,
    // 链码
    pub chain_code: String,
    // 余额
    // #[serde(deserialize_with = "wallet_utils::serde_func::deserialize_decimal_from_str")]
    pub balance: f64,
    // 代币编码
    #[serde(deserialize_with = "wallet_utils::serde_func::deserialize_uppercase")]
    pub code: String,
    // 合约地址
    #[serde(
        rename = "contractAddress",
        default,
        deserialize_with = "wallet_utils::serde_func::deserialize_empty_string_as_none"
    )]
    pub token_address: Option<String>,
    // 代币精度
    // pub decimals: Option<u8>,
}

impl Init {
    pub(crate) async fn exec(self, _msg_id: &str) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let backend = crate::manager::Context::get_global_backend_api()?;
        let cryptor = crate::Context::get_global_aes_cbc_cryptor()?;
        let repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
        let mut assets_service = AssetsService::new(repo);

        let Init(body) = &self;
        for init in body {
            let InitBody {
                ref address,
                ref chain_code,
                ref balance,
                ref code,
                ref token_address,
                // decimals: _,
            } = init;

            let assets = assets_service
                .repo
                .assets_by_id(&AssetsId {
                    address: address.to_string(),
                    chain_code: chain_code.to_string(),
                    symbol: code.to_string(),
                })
                .await?;

            let chain_instance =
                crate::domain::chain::adapter::ChainAdapterFactory::get_transaction_adapter(
                    chain_code,
                )
                .await?;

            let decimals = if let Some(token_addr) = token_address
                && !token_addr.is_empty()
            {
                chain_instance.decimals(token_addr).await?
            } else {
                backend
                    .token_query_by_contract_address(
                        cryptor,
                        &TokenQueryByContractAddressReq {
                            chain_code: chain_code.to_string(),
                            contract_address: "".to_string(),
                        },
                    )
                    .await?
                    .unit
            };
            tracing::warn!("Init msg: {_msg_id} ==================5");
            // TODO: 处理余额计算
            let balance = wallet_utils::unit::convert_to_u256(&balance.to_string(), decimals)?;

            match assets {
                Some(_assets) => {
                    let format_balance = wallet_utils::unit::format_to_string(balance, decimals)?;
                    assets_service
                        .repo
                        .update_balance(
                            &AssetsId {
                                address: address.to_string(),
                                chain_code: chain_code.to_string(),
                                symbol: code.to_string(),
                            },
                            &format_balance,
                        )
                        .await?;
                }
                None => {
                    let main_code =
                        wallet_database::entities::coin::CoinEntity::main_coin(chain_code, &*pool)
                            .await?;
                    let assets_id = AssetsId::new(address, chain_code, code);
                    let mut assets =
                        CreateAssetsVo::new(assets_id, decimals, token_address.clone(), None, 0)
                            .with_u256(alloy::primitives::U256::default(), decimals)?;

                    if let Some(main_code) = &main_code {
                        assets = assets
                            .with_name(&main_code.name)
                            .with_protocol(main_code.protocol.clone());
                    }

                    // 查询币价
                    if let Some(main_code) = main_code
                        && main_code.price.is_empty()
                    {
                        let mut req: TokenQueryPriceReq = TokenQueryPriceReq(Vec::new());
                        req.insert(
                            chain_code,
                            &assets.token_address.clone().unwrap_or_default(),
                        );
                        let task = Task::Common(CommonTask::QueryCoinPrice(req));
                        Tasks::new().push(task).send().await?;
                    }

                    assets_service.repo.upsert_assets(assets).await?;
                }
            }
        }

        let data = NotifyEvent::Init(self);
        FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use rust_decimal::Decimal;
    use wallet_transport_backend::response_vo::coin::TokenQueryByContractAddressRes;

    use crate::messaging::mqtt::topics::Init;
    use std::str::FromStr;

    #[derive(serde::Deserialize)]
    struct Test {
        price: Decimal,
        marketValue: Decimal,
    }

    #[test]
    fn test_scientific_notation_deserialization() {
        // 测试用例1：普通数字
        let normal_num = serde_json:: json!({
            "price": 668.32,
            "marketValue": 97133760610.0
        });

        // 测试用例2：科学计数法
        let scientific_num = serde_json::  json!({
            "price": 6.683208365533216E2,  // 668.3208365533216
            "marketValue": 9.713376061E10  // 97133760610.0
        });

        // 测试用例3：极大数字的科学计数法
        let large_scientific = serde_json::  json!({
            "price": 1.23E+20,
            "marketValue": 9.99E+30
        });

        // 测试用例4：极小数科学计数法
        let small_scientific = serde_json::            json!({
            "price": 1.23E-10,
            "marketValue": 9.99E-8
        });

        // 定义辅助函数
        fn test_deserialization(value: serde_json::Value) -> Result<(), String> {
            let res: Result<Test, _> = wallet_utils::serde_func::serde_from_value(value);

            match res {
                Ok(data) => {
                    println!("Deserialized successfully:");
                    println!("Price: {:?}", data.price);
                    println!("Market Value: {:?}", data.marketValue);
                    Ok(())
                }
                Err(e) => Err(format!("Failed to deserialize: {}", e)),
            }
        }

        // 执行测试
        println!("=== Testing normal number ===");

        println!("\n=== Testing scientific notation ===");
        println!("\n=== Testing large scientific ===");

        let res = test_deserialization(small_scientific);
        println!("Result: {:?}", res);
        // assert!(test_deserialization(large_scientific).is_ok());

        println!("\n=== Testing small scientific ===");
        // assert!(test_deserialization(small_scientific).is_ok());
    }

    #[test]
    fn test_() {
        let raw = r#"
        {
            "bizType": "INIT",
            "body": [
                {
                    "address": "TCVt2AYPjUZdSvLgUy8x2xhT7uj1FrQRZs",
                    "balance": 2000000000,
                    "chainCode": "tron",
                    "code": "trx"
                }
            ],
            "clientId": "wenjing",
            "deviceType": "ANDROID",
            "sn": "wenjing"
        }
        "#;
        let res = serde_json::from_str::<Init>(&raw);
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
