use std::{sync::Arc, time::Instant};
use wallet_database::entities::api_collect::ApiCollectEntity;

/// 诊断来源
#[derive(Debug, Clone, Copy)]
pub enum DiagnoseSource {
    Advancer,
    ManualAdvance,
    PeriodicScan,
}

/// 诊断阶段
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiagnoseStage {
    OrderAck,
    Build,
    Broadcast,
    Recover,
    ResultAck,
    ServiceFeeUpload,
    TxFeeResAck,
    TxExecReceipt,
    Unknown,
}

/// 诊断元数据（不可变）
#[derive(Debug, Clone)]
pub struct DiagnoseMeta {
    pub trade_no: Arc<str>,
    pub source: DiagnoseSource,
    pub stage: DiagnoseStage,
    pub ts: Instant,
}

impl DiagnoseMeta {
    pub fn new(
        trade_no: impl Into<Arc<str>>,
        source: DiagnoseSource,
        stage: DiagnoseStage,
    ) -> Self {
        Self { trade_no: trade_no.into(), source, stage, ts: Instant::now() }
    }
}

#[derive(Debug)]
pub enum DiagnoseEvent {
    /// 无推进时的诊断事件
    NoAdvancement { meta: DiagnoseMeta, collect: ApiCollectEntity },
    /// 周期性扫描时的诊断事件
    PeriodicScan { meta: DiagnoseMeta, collect: ApiCollectEntity },
    /// 手动触发的诊断事件
    ManualDiagnose {
        meta: DiagnoseMeta,
        collect: ApiCollectEntity,
        extra: String, // 附加信息
    },
    /// 意图分发失败时的诊断事件
    IntentDispatchFailed { meta: DiagnoseMeta },
}

/// 诊断事件发送器
pub type DiagnoseEventSender = tokio::sync::mpsc::Sender<DiagnoseEvent>;

/// 诊断事件接收器
pub type DiagnoseEventReceiver = tokio::sync::mpsc::Receiver<DiagnoseEvent>;

/// 创建诊断事件通道
pub fn channel(capacity: usize) -> (DiagnoseEventSender, DiagnoseEventReceiver) {
    tokio::sync::mpsc::channel(capacity)
}
