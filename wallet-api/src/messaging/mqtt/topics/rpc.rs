use crate::domain::node::NodeDomain;

// biz_type = RPC_ADDRESS_CHANGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RpcChange(Vec<RpcChangeBody>);

// biz_type = RPC_ADDRESS_CHANGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RpcChangeBody {
    // 链码
    pub chain_code: String,
    pub rpc_address_info_body_list: Vec<RpcAddressInfoBody>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RpcAddressInfoBody {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub chain_id: Option<i32>,
    pub name: String,
    pub url: String,
}

impl RpcChange {
    pub(crate) async fn exec(self) -> Result<(), crate::ServiceError> {
        let pool = crate::manager::Context::get_global_sqlite_pool()?;
        let mut repo = wallet_database::factory::RepositoryFactory::repo(pool.clone());
        // let list = crate::default_data::node::get_default_node_list()?;

        let RpcChange(body) = &self;
        let mut backend_nodes = Vec::new();
        let mut chains_set = std::collections::HashSet::new();
        let mut chain_codes = Vec::new();
        for rpc_change_body in body {
            let RpcChangeBody {
                chain_code,
                rpc_address_info_body_list,
            } = rpc_change_body;

            for node in rpc_address_info_body_list.iter() {
                let Some(id) = &node.id else {
                    continue;
                };
                let key = (node.name.clone(), chain_code.clone());
                chains_set.insert(key);
                chain_codes.push(chain_code.to_string());
                let network = "mainnet";
                let node = wallet_database::entities::node::NodeCreateVo::new(
                    id, &node.name, chain_code, &node.url, None,
                )
                .with_network(network);
                match wallet_database::repositories::node::NodeRepoTrait::add(&mut repo, node).await
                {
                    Ok(node) => backend_nodes.push(node),
                    Err(e) => {
                        tracing::error!("node_create: {:?}", e);
                        continue;
                    }
                };
            }
        }

        NodeDomain::prune_nodes(&mut repo, &mut chains_set, Some(0)).await?;
        NodeDomain::sync_nodes_and_link_to_chains(&mut repo, chain_codes, &backend_nodes).await?;

        // let data = crate::notify::NotifyEvent::Init(self);
        // crate::notify::FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use crate::messaging::mqtt::Message;

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
