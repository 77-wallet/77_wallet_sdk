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
        pub const SIGNED_TRAN_CREATE: &str = "signed/trans/create";

        // Sign a multisig transaction
        pub const SIGNED_TRAN_ACCEPT: &str = "signed/trans/accept";

        // Report the transaction hash of the executed multisig to the backend
        pub const SIGNED_TRAN_UPDATE_TRANS_HASH: &str = "signed/trans/updateTransdHash";
    }

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

    pub const ADDRESS_FIND_MULTI_SIGNED_DETAILS: &str = "address/findMultiSignedDetails";
}

// /// 测试环境
// #[cfg(feature = "test")]
// pub const BASE_URL: &str = "https://api.puke668.top";
// #[cfg(feature = "test")]
// pub const MQTT_URL: &str = "mqtt://126.214.108.58:11883";

// //开发环境
// // #[cfg(not(feature = "test"))]
// // pub const BASE_URL: &str = "https://api.puke668.top";
// // #[cfg(not(feature = "test"))]
// // pub const MQTT_URL: &str = "mqtt://126.214.108.58:11883";

// #[cfg(not(feature = "test"))]
// pub const BASE_URL: &str = "https://walletapi.puke668.top";
// #[cfg(not(feature = "test"))]
// pub const MQTT_URL: &str = "mqtt://100.106.144.126:1883";

// 代理的全局rpc节点
pub const BASE_RPC_URL: &str = "rpc.88ai.fun";

// 开发环境
#[cfg(feature = "dev")]
pub const BASE_URL: &str = "https://walletapi.puke668.top";
#[cfg(feature = "dev")]
pub const MQTT_URL: &str = "mqtt://100.106.144.126:1883";

// 测试环境
#[cfg(feature = "test")]
pub const BASE_URL: &str = "https://api.puke668.top";
#[cfg(feature = "test")]
pub const MQTT_URL: &str = "mqtt://126.214.108.58:11883";

// 生产环境
#[cfg(feature = "prod")]
pub const BASE_URL: &str = "https://api.77wallet.org";
#[cfg(feature = "prod")]
pub const MQTT_URL: &str = "mqtt://100.106.144.126:1883";
