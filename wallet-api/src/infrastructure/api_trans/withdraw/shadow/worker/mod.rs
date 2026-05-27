// withdraw/shadow/worker/mod.rs
mod shadow_withdraw_worker;
mod side_effect_worker;

pub use shadow_withdraw_worker::ShadowWithdrawWorker;
pub use side_effect_worker::SideEffectWorker;

/// ShadowWithdrawWorker 命令
#[derive(Debug, Clone)]
pub enum ShadowWithdrawCommand {
    /// 写入审计展示用手续费预估快照
    EstimateFee(String),
    /// 评估资源闸门
    EvalResourceGate(String),
    /// 执行平台代理资源代理任务
    ExecuteResourceDelegation(String),
    /// 构建交易
    BuildTx(String),
    /// 广播交易
    Broadcast(String),
    /// 恢复交易
    Recover(String),
}

/// SideEffectWorker 命令
#[derive(Debug, Clone)]
pub enum SideEffectCommand {
    /// 发送交易 ACK
    SendTxAck(String),
    /// 发送平台资源结果确认 ACK
    SendResourceResultAck(String),
    /// 发送平台资源任务接收 ACK
    SendResourceTaskAck(String),
    /// 上传平台代理资源执行回执
    UploadResourceTxExecReceipt(String),
    /// 发送交易结果 ACK
    SendTxResAck(String),
    /// 上传交易执行回执
    UploadTxExecReceipt(String),
}
