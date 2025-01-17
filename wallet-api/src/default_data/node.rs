// struct InitChainsInfo {
//     rpc_url: String,
//     ws_url: String,
// }

use std::collections::HashMap;

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct DefaultNode {
    // pub(crate) chain_code: String,
    pub(crate) node_name: String,
    pub(crate) rpc_url: String,
    pub(crate) http_url: String,
    pub(crate) network: String,
    pub(crate) active: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct Nodes {
    pub(crate) nodes: Vec<DefaultNode>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct DefaultNodeList {
    pub(crate) nodes: HashMap<String, Nodes>,
}

static INIT_NODES_INFO: once_cell::sync::Lazy<once_cell::sync::OnceCell<DefaultNodeList>> =
    once_cell::sync::Lazy::new(once_cell::sync::OnceCell::new);

pub(crate) fn get_default_node_list() -> Result<&'static DefaultNodeList, crate::ServiceError> {
    INIT_NODES_INFO.get_or_try_init(|| {
        let toml_content = include_str!("../../data/config/node.toml");
        let toml_data: DefaultNodeList = wallet_utils::serde_func::toml_from_str(&toml_content)?;

        Ok(toml_data)
    })
}

// #[derive(Debug, Clone, serde::Deserialize)]
// pub(crate) struct ChainDefaultNode(pub(crate) HashMap<String, DefaultNode>);

// static CHAIN_DEFAULT_NODE: once_cell::sync::Lazy<once_cell::sync::OnceCell<ChainDefaultNode>> =
//     once_cell::sync::Lazy::new(once_cell::sync::OnceCell::new);

// pub(crate) fn get_chain_default_node() -> Result<&'static ChainDefaultNode, crate::ServiceError> {
//     CHAIN_DEFAULT_NODE.get_or_try_init(|| {
//         let list = get_default_node_list()?;

//         // 只需要每个链一个
//         let res = ChainDefaultNode(
//             list.nodes
//                 .iter()
//                 .map(|(chain_code, nodes)| {
//                     let node = nodes.nodes.iter().find(|n| n.active).unwrap();
//                     (chain_code.clone(), node.clone())
//                 })
//                 .collect(),
//         );
//         Ok(res)
//     })
// }

// impl ChainDefaultNode {
//     pub(crate) fn get_default_node(&self, chain_code: &str) -> Option<&DefaultNode> {
//         self.0.get(chain_code)
//     }
// }
