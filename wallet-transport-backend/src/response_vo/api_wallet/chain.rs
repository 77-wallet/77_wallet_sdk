use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiChainListResp(pub Vec<ApiChainItem>);

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiChainItem {
    create_time: String,
    update_time: String,
    id: String,
    /// 链名称
    name: Option<String>,
    code: String,
    /// 链id
    chain_id: Option<i64>,
    /// 线上块高
    global_height: i64,
    /// 本地块高
    local_height: i64,
    /// 启用
    #[serde(default)]
    enable: bool,
    /// 排序
    seq: i64,
    /// 默认链
    #[serde(default)]
    default_chain: bool,
    /// TG监控通知
    #[serde(default)]
    tg_alert: bool,
    /// 查看链上地址URL
    #[serde(default)]
    address_url: String,
    /// 查看链上hash URL
    hash_url: String,
    /// 查看链上合约地址URL
    token_url: String,
    /// 启用块高监听开关
    #[serde(default)]
    enable_block: bool,
    /// 出块时间 例：2秒，1小时，针对tg
    block_time: String,
}
