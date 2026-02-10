#[derive(Clone)]
pub(super) enum ProcessWithdrawTxCommand {
    Tx(String),
}

#[derive(Clone)]
pub(super) enum ProcessWithdrawTxReportCommand {
    Tx(String),
}

#[derive(Clone)]
pub(super) enum ProcessWithdrawTxConfirmReportCommand {
    Tx(String),
}
