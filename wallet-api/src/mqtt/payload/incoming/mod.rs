use permission::PermissionAccept;

pub mod announcement;
pub mod chain;
pub mod init;
pub mod permission;
pub mod resource;
pub mod rpc;
pub mod signature;
#[cfg(feature = "token")]
pub mod token;
pub mod transaction;

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Message {
    // 消息id
    pub(crate) msg_id: String,
    // 业务类型(一个枚举值)
    pub(crate) biz_type: BizType,
    // 业务数据 T 泛型
    pub(crate) body: serde_json::Value,
    // 客户端标识
    #[allow(dead_code)]
    pub(crate) client_id: String,
    // 设备号
    #[allow(dead_code)]
    pub(crate) sn: String,
    // 设备类型
    #[allow(dead_code)]
    pub(crate) device_type: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BizType {
    // 多签订单通知
    /// ORDER_MULTI_SIGN_ACCEPT
    /// 订单多签-发起签名受理   同步多签账号(已测试)
    OrderMultiSignAccept,
    /// ORDER_MULTI_SIGN_ACCEPT_COMPLETE_MSG
    /// 订单多签-发起签名受理完成-消息通知
    OrderMultiSignAcceptCompleteMsg,
    /// ORDER_MULTI_SIGN_SERVICE_COMPLETE
    /// 订单多签-服务费收取完成
    OrderMultiSignServiceComplete,
    /// 订单多签-账户创建完成
    OrderMultiSignCreated,
    /// 订单多签-账户创建取消
    OrderMultiSignCancel,

    // /// 同步参与状态消息 发给发送者
    // SyncMultisigAccountStatus,
    // /// 订单多签-订单全部完成 多签订单完成后，所有人（发起人，参与人）都要收到消息
    // OrderMultiSignAllComplete,
    /// 多签转账-发起签名受理完成
    MultiSignTransAccept,
    /// 多签转账-发起签名受理完成
    MultiSignTransCancel,
    /// 多签转账-发起签名受理完成
    MultiSignTransAcceptCompleteMsg,

    /// 多签转账-确认完成
    MultiSignTransAcceptHashComplete,

    /// 账变
    AcctChange,

    /// 账户余额初始化
    Init,
    /// 代币价格变动
    TokenPriceChange,
    /// 公告
    BulletinMsg,
    /// 节点变动
    RpcAddressChange,

    /// 资源变动
    TronSignFreezeDelegateVoteChange,
    /// 权限更新
    PermissionAccept,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(untagged)]
pub enum Body {
    OrderMultiSignAccept(signature::OrderMultiSignAccept),
    OrderMultiSignAcceptCompleteMsg(signature::OrderMultiSignAcceptCompleteMsg),
    OrderMultiSignServiceComplete(signature::OrderMultiSignServiceComplete),
    OrderMultiSignCreated(signature::OrderMultiSignCreated),
    OrderMultiSignCancel(signature::OrderMultiSignCancel),
    MultiSignTransAccept(transaction::MultiSignTransAccept),
    MultiSignTransCancel(transaction::MultiSignTransCancel),

    MultiSignTransAcceptCompleteMsg(transaction::MultiSignTransAcceptCompleteMsg),
    /// 账变
    AcctChange(transaction::AcctChange),
    Init(init::Init),
    #[cfg(feature = "token")]
    TokenPriceChange(token::TokenPriceChange),
    BulletinMsg(announcement::BulletinMsg),
    RpcChange(rpc::RpcChange),

    /// 资源
    TronSignFreezeDelegateVoteChange(resource::TronSignFreezeDelegateVoteChange),
    /// 权限更新
    PermissionAccept(PermissionAccept),
}
