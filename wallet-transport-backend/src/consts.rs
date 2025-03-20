pub mod endpoint {
    pub mod multisig {
        // Agree to participate in a multisig account
        pub const SIGNED_ORDER_ACCEPT: &str = "signed/order/accept";

        // Initiator cancels a multisig account
        pub const SIGNED_ORDER_CANCEL: &str = "signed/order/cancel";

        // Multisig deployment: Report the service fee hash to the backend
        pub const SIGNED_ORDER_UPDATE_RECHARGE_HASH: &str = "signed/order/updateRechargeHash";

        // Multisig deployment: Report the deployment transaction hash to the backend
        pub const SIGNED_ORDER_UPDATE_SIGNED_HASH: &str = "signed/order/updateSignedHash";

        // Create a multisig transaction queue
        // pub const SIGNED_TRAN_CREATE: &str = "signed/trans/create";
        pub const SIGNED_TRAN_CREATE: &str = "signed/trans/v2/create";

        // Sign a multisig transaction
        pub const SIGNED_TRAN_ACCEPT: &str = "signed/trans/accept";

        // Report the transaction hash of the executed multisig to the backend
        pub const SIGNED_TRAN_UPDATE_TRANS_HASH: &str = "signed/trans/updateTransdHash";

        pub const PERMISSION_ACCEPT: &str = "permission/change";
    }

    pub const UPLOAD_PERMISSION_TRANS: &str = "permission/uploadTrans";

    pub const DEVICE_INIT: &str = "device/init";
    pub const DEVICE_DELETE: &str = "device/delete";
    pub const DEVICE_UNBIND_ADDRESS: &str = "device/unBindAddress";
    pub const DEVICE_BIND_ADDRESS: &str = "device/bindAddress";
    pub const KEYS_INIT: &str = "keys/init";
    pub const ADDRESS_INIT: &str = "address/init";
    pub const LANGUAGE_INIT: &str = "language/init";

    pub const TOKEN_CUSTOM_TOKEN_INIT: &str = "token/custom/token/init";
    pub const TOKEN_QUERY_RATES: &str = "token/queryRates";
    pub const SYS_CONFIG_FIND_CONFIG_BY_KEY: &str = "sys/config/findConfigByKey";
    pub const APP_INSTALL_DOWNLOAD: &str = "app/install/download";
    pub const VERSION_VIEW: &str = "version/view";
    // pub const CHAIN_DEFAULT_LIST: &str = "chain/defaultList";
    pub const CHAIN_LIST: &str = "chain/list";
    pub const CHAIN_RPC_LIST: &str = "chain/rpcList";
    pub const MQTT_INIT: &str = "mqtt/init";

    pub const SEND_MSG_CONFIRM: &str = "sendMsg/confirm";

    pub const VERSION_DOWNLOAD: &str = "version/download";

    pub const ADDRESS_FIND_MULTI_SIGNED_DETAILS: &str = "address/findMultiSignedDetails";
}

/// 代理的全局rpc节点
pub const BASE_RPC_URL: &str = "apprpc.88ai.fun";

cfg_if::cfg_if! {
    // 默认使用开发环境 (dev)
    if #[cfg(any(feature = "dev", not(any(feature = "test", feature = "prod"))))] {
        pub const BASE_URL: &str = "https://walletapi.puke668.top";
    // 测试环境
    } else if #[cfg(feature = "test")] {
        pub const BASE_URL: &str = "https://test-api.puke668.top";

    // 生产环境
    } else if #[cfg(feature = "prod")] {
        pub const BASE_URL: &str = "https://api.77wallet.org";
    } else {
        compile_error!("No valid feature selected! Use 'dev', 'test', or 'prod'.");
    }
}
