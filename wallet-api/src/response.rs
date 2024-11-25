#[derive(serde::Serialize, serde::Deserialize, Debug, Default, PartialEq, Eq, Clone)]
pub struct Response<T> {
    pub code: u32,
    pub message: String,
    pub result: Option<T>,
}

impl<T> From<Result<T, crate::ServiceError>> for Response<T>
where
    T: serde::Serialize + Sized,
{
    fn from(res: Result<T, crate::ServiceError>) -> Self {
        match res {
            Ok(ok) => ok.into(),
            Err(err) => {
                let (code, message) = err.into();
                Response {
                    code,
                    message,
                    result: None,
                }
            }
        }
    }
}

/// any type into ok response
impl<T> From<T> for Response<T>
where
    T: serde::Serialize + Sized,
{
    fn from(msg: T) -> Self {
        Self {
            code: 200,
            message: String::new(),
            result: Some(msg),
        }
    }
}

impl From<crate::ServiceError> for (u32, String) {
    fn from(err: crate::ServiceError) -> Self {
        // 网络的错误类型单独提出来
        if err.is_network_error() {
            return (502, err.to_string());
        }

        let (code, message) = match err {
            // 给前端展示的错误
            crate::ServiceError::Business(msg) => (msg.get_status_code(), msg.to_string()), // 业务逻辑错误
            crate::ServiceError::Parameter(_) => (422, err.to_string()),
            // 不给前端展示的错误（返回统一的错误信息，详细信息仅记录日志）
            crate::ServiceError::System(_) => (500, err.to_string()), // 系统内部错误
            crate::ServiceError::Keystore(_) => (510, err.to_string()), // 密钥库错误
            crate::ServiceError::Utils(_) => (520, err.to_string()),  // 工具类错误
            // 后端错误
            crate::ServiceError::TransportBackend(bakend_err) => map_backend_error(bakend_err),
            crate::ServiceError::Transport(_) => (531, err.to_string()), // 传输错误
            crate::ServiceError::ChainInteract(_) => (540, err.to_string()), // 链交互错误
            crate::ServiceError::ChainInstance(_) => (550, err.to_string()), // 链实例错误
            crate::ServiceError::Core(_) => (610, err.to_string()),      // 核心逻辑错误
            crate::ServiceError::Types(_) => (620, err.to_string()),     // 类型相关错误
            crate::ServiceError::Database(_) => (630, err.to_string()),
            crate::ServiceError::Tree(_) => (640, err.to_string()),
        };
        (code, message)
    }
}

// 后端的某些错误映射
fn map_backend_error(err: wallet_transport_backend::Error) -> (u32, String) {
    match err {
        wallet_transport_backend::Error::BackendServiceError(service_err) => match service_err {
            wallet_transport_backend::error::BackendServiceError::NotPlatformAddress => (
                crate::MultisigAccountError::NotPlatFormAddress.get_status_code(),
                service_err.to_string(),
            ),
        },
        _ => (530, err.to_string()),
    }
}

impl<T> std::ops::FromResidual<Result<std::convert::Infallible, crate::ServiceError>>
    for Response<T>
{
    fn from_residual(residual: Result<std::convert::Infallible, crate::ServiceError>) -> Self {
        match residual {
            Err(err) => {
                let (code, message) = err.into();
                Response {
                    code,
                    message,
                    result: None,
                }
            }
        }
    }
}

impl<T> From<Result<T, crate::Errors>> for Response<T>
where
    T: serde::Serialize + Sized,
{
    fn from(res: Result<T, crate::Errors>) -> Self {
        match res {
            Ok(ok) => ok.into(),
            Err(err) => {
                let (code, message) = (err).into();
                Response {
                    code,
                    message,
                    result: None,
                }
            }
        }
    }
}

impl From<crate::Errors> for (u32, String) {
    fn from(err: crate::Errors) -> Self {
        let (code, message) = match err {
            crate::Errors::Service(e) => e.into(),
            crate::Errors::Parameter(_) => (204, err.to_string()),
        };
        (code, message)
    }
}

impl<T> std::ops::FromResidual<Result<std::convert::Infallible, crate::Errors>> for Response<T> {
    fn from_residual(residual: Result<std::convert::Infallible, crate::error::Errors>) -> Self {
        match residual {
            Err(err) => {
                let (code, message) = err.into();
                Response {
                    code,
                    message,
                    result: None,
                }
            }
        }
    }
}
