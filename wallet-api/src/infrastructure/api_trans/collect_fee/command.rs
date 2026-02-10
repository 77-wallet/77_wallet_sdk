#[derive(Clone)]
pub(crate) enum ProcessFeeTxCommand {
    Tx(String),
}

#[derive(Clone)]
pub(crate) enum ProcessFeeTxReportCommand {
    Tx(String),
}

#[derive(Clone)]
pub(crate) enum ProcessFeeTxConfirmReportCommand {
    Tx(String),
}
