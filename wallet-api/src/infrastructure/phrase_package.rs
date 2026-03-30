use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::{domain::api_wallet::unlock::SeedEnvelopeCodec, error::service::ServiceError};

const PHRASE_PACKAGE_PREFIX: &str = "wp1.";

pub(crate) struct PhrasePackageCodec;

impl PhrasePackageCodec {
    pub(crate) async fn encrypt_phrase(
        password: &str,
        phrase: &str,
    ) -> Result<String, ServiceError> {
        let phrase_package = SeedEnvelopeCodec::encrypt_seed_bundle(password, phrase.as_bytes())
            .await
            .map(|package| STANDARD.encode(package))?;
        Ok(format!("{PHRASE_PACKAGE_PREFIX}{phrase_package}"))
    }

    fn decode_phrase_package(phrase: &str) -> Result<Vec<u8>, ServiceError> {
        let Some(rest) = phrase.strip_prefix(PHRASE_PACKAGE_PREFIX) else {
            return Err(crate::error::service::ServiceError::System(
                crate::error::system::SystemError::Internal(
                    "unsupported phrase package".to_string(),
                ),
            ));
        };

        STANDARD.decode(rest).map_err(|err| {
            crate::error::service::ServiceError::System(
                crate::error::system::SystemError::Internal(err.to_string()),
            )
        })
    }

    pub(crate) async fn decrypt_phrase(
        password: &str,
        phrase: &str,
    ) -> Result<String, ServiceError> {
        let package = Self::decode_phrase_package(phrase)?;
        let data = SeedEnvelopeCodec::decrypt_seed_bundle(password, &package).await?;
        Ok(wallet_utils::conversion::vec_to_string(&data)?)
    }
}

#[cfg(test)]
mod tests {
    use super::PhrasePackageCodec;
    use std::sync::Once;

    static TEST_TRACING: Once = Once::new();

    const TEST_PASSWORD: &str = "test-password";

    fn init_test_tracing() {
        TEST_TRACING.call_once(|| {
            let _ = tracing_subscriber::fmt()
                .with_test_writer()
                .with_max_level(tracing::Level::INFO)
                .try_init();
        });
    }

    #[tokio::test]
    async fn phrase_package_roundtrip_logs() {
        init_test_tracing();
        let phrase = "phrase-package-roundtrip";

        let encoded = PhrasePackageCodec::encrypt_phrase(TEST_PASSWORD, phrase)
            .await
            .expect("encrypt phrase package");
        eprintln!(
            "[phrase-package] 1) stored phrase package (stored_len={}, opaque={}, prefix={})",
            encoded.len(),
            !encoded.starts_with('{'),
            encoded.get(..4).unwrap_or_default()
        );
        assert!(encoded.starts_with("wp1."));
        assert!(!encoded.starts_with('{'));

        let decoded = PhrasePackageCodec::decrypt_phrase(TEST_PASSWORD, &encoded)
            .await
            .expect("decrypt phrase package");
        eprintln!(
            "[phrase-package] 2) decrypted phrase roundtrip (plain_len={}, matches_expected={})",
            decoded.len(),
            decoded == phrase
        );
        assert_eq!(decoded, phrase);
    }
}
