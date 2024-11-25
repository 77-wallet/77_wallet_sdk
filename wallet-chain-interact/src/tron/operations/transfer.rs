use super::{
    contract::{ConstantContract, TriggerContractParameter},
    Contract, Parameter, RawData, RawTransactionParams, TronConstantOperation,
    TronSimulateOperation, TronTxOperation,
};
use crate::{
    abi_encode_address, abi_encode_u256,
    tron::{
        operations::TronTransactionResponse, params::ResourceConsumer,
        protocol::protobuf::transaction::Raw, provider::Provider,
    },
};
use alloy::primitives::U256;
use anychain_core::Transaction as _;
use wallet_utils::address;

/// transfer parameter
#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct TransferOpt {
    pub from: String,
    pub to: String,
    pub value: i64,
    pub signature_num: u8,
    pub memo: Option<String>,
}

impl TransferOpt {
    pub fn new(
        from: &str,
        to: &str,
        value: U256,
        _memo: Option<String>,
    ) -> Result<Self, crate::Error> {
        Ok(Self {
            from: address::bs58_addr_to_hex(from)?,
            to: address::bs58_addr_to_hex(to)?,
            value: value.to::<i64>(),
            signature_num: 1,
            memo: None,
        })
    }

    pub fn set_sign_num(&mut self, sign_num: u8) {
        self.signature_num = sign_num;
    }
}

/// tron create transfer transaction  response
#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct TronTransferResp {
    pub amount: i64,
    pub owner_address: String,
    pub to_address: String,
    #[serde(rename = "Permission_id")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_id: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_data: Option<String>,
}

#[async_trait::async_trait]
impl TronTxOperation<TronTransferResp> for TransferOpt {
    async fn build_raw_transaction(
        &self,
        provider: &Provider,
    ) -> crate::Result<RawTransactionParams> {
        let params = TronTransferResp {
            amount: self.value,
            owner_address: self.from.clone(),
            to_address: self.to.clone(),
            permission_id: None,
            extra_data: self.memo.clone(),
        };

        let res = provider.create_transaction(params).await?;
        Ok(RawTransactionParams::from(res))
    }

    fn get_to(&self) -> String {
        self.to.clone()
    }
}

impl TronSimulateOperation<TronTransferResp> for TransferOpt {
    fn simulate_raw_transaction(&self) -> crate::Result<RawTransactionParams> {
        let ct = anychain_tron::trx::build_transfer_contract(
            &self.from,
            &self.to,
            &self.value.to_string(),
        )
        .map_err(|e| {
            crate::Error::Other(format!("any chian tron build contract tx error: {}", e))
        })?;

        let mut param = anychain_tron::TronTransactionParameters::default();
        param.set_timestamp(anychain_tron::trx::timestamp_millis());
        param.set_ref_block(
            27007120,
            "00000000019c1890f87d110a81d815b9a38a3e62d44a00a7c8fd50a7b322a2df",
        );
        param.set_contract(ct);
        let transaction = anychain_tron::TronTransaction::new(&param)
            .map_err(|e| crate::Error::Other(format!("any chian tron build tx error: {}", e)))?;

        let raw_data_hex = transaction.to_bytes().map_err(|e| {
            crate::Error::Other(format!("any chain tron transaction to bytes error: {}", e))
        })?;

        let value = TronTransferResp {
            amount: self.value,
            owner_address: self.from.clone(),
            to_address: self.to.clone(),
            permission_id: None,
            extra_data: self.memo.clone(),
        };

        let contract = Contract {
            parameter: Parameter {
                value,
                type_url: "type.googleapis.com/protocol.TransferContract".to_string(),
            },
            types: "TransferContract".to_string(),
            permission_id: None,
        };

        let raw = RawData {
            contract: vec![contract],
            ref_block_bytes: "cc46".to_string(),
            ref_block_hash: "86013f30ec6d034b".to_string(),
            expiration: 1719569763000,
            fee_limit: None,
            data: self.memo.clone(),
            timestamp: 1719569703563,
        };

        let tx_resp = TronTransactionResponse {
            visible: false,
            tx_id: "4c3a90b598ae28174617a5960bf9d38500ac1ec5e57e7679141b04cda3467f1f".to_string(),
            raw_data: raw,
            raw_data_hex: wallet_utils::hex_func::hex_encode(raw_data_hex),
            ext: None,
        };

        Ok(RawTransactionParams::from(tx_resp))
    }
}

