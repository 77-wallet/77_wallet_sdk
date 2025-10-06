use super::account::{BalanceInfo, BalanceNotTruncate, default_unit_price_as_zero};
use crate::request::transaction::Signer;
use alloy::primitives::U256;
use wallet_chain_interact::{
    BillResourceConsume,
    eth::{self, FeeSetting},
    tron,
};
use wallet_database::entities::{
    bill::{BillEntity, BillKind},
    multisig_queue::{MemberSignedResult, MultisigQueueStatus, NewMultisigQueueEntity},
};
use wallet_transport_backend::response_vo::{chain::GasOracle, coin::TokenCurrency};
use wallet_utils::{serde_func, unit};
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionResult {
    pub tx_hash: String,
}

#[derive(Debug, serde::Serialize)]
pub struct MultisigQueueFeeParams {
    pub from: String,
    pub to: String,
    pub value: String,
    pub chain_code: String,
    pub symbol: String,
    pub token_address: Option<String>,
    pub spend_all: Option<bool>,
}

#[derive(Debug, serde::Serialize)]
pub struct TransferParams {
    pub from: String,
    pub to: String,
    pub value: String,
    pub expiration: Option<i64>,
    pub chain_code: String,
    pub symbol: String,
    pub token_address: Option<String>,
    pub notes: Option<String>,
    pub spend_all: bool,
    pub signer: Option<Signer>,
}

