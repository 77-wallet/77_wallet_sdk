#[derive(Clone)]
pub(super) enum ProcessCollectTxCommand {
    Tx(String),
}

#[derive(Clone)]
pub(super) enum ProcessCollectTxReportCommand {
    Tx(String),
}

#[derive(Clone)]
pub(super) enum ProcessCollectTxConfirmReportCommand {
    Tx(String),
}
