use crate::error::business::{
    chain::ChainError, coin::CoinError, multisig_account::MultisigAccountError,
};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Keystore error: `{0}`")]
    Core(#[from] wallet_core::error::Error),
    #[error("Types error: `{0}`")]
    Types(#[from] wallet_types::Error),
    #[error("Tree error: `{0}`")]
    Tree(#[from] wallet_tree::Error),
    #[error("Keystore error: `{0}`")]
    Keystore(#[from] wallet_crypto::error::Error),
    #[error("Utils error: `{0}`")]
    Utils(#[from] wallet_utils::error::Error),
    #[error("TransportBackend error: `{0}`")]
    TransportBackend(#[from] wallet_transport_backend::Error),
    #[error("Transport error: `{0}`")]
    Transport(#[from] wallet_transport::TransportError),
    #[error("Oss error: `{0}`")]
    Oss(#[from] wallet_oss::TransportError),
    #[error("Chain instance error: `{0}`")]
    ChainInstance(#[from] wallet_chain_instance::Error),
    #[error("Chain interact error: `{0}`")]
    ChainInteract(#[from] wallet_chain_interact::Error),
    #[error("System error: {0}")]
    System(#[from] crate::error::system::SystemError),
    #[error("Database error: {0}")]
    Database(#[from] wallet_database::Error),
    // 业务错误
    #[error("Business error: {0}")]
    Business(#[from] super::business::BusinessError),
    #[error("parameter error: {0}")]
    Parameter(String),
    #[error("aggregator code: {agg_code} error: {msg}")]
    AggregatorError { code: i32, agg_code: i32, msg: String },
}

impl ServiceError {
    pub fn is_network_error(&self) -> bool {
        match self {
            ServiceError::Keystore(err) => err.is_network_error(),
            ServiceError::Utils(err) => err.is_network_error(),
            ServiceError::TransportBackend(err) => err.is_network_error(),
            ServiceError::Transport(err) => err.is_network_error(),
            ServiceError::ChainInstance(err) => err.is_network_error(),
            ServiceError::ChainInteract(err) => err.is_network_error(),
            ServiceError::Database(err) => err.is_network_error(),
            _ => false,
        }
    }
}

impl From<ServiceError> for (i64, String) {
    fn from(err: ServiceError) -> Self {
        // record log to upload to aliyun
        tracing::error!(?err, "api_error");
        // Separate network error types.
        if err.is_network_error() {
            return (502, err.to_string());
        }

        let (code, message) = match err {
            ServiceError::Business(msg) => (msg.get_status_code(), msg.to_string()),
            ServiceError::Parameter(_) => (422, err.to_string()),
            ServiceError::Keystore(_) => (510, err.to_string()),
            ServiceError::Utils(_) => (520, err.to_string()),
            ServiceError::TransportBackend(bakend_err) => map_backend_error(bakend_err),
            ServiceError::Transport(_) => (531, err.to_string()),
            ServiceError::ChainInteract(err) => map_chain_interact_error(err),
            ServiceError::ChainInstance(_) => (550, err.to_string()),
            ServiceError::Core(_) => (610, err.to_string()),
            ServiceError::Types(_) => (620, err.to_string()),
            ServiceError::Database(_) => (630, err.to_string()),
            ServiceError::Tree(_) => (640, err.to_string()),
            ServiceError::Oss(_) => (650, err.to_string()),
            ServiceError::System(_) => (660, err.to_string()),
            ServiceError::AggregatorError { code, agg_code: _, msg: _ } => {
                let error = if code != -1 { code } else { 670 };
                (error as i64, err.to_string())
            }
        };
        (code, message)
    }
}

// 后端的某些错误映射
fn map_backend_error(err: wallet_transport_backend::Error) -> (i64, String) {
    match err {
        wallet_transport_backend::Error::BackendServiceError(service_err) => match service_err {
            wallet_transport_backend::error::BackendServiceError::NotPlatformAddress => (
                MultisigAccountError::NotPlatFormAddress.get_status_code(),
                service_err.to_string(),
            ),
        },
        _ => (530, err.to_string()),
    }
}

fn map_chain_interact_error(err: wallet_chain_interact::Error) -> (i64, String) {
    let msg = err.to_string();
    match err {
        wallet_chain_interact::Error::TransportError(
            wallet_transport::TransportError::NodeResponseError(node_response_error),
        ) => match node_response_error.code {
            // sol链错误码,转账金额小于租金
            -32002 => {
                let msg = node_response_error.message.unwrap_or_default();
                if msg.contains("insufficient funds for rent") {
                    (ChainError::InsufficientFundsRent.get_status_code(), msg)
                } else {
                    (node_response_error.code, msg)
                }
            }
            -32602 => {
                let err_msg = node_response_error.message.unwrap_or_default();
                (CoinError::InvalidContractAddress(err_msg.clone()).get_status_code(), err_msg)
            }
            -26 => {
                // ltc btc doge dust transaction
                let err_msg = node_response_error.message.unwrap_or_default();
                (ChainError::DustTransaction.get_status_code(), err_msg)
            }
            _ => (node_response_error.code, node_response_error.message.unwrap_or_default()),
        },
        wallet_chain_interact::Error::ContractValidationError(msg) => match msg {
            wallet_chain_interact::ContractValidationError::WithdrawTooSoon(_) => {
                (ChainError::WithdrawTooSoon.get_status_code(), msg.to_string())
            }
            wallet_chain_interact::ContractValidationError::WitnessAccountDoesNotHaveAnyReward => {
                (ChainError::WitnessAccountDoesNotHaveAnyReward.get_status_code(), msg.to_string())
            }
            wallet_chain_interact::ContractValidationError::EnergyLockPeriodTooShort(_) => {
                (ChainError::LockPeriodTooShort.get_status_code(), msg.to_string())
            }
            wallet_chain_interact::ContractValidationError::Other(ref error) => {
                if error.contains("Validate TransferContract error, balance is not sufficient") {
                    (ChainError::InsufficientBalance.get_status_code(), msg.to_string())
                } else {
                    (-40000, msg.to_string())
                }
            }
        },
        wallet_chain_interact::Error::RpcError(_) => (-40000, msg.to_string()),
        _ => (540, msg),
    }
}
