use crate::constant::btc_address_type::*;

use super::category::BtcAddressCategory;

use once_cell::sync::Lazy;

pub static BTC_ADDRESS_TYPES: Lazy<Vec<AddressType>> = Lazy::new(|| {
    vec![
        AddressType::Btc(BtcAddressType::P2wpkh),
        AddressType::Btc(BtcAddressType::P2shWpkh),
        AddressType::Btc(BtcAddressType::P2tr),
        AddressType::Btc(BtcAddressType::P2pkh),
    ]
});

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Copy)]
#[serde(untagged)]
pub enum AddressType {
    Btc(BtcAddressType),
    Other,
}

// impl AddressType {
//     pub fn get_btc_address_types() -> Vec<AddressType> {
//         BTC_ADDRESS_TYPES.to_vec()
//     }
// }

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Copy)]
pub enum BtcAddressType {
    /// Pay to public hash (legacy)
    P2pkh,
    /// Pay to script hash
    P2sh,
    /// bech32（Pay to public hash）
    P2shWpkh,
    /// 隔离见证（兼容）
    P2shWsh,
    /// 隔离见证（原生）
    P2wpkh,
    P2wsh,
    /// taproot 单签
    P2tr,
    /// taproot 多签
    P2trSh,
}

impl AsRef<str> for BtcAddressType {
    fn as_ref(&self) -> &str {
        match self {
            BtcAddressType::P2pkh => P2PKH,
            BtcAddressType::P2sh => P2SH,
            BtcAddressType::P2shWpkh => P2SH_WPKH,
            BtcAddressType::P2shWsh => P2SH_WSH,
            BtcAddressType::P2wpkh => P2WPKH,
            BtcAddressType::P2wsh => P2WSH,
            BtcAddressType::P2tr => P2TR,
            BtcAddressType::P2trSh => P2TR_SH,
        }
    }
}

impl From<BtcAddressCategory> for BtcAddressType {
    fn from(addr_scheme: BtcAddressCategory) -> Self {
        match addr_scheme {
            BtcAddressCategory::Legacy => BtcAddressType::P2sh,
            BtcAddressCategory::NestedSegWit => BtcAddressType::P2shWsh,
            BtcAddressCategory::NativeSegWit => BtcAddressType::P2wsh,
            BtcAddressCategory::Taproot => BtcAddressType::P2trSh,
        }
    }
}

impl std::fmt::Display for AddressType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AddressType::Btc(btc_address_type) => write!(f, "{}", btc_address_type),
            AddressType::Other => write!(f, ""),
        }
    }
}

impl std::fmt::Display for BtcAddressType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

impl TryFrom<Option<String>> for AddressType {
    type Error = crate::Error;
    fn try_from(value: Option<String>) -> Result<Self, Self::Error> {
        match value {
            Some(v) => BtcAddressType::try_from(v.as_str()).map(AddressType::Btc),
            None => Ok(AddressType::Other),
        }
    }
}

impl AsRef<str> for AddressType {
    fn as_ref(&self) -> &str {
        match self {
            AddressType::Btc(btc_address_type) => btc_address_type.as_ref(),
            AddressType::Other => "",
        }
    }
}

impl<T: AsRef<str>> TryFrom<Option<T>> for BtcAddressType {
    type Error = crate::Error;
    fn try_from(value: Option<T>) -> Result<Self, Self::Error> {
        match value {
            Some(v) => BtcAddressType::try_from(v.as_ref()),
            None => Err(crate::Error::BtcNeedAddressType),
        }
    }
}

impl TryFrom<&str> for BtcAddressType {
    type Error = crate::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(match value.to_lowercase().as_ref() {
            P2PKH => BtcAddressType::P2pkh,
            P2SH => BtcAddressType::P2sh,
            P2SH_WPKH => BtcAddressType::P2shWpkh,
            P2SH_WSH => BtcAddressType::P2shWsh,
            P2WPKH => BtcAddressType::P2wpkh,
            P2WSH => BtcAddressType::P2wsh,
            P2TR => BtcAddressType::P2tr,
            P2TR_SH => BtcAddressType::P2trSh,
            other => return Err(crate::Error::BtcAddressTypeInvalid(other.to_string())),
        })
    }
}
