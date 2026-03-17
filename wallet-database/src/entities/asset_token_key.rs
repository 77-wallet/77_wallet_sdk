use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum AssetTokenKey {
    #[default]
    Native,
    Contract(String),
}

impl AssetTokenKey {
    pub fn from_raw(token: Option<&str>) -> Self {
        match token.map(str::trim) {
            Some(token) if !token.is_empty() => Self::Contract(token.to_string()),
            _ => Self::Native,
        }
    }

    pub fn from_db_value(token: &str) -> Self {
        Self::from_raw(Some(token))
    }

    pub fn as_db_str(&self) -> &str {
        match self {
            Self::Native => "",
            Self::Contract(token) => token.as_str(),
        }
    }

    pub fn to_chain_token_option(&self) -> Option<String> {
        match self {
            Self::Native => None,
            Self::Contract(token) => {
                let normalized = token.trim();
                if normalized.is_empty() { None } else { Some(normalized.to_string()) }
            }
        }
    }

    pub fn into_chain_token_option(self) -> Option<String> {
        match self {
            Self::Native => None,
            Self::Contract(token) => {
                let normalized = token.trim();
                if normalized.is_empty() { None } else { Some(normalized.to_string()) }
            }
        }
    }

    pub fn to_api_token_option_legacy(&self) -> Option<String> {
        Some(self.as_db_str().to_string())
    }

    pub fn into_api_token_option_legacy(self) -> Option<String> {
        Some(match self {
            Self::Native => String::new(),
            Self::Contract(token) => token,
        })
    }

    pub fn to_option_string_for_api(&self) -> Option<String> {
        self.to_api_token_option_legacy()
    }

    pub fn into_option_string_for_api(self) -> Option<String> {
        self.into_api_token_option_legacy()
    }

    pub fn is_native(&self) -> bool {
        matches!(self, Self::Native)
    }

    pub fn is_contract(&self) -> bool {
        matches!(self, Self::Contract(_))
    }
}

impl std::fmt::Display for AssetTokenKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_db_str())
    }
}

impl From<Option<String>> for AssetTokenKey {
    fn from(value: Option<String>) -> Self {
        Self::from_raw(value.as_deref())
    }
}

impl From<String> for AssetTokenKey {
    fn from(value: String) -> Self {
        Self::from_raw(Some(value.as_str()))
    }
}

impl From<&str> for AssetTokenKey {
    fn from(value: &str) -> Self {
        Self::from_raw(Some(value))
    }
}

impl PartialEq<Option<String>> for AssetTokenKey {
    fn eq(&self, other: &Option<String>) -> bool {
        self == &AssetTokenKey::from(other.clone())
    }
}

impl Serialize for AssetTokenKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_db_str())
    }
}

impl<'de> Deserialize<'de> for AssetTokenKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = Option::<String>::deserialize(deserializer)?;
        Ok(Self::from_raw(raw.as_deref()))
    }
}

impl sqlx::Type<sqlx::Sqlite> for AssetTokenKey {
    fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
        <String as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Sqlite> for AssetTokenKey {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Sqlite as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        buf.push(sqlx::sqlite::SqliteArgumentValue::Text(self.as_db_str().to_string().into()));
        Ok(sqlx::encode::IsNull::No)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for AssetTokenKey {
    fn decode(
        value: <sqlx::Sqlite as sqlx::Database>::ValueRef<'r>,
    ) -> Result<Self, sqlx::error::BoxDynError> {
        let raw = <String as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        Ok(Self::from_db_value(&raw))
    }
}

#[cfg(test)]
mod tests {
    use super::AssetTokenKey;

    #[test]
    fn asset_token_key_normalizes_native_inputs() {
        assert_eq!(AssetTokenKey::from_raw(None), AssetTokenKey::Native);
        assert_eq!(AssetTokenKey::from_raw(Some("")), AssetTokenKey::Native);
        assert_eq!(AssetTokenKey::from_raw(Some("   ")), AssetTokenKey::Native);
    }

    #[test]
    fn asset_token_key_normalizes_contract_inputs() {
        assert_eq!(
            AssetTokenKey::from_raw(Some(" 0xabc ")),
            AssetTokenKey::Contract("0xabc".to_string())
        );
    }

    #[test]
    fn asset_token_key_keeps_db_encoding_compatible() {
        assert_eq!(AssetTokenKey::Native.as_db_str(), "");
        assert_eq!(AssetTokenKey::Contract("0xabc".to_string()).as_db_str(), "0xabc");
    }

    #[test]
    fn asset_token_key_chain_option_normalizes_blank_contract() {
        assert_eq!(AssetTokenKey::Contract("".to_string()).to_chain_token_option(), None);
        assert_eq!(AssetTokenKey::Contract("   ".to_string()).to_chain_token_option(), None);
    }
}
