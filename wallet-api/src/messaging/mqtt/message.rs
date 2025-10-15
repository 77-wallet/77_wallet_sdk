use std::fmt;
use crate::messaging::mqtt::topics::{
    AcctChange, BulletinMsg, MultiSignTransAccept, MultiSignTransAcceptCompleteMsg,
    MultiSignTransCancel, OrderMultiSignAccept, OrderMultiSignAcceptCompleteMsg,
    OrderMultiSignCancel, OrderMultiSignCreated, OrderMultiSignServiceComplete, PermissionAccept,
    RpcChange,
    api_wallet::{
        cmd::{
            address_allock::AwmCmdAddrExpandMsg, address_use::AddressUseMsg,
            fee_res::AwmCmdFeeResMsg, unbind_uid::AwmCmdUidUnbindMsg,
            wallet_activation::AwmCmdActiveMsg,
        },
        trans::AwmOrderTransMsg,
        trans_result::AwmOrderTransResMsg,
    },
};

use super::topics::{CleanPermission, multisign_trans_execute::MultiSignTransExecute};

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Message {
    // 消息id
    pub(crate) msg_id: String,
    // 业务类型(一个枚举值)
    pub(crate) biz_type: BizType,
    // 业务数据
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
    // ORDER_MULTI_SIGN_ACCEPT
    // 订单多签-发起签名受理   同步多签账号(已测试)
    OrderMultiSignAccept,
    // ORDER_MULTI_SIGN_ACCEPT_COMPLETE_MSG
    // 订单多签-发起签名受理完成-消息通知
    OrderMultiSignAcceptCompleteMsg,
    // ORDER_MULTI_SIGN_SERVICE_COMPLETE
    // 订单多签-服务费收取完成
    OrderMultiSignServiceComplete,
    // 订单多签-账户创建完成
    OrderMultiSignCreated,
    // 订单多签-账户创建取消
    OrderMultiSignCancel,
    // 同步参与状态消息 发给发送者
    // SyncMultisigAccountStatus,
    // 订单多签-订单全部完成 多签订单完成后，所有人（发起人，参与人）都要收到消息
    // OrderMultiSignAllComplete,
    // 多签转账-发起签名受理完成
    MultiSignTransAccept,
    // 多签转账-发起签名受理完成
    MultiSignTransCancel,
    // 多签转账-发起签名受理完成
    MultiSignTransAcceptCompleteMsg,
    // 多签转账-确认完成
    MultiSignTransAcceptHashComplete,
    // 账变
    AcctChange,
    // // 账户余额初始化
    // Init,
    // 代币价格变动
    TokenPriceChange,
    /// 公告
    BulletinMsg,
    // 节点变动
    RpcAddressChange,
    // 资源变动
    TronSignFreezeDelegateVoteChange,
    // 权限更新
    PermissionAccept,
    // 所有签名已经完成
    OrderMultiSignAllMemberAccepted,
    // 多签交易执行事件(修改成员交易队列的状态)
    MultiSignTransExecute,
    // 多签账号部署需要清空原来账号的权限
    CleanPermission,

    // api wallet
    // 地址使用
    AddressUse,

    // AWM_ORDER_TRANS API钱包的订单消息
    AwmOrderTrans,
    /// AWM_ORDER_TRANS_RES API钱包的订单结果消息
    AwmOrderTransRes,
    /// AWM_CMD_ADDR_EXPAND API钱包的地址扩容消息
    AwmCmdAddrExpand,
    // AWM_CMD_UID_UNBIND API钱包的钱包解绑消息
    AwmCmdUidUnbind,
    // AWM_CMD_FEE_RES API手续费结果事件
    AwmCmdFeeRes,
    // AWM_CMD_ACTIVE API钱包激活
    AwmCmdActive,
}

impl fmt::Display for BizType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BizType::OrderMultiSignAccept => "OrderMultiSignAccept",
            BizType::OrderMultiSignAcceptCompleteMsg => "OrderMultiSignAcceptCompleteMsg",
            BizType::OrderMultiSignServiceComplete => "OrderMultiSignServiceComplete",
            BizType::OrderMultiSignCreated => "OrderMultiSignCreated",
            BizType::OrderMultiSignCancel => "OrderMultiSignCreated",
            BizType::MultiSignTransAccept => "MultiSignTransAccept",
            BizType::MultiSignTransCancel => "MultiSignTransCancel",
            BizType::MultiSignTransAcceptCompleteMsg => "MultiSignTransAcceptCompleteMsg",
            BizType::MultiSignTransAcceptHashComplete => "MultiSignTransAcceptHashComplete",
            BizType::AcctChange => "AcctChange",
            BizType::TokenPriceChange => "TokenPriceChange",
            BizType::BulletinMsg => "BulletinMsg",
            BizType::RpcAddressChange => "RpcAddressChange",
            BizType::TronSignFreezeDelegateVoteChange => "TronSignFreezeDelegateVoteChange",
            BizType::PermissionAccept => "PermissionAccept",
            BizType::OrderMultiSignAllMemberAccepted => "OrderMultiSignAllMemberAccepted",
            BizType::MultiSignTransExecute => "MultiSignTransExecute",
            BizType::CleanPermission => "CleanPermission",
            BizType::AddressUse => "AddressUse",
            BizType::AwmOrderTrans => "AwmOrderTrans",
            BizType::AwmOrderTransRes => "AwmOrderTransRes",
            BizType::AwmCmdAddrExpand => "AwmCmdAddrExpand",
            BizType::AwmCmdUidUnbind => "AwmCmdUidUnbind",
            BizType::AwmCmdFeeRes => "AwmCmdFeeRes",
            BizType::AwmCmdActive => "AwmCmdActive",
        };
        write!(f, "{}", s)
    }
}


#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(untagged)]
pub enum Body {
    OrderMultiSignAccept(OrderMultiSignAccept),
    OrderMultiSignAcceptCompleteMsg(OrderMultiSignAcceptCompleteMsg),
    OrderMultiSignServiceComplete(OrderMultiSignServiceComplete),
    OrderMultiSignCreated(OrderMultiSignCreated),
    OrderMultiSignCancel(OrderMultiSignCancel),
    MultiSignTransAccept(MultiSignTransAccept),
    MultiSignTransCancel(MultiSignTransCancel),

    MultiSignTransAcceptCompleteMsg(MultiSignTransAcceptCompleteMsg),
    /// 账变
    AcctChange(AcctChange),
    // Init(Init),
    #[cfg(feature = "token")]
    TokenPriceChange(crate::messaging::mqtt::topics::TokenPriceChange),
    BulletinMsg(BulletinMsg),
    RpcChange(RpcChange),

    /// 资源
    // TronSignFreezeDelegateVoteChange(TronSignFreezeDelegateVoteChange),
    /// 权限更新
    PermissionAccept(PermissionAccept),
    OrderMultiSignAllMemberAccepted,
    OrderMultiTransExecute(MultiSignTransExecute),
    CleanPermission(CleanPermission),

    /// api wallet
    AwmOrderTrans(AwmOrderTransMsg),
    AwmOrderTransRes(AwmOrderTransResMsg),
    AwmCmdAddrExpand(AwmCmdAddrExpandMsg),
    // AwmCmdFeeRes(AwmCmdFeeResMsg),
    AwmCmdActive(AwmCmdActiveMsg),
    AwmCmdUidUnbind(AwmCmdUidUnbindMsg),
    AddressUse(AddressUseMsg),
    AwmCmdOrderTransFeeRes(AwmCmdFeeResMsg),
}
