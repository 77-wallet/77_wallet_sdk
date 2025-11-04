#[derive(Clone)]
pub(crate) enum ProcessWithdrawTxCommand {
    Tx(String),
}

#[derive(Clone)]
pub(crate) enum ProcessWithdrawTxReportCommand {
    Tx(String),
}

#[derive(Clone)]
pub(crate) enum ProcessWithdrawTxConfirmReportCommand {
    Tx(String),
}