impl From<&TransferParams> for NewMultisigQueueEntity {
    fn from(value: &TransferParams) -> Self {
        let notes = value.notes.clone().unwrap_or_default();

        let now = wallet_utils::time::now().timestamp();
        let expiration = value.expiration.unwrap_or(1) * 3600 + now;

        Self {
            id: "".to_string(),
            from_addr: value.from.clone(),
            to_addr: value.to.clone(),
            value: value.value.clone(),
            symbol: value.symbol.clone(),
            expiration,
            chain_code: value.chain_code.to_string(),
            token_addr: None,
            msg_hash: "".to_string(),
            tx_hash: "".to_string(),
            raw_data: "".to_string(),
            notes,
            status: MultisigQueueStatus::PendingSignature,
            signatures: vec![],
            account_id: "".to_string(),
            fail_reason: "".to_string(),
            create_at: wallet_utils::time::now(),
            transfer_type: BillKind::Transfer,
            permission_id: "".to_string(),
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BillDetailVo {
    #[serde(flatten)]
    pub bill: BillEntity,
    pub resource_consume: Option<BillResourceConsume>,
    pub fee_symbol: String,
    pub signature: Option<Vec<MemberSignedResult>>,
    pub wallet_name: String,
    pub account_name: String,
}

impl BillDetailVo {
    pub fn new(
        bill: BillEntity,
        fee_symbol: String,
        signature: Option<Vec<MemberSignedResult>>,
    ) -> Result<Self, crate::error::service::ServiceError> {
        let resource_consume = if !bill.resource_consume.is_empty() && bill.resource_consume != "0"
        {
            Some(BillResourceConsume::from_json_str(&bill.resource_consume)?)
        } else {
            None
        };

        Ok(Self {
            bill,
            resource_consume,
            fee_symbol,
            signature,
            wallet_name: "".to_string(),
            account_name: "".to_string(),
        })
    }
}

// about fee estimate fee
#[derive(Debug, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EstimateFeeResp {
    pub symbol: String,
    pub chain_code: String,
    pub content: String,
}
impl EstimateFeeResp {
    pub fn new(symbol: String, chain_code: String, content: String) -> Self {
        Self { symbol, chain_code, content }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeDetails<T>(Vec<FeeStructure<T>>);
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeDetailsVo<T> {
    default: String,
    pub data: Vec<FeeStructureVo<T>>,
}

impl FeeDetails<EthereumFeeDetails> {
    pub fn to_resp(
        self,
        token_price: TokenCurrency,
        currency: &str,
    ) -> FeeDetailsVo<EthereumFeeDetails> {
        let mut res = vec![];

        let unit_price = token_price.get_price(currency);
        for fee in self.0 {
            let fee_structure = FeeStructureVo {
                types: fee.types,
                estimate_fee: BalanceInfo::new(fee.estimate_fee, unit_price, currency),
                max_fee: BalanceInfo::new(fee.max_fee, unit_price, currency),
                fee_setting: fee.fee_setting,
            };
            res.push(fee_structure);
        }
        FeeDetailsVo { default: "propose".to_string(), data: res }
    }
}

impl TryFrom<(GasOracle, i64)> for FeeDetails<EthereumFeeDetails> {
    type Error = crate::error::service::ServiceError;
    fn try_from((gas_oracle, gas_limit): (GasOracle, i64)) -> Result<Self, Self::Error> {
        let base =
            unit::convert_to_u256(&gas_oracle.suggest_base_fee.unwrap_or("0".to_string()), 9)?;

        // unit default is gwei , and to wei
        let mut res = vec![];

        if let Some(safe_gas_price) = gas_oracle.safe_gas_price {
            let safe = unit::convert_to_u256(&safe_gas_price, 9)?;
            let priority_fee = safe - base;
            let sales_fee = FeeStructure::new(gas_limit, base, priority_fee, "safe")?;
            res.push(sales_fee);
        }

        if let Some(propose_gas_price) = gas_oracle.propose_gas_price {
            let propose = unit::convert_to_u256(&propose_gas_price, 9)?;
            let priority_fee = propose - base;
            let propose_fee = FeeStructure::new(gas_limit, base, priority_fee, "propose")?;
            res.push(propose_fee);
        }

        if let Some(fast_gas_price) = gas_oracle.fast_gas_price {
            let fast = unit::convert_to_u256(&fast_gas_price, 9)?;
            let priority_fee = fast - base;
            let fast_fee = FeeStructure::new(gas_limit, base, priority_fee, "fast")?;
            res.push(fast_fee);
        }

        Ok(Self(res))
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeStructure<T> {
    #[serde(rename = "type")]
    pub types: String,
    pub estimate_fee: f64,
    pub max_fee: f64,
    pub fee_setting: T,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeStructureVo<T> {
    #[serde(rename = "type")]
    pub types: String,
    pub estimate_fee: BalanceInfo,
    pub max_fee: BalanceInfo,
    pub fee_setting: T,
}

impl FeeStructure<EthereumFeeDetails> {
    pub fn new(
        gas_limit: i64,
        base_fee: U256,
        mut priority_fee: U256,
        types: &str,
    ) -> Result<Self, crate::error::service::ServiceError> {
        let base_plus_tip = base_fee + priority_fee;

        // 1.2 倍
        let mut max_fee_per_gas = base_plus_tip * U256::from(12) / U256::from(10);

        // 向上取整到 0.1 gwei（1e8 wei）
        let scale = U256::from(10).pow(U256::from(8)); // 0.1 gwei
        max_fee_per_gas = ((max_fee_per_gas + (scale - U256::from(1))) / scale) * scale;
        max_fee_per_gas = max_fee_per_gas.max(base_plus_tip);

        // 避免节点最小费用拦截
        if priority_fee == U256::ZERO {
            priority_fee = U256::from(10000);
        }

        let fee_setting = EthereumFeeDetails::new(
            gas_limit,
            base_fee.to_string(),
            priority_fee.to_string(),
            max_fee_per_gas.to_string(),
        );

        let gas_limit = U256::from(gas_limit);
        let max_fee = gas_limit * max_fee_per_gas;

        let estimate_fee = gas_limit * (base_fee + priority_fee);

        Ok(Self {
            estimate_fee: unit::format_to_f64(estimate_fee, eth::consts::ETH_DECIMAL)?,
            max_fee: unit::format_to_f64(max_fee, eth::consts::ETH_DECIMAL)?,
            types: types.to_string(),
            fee_setting,
        })
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EthereumFeeDetails {
    pub gas_limit: i64,
    pub base_fee: String,
    pub priority_fee: String,
    pub max_fee_per_gas: String,
}

impl TryFrom<&str> for EthereumFeeDetails {
    type Error = crate::error::service::ServiceError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let rs: Self = serde_func::serde_from_str(value)?;
        Ok(rs)
    }
}

impl TryFrom<EthereumFeeDetails> for wallet_chain_interact::eth::FeeSetting {
    type Error = crate::error::service::ServiceError;

    fn try_from(value: EthereumFeeDetails) -> Result<Self, Self::Error> {
        Ok(Self {
            gas_limit: U256::from(value.gas_limit),
            base_fee: unit::u256_from_str(&value.base_fee)?,
            max_priority_fee_per_gas: unit::u256_from_str(&value.priority_fee)?,
            max_fee_per_gas: unit::u256_from_str(&value.max_fee_per_gas)?,
        })
    }
}
impl EthereumFeeDetails {
    pub fn new(
        gas_limit: i64,
        base_fee: String,
        priority_fee: String,
        max_fee_per_gas: String,
    ) -> Self {
        Self { gas_limit, base_fee, priority_fee, max_fee_per_gas }
    }
}
impl From<FeeSetting> for EthereumFeeDetails {
    fn from(value: FeeSetting) -> Self {
        Self {
            gas_limit: value.gas_limit.to::<u64>() as i64,
            base_fee: value.base_fee.to_string(),
            priority_fee: value.max_priority_fee_per_gas.to_string(),
            max_fee_per_gas: value.max_fee_per_gas.to_string(),
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TronFeeDetails {
    pub estimate_fee: BalanceInfo,
    pub energy: u64,
    pub user_energy: u64,
    pub energy_price: f64,
    pub bandwidth: u64,
    pub user_bandwidth: u64,
    pub bandwidth_price: f64,
    pub extra_fee: f64,
}
impl TronFeeDetails {
    pub fn new(
        consumer: tron::params::ResourceConsumer,
        token_currency: TokenCurrency,
        currency: &str,
    ) -> Result<Self, crate::error::service::ServiceError> {
        let amount = consumer.transaction_fee();
        let amount = unit::string_to_f64(&amount)?;
        let fee = BalanceInfo::new(amount, token_currency.get_price(currency), currency);

        let (energy, energy_limit, energy_price) = if let Some(energy) = consumer.energy {
            (energy.consumer, energy.limit, energy.price as f64 / tron::consts::TRX_TO_SUN as f64)
        } else {
            (0, 0, 0.0)
        };

        Ok(Self {
            estimate_fee: fee,
            energy: energy as u64,
            user_energy: energy_limit as u64,
            energy_price,
            bandwidth: consumer.bandwidth.consumer as u64,
            user_bandwidth: consumer.bandwidth.limit as u64,
            bandwidth_price: consumer.bandwidth.price as f64 / tron::consts::TRX_TO_SUN as f64,
            extra_fee: consumer.extra_fee as f64 / tron::consts::TRX_TO_SUN as f64,
        })
    }
}

#[derive(Debug, serde::Serialize)]
pub struct BitcoinFeeDetails {
    pub fee_rate: f64,
    pub size: i64,
}

// 目前在多签交易的时候使用,以及不需要显示块中慢
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommonFeeDetails {
    pub estimate_fee: BalanceNotTruncate,
}

impl CommonFeeDetails {
    // fee unit is format
    pub fn new(
        fee: f64,
        token_currency: TokenCurrency,
        currency: &str,
    ) -> Result<Self, crate::error::service::ServiceError> {
        let amount = wallet_utils::conversion::decimal_from_f64(fee).unwrap_or_default();

        let unit_pice = token_currency.get_price(currency);

        let unit_price =
            unit_pice.map(|p| wallet_utils::conversion::decimal_from_f64(p)).transpose()?;

        Ok(Self { estimate_fee: BalanceNotTruncate::new(amount, unit_price, currency)? })
    }

    pub fn to_json_str(&self) -> Result<String, crate::error::service::ServiceError> {
        Ok(wallet_utils::serde_func::serde_to_string(&self)?)
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoinCurrency {
    pub currency: String,
    #[serde(serialize_with = "default_unit_price_as_zero")]
    pub unit_price: Option<f64>,
}