/// token transfer parameters
#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct ContractTransferOpt {
    pub contract_address: String,
    pub owner_address: String,
    pub to: String,
    pub value: U256,
    pub signature_num: u8,
    pub fee_limit: Option<i64>,
    pub memo: Option<String>,
}

impl ContractTransferOpt {
    pub fn new(
        contract_address: &str,
        owner_address: &str,
        to: &str,
        value: U256,
        _memo: Option<String>,
    ) -> Result<Self, crate::Error> {
        Ok(Self {
            contract_address: address::bs58_addr_to_hex(contract_address)?,
            owner_address: address::bs58_addr_to_hex(owner_address)?,
            to: address::bs58_addr_to_hex(to)?,
            value,
            signature_num: 1,
            fee_limit: None,
            // memo: memo.map(|m| wallet_utils::hex_func::utf8_to_hex(&m)),
            memo: None,
        })
    }
    pub fn set_sign_num(&mut self, sign_num: u8) {
        self.signature_num = sign_num;
    }

    pub fn set_fee_limit(&mut self, resource: ResourceConsumer) {
        let fee_limit = resource.fee_limit();

        // 上浮动 10%
        self.fee_limit = Some(fee_limit + (fee_limit * 20 / 100));
    }

    pub fn transaction_params(&self) -> TriggerContractParameter {
        let function_selector = "transfer(address,uint256)";

        let parameter = format!(
            "{}{}",
            abi_encode_address(&self.to),
            abi_encode_u256(self.value)
        );

        let mut trigger = TriggerContractParameter::new(
            &self.contract_address,
            &self.owner_address,
            function_selector,
            parameter,
        );
        if let Some(fee_limit) = self.fee_limit {
            trigger.fee_limit = Some(fee_limit);
        }
        trigger
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct ContractTransferResp {
    pub data: String,
    pub owner_address: String,
    pub contract_address: String,
}

/// trigger smart contract
#[async_trait::async_trait]
impl TronTxOperation<ContractTransferResp> for ContractTransferOpt {
    async fn build_raw_transaction(
        &self,
        provider: &Provider,
    ) -> crate::Result<RawTransactionParams> {
        let memo = self.memo.clone();
        let params = self.transaction_params();

        let res = provider.trigger_smart_contract(params).await?;

        // 如果有订单备注的情况,需要将订单备注加data中并重新计算
        if let Some(memo) = memo {
            let mut raw_data = res.transaction.raw_data;
            raw_data.data = Some(memo.clone());

            // 获取原始数据,并加入订单备注
            let mut protobuf_raw = Raw::from_str(res.transaction.raw_data_hex.as_str())?;
            protobuf_raw.data = wallet_utils::hex_func::hex_decode(&memo)?;

            let bytes = protobuf_raw.to_bytes()?;

            let raw_params = RawTransactionParams {
                tx_id: Raw::tx_id(&bytes),
                raw_data: raw_data.to_json_string()?,
                raw_data_hex: Raw::raw_data_hex(&bytes),
                signature: vec![],
            };

            Ok(raw_params)
        } else {
            Ok(RawTransactionParams::from(res.transaction))
        }
    }

    fn get_to(&self) -> String {
        self.to.clone()
    }
}

/// only constant smart contract: to get contract information or estimate energy.
#[async_trait::async_trait]
impl TronConstantOperation<ContractTransferResp> for ContractTransferOpt {
    async fn constant_contract(
        &self,
        provider: &Provider,
    ) -> crate::Result<ConstantContract<ContractTransferResp>> {
        let mut result = provider
            .trigger_constant_contract(self.transaction_params())
            .await?;

        if let Some(memo) = self.memo.clone() {
            let mut protobuf_raw = Raw::from_str(&result.transaction.raw_data_hex)?;
            protobuf_raw.data = wallet_utils::hex_func::hex_decode(&memo)?;

            let bytes = protobuf_raw.to_bytes()?;

            result.transaction.tx_id = Raw::tx_id(&bytes);
            result.transaction.raw_data_hex = Raw::raw_data_hex(&bytes);
            result.transaction.raw_data.data = Some(memo.clone());

            Ok(result)
        } else {
            Ok(result)
        }
    }
}
