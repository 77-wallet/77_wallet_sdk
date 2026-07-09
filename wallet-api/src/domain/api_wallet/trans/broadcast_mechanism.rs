use crate::{domain::api_wallet::adapter::tx::RawTx, error::service::ServiceError};
use sha3::{Digest, Keccak256};

pub(crate) fn evm_raw_hash_hint(raw: &[u8]) -> String {
    let digest = Keccak256::digest(raw);
    format!("0x{}", hex::encode(digest))
}

pub(crate) fn normalize_expected_tx_hash(expected_tx_hash: Option<&str>) -> Option<String> {
    expected_tx_hash.map(str::trim).filter(|s| !s.is_empty()).map(ToOwned::to_owned)
}

pub(crate) fn select_broadcast_tx_hash_hint(
    raw: &RawTx,
    expected_tx_hash: Option<&str>,
) -> Option<String> {
    match raw {
        RawTx::Tron(raw, ..) => Some(raw.tx_id.clone()),
        RawTx::Evm(raw, ..) => Some(evm_raw_hash_hint(raw)),
        // SOL duplicate 场景必须使用构建阶段持久化的 tx_hash；
        // RawTx::Sol 第一字段是序列化串，不是可查询签名。
        RawTx::Sol(..) => normalize_expected_tx_hash(expected_tx_hash),
    }
}

pub(crate) fn resolve_duplicate_tx_hash(
    expected_tx_hash: Option<&str>,
    fallback_hint: Option<&str>,
) -> Option<String> {
    normalize_expected_tx_hash(expected_tx_hash).or_else(|| fallback_hint.map(ToOwned::to_owned))
}

pub(crate) fn clone_raw_for_single_retry(raw: &RawTx) -> Option<RawTx> {
    match raw {
        RawTx::Evm(raw, fee) => Some(RawTx::Evm(raw.clone(), *fee)),
        RawTx::Sol(sig, serialized) => Some(RawTx::Sol(sig.clone(), serialized.clone())),
        RawTx::Tron(..) => None,
    }
}

pub(crate) fn is_duplicate_broadcast_error(err: &ServiceError) -> bool {
    let s = err.to_string().to_ascii_lowercase();
    // Tron (nileex) observed:
    // - "rpc error Transaction already exists."
    // - "rpc error Dup transaction."
    //
    // EVM common patterns:
    // - "known transaction"
    // - "already known"
    //
    // Solana observed patterns:
    // - "Transaction simulation failed: This transaction has already been processed"
    // - "Transaction simulation failed: This transn has already been processed"
    //
    // These indicate the tx is already accepted/seen by the node; treat broadcast as idempotent.
    (s.contains("transaction") && (s.contains("already exists") || s.contains("already known")))
        || s.contains("known transaction")
        || s.contains("dup transaction")
        || s.contains("duplicate transaction")
        || s.contains("already been processed")
        || s.contains("has already been processed")
}

pub(crate) fn is_evm_chain_code(chain_code: &str) -> bool {
    chain_code.eq_ignore_ascii_case("eth") || chain_code.eq_ignore_ascii_case("bnb")
}

pub(crate) fn is_nonce_too_low_error(err: &ServiceError) -> bool {
    let s = err.to_string().to_ascii_lowercase();
    s.contains("nonce too low")
        || s.contains("nonce is too low")
        || s.contains("invalid nonce: too low")
        || s.contains("nonce already used")
        || s.contains("nonce has already been used")
        || s.contains("invalid nonce: already used")
}

pub(crate) fn should_treat_nonce_too_low_as_broadcast_success(
    chain_code: &str,
    err: &ServiceError,
    has_raw_tx: bool,
    has_tx_hash: bool,
) -> bool {
    has_raw_tx && has_tx_hash && is_evm_chain_code(chain_code) && is_nonce_too_low_error(err)
}

pub(crate) fn is_blockhash_not_found_error(err: &ServiceError) -> bool {
    let s = err.to_string().to_ascii_lowercase();
    s.contains("blockhash not found") || s.contains("block hash not found")
}

pub(crate) fn is_transaction_expired_error(err: &ServiceError) -> bool {
    err.to_string().to_ascii_lowercase().contains("transaction expired")
}

pub(crate) fn should_invalidate_tron_expired_broadcast(
    chain_code: &str,
    err: &ServiceError,
    has_raw_tx: bool,
    has_tx_hash: bool,
    last_broadcast_at_present: bool,
) -> bool {
    chain_code.eq_ignore_ascii_case("tron")
        && is_transaction_expired_error(err)
        && has_raw_tx
        && has_tx_hash
        && !last_broadcast_at_present
}

pub(crate) fn is_recoverable_sol_broadcast_error(_err: &ServiceError) -> bool {
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BroadcastFailureFacts<'a> {
    pub(crate) chain_code: &'a str,
    pub(crate) raw_tx_present: bool,
    pub(crate) tx_hash_present: bool,
    pub(crate) last_broadcast_at_present: bool,
    pub(crate) broadcast_stage: bool,
}

impl<'a> BroadcastFailureFacts<'a> {
    pub(crate) fn new(
        chain_code: &'a str,
        raw_tx_present: bool,
        tx_hash_present: bool,
        last_broadcast_at_present: bool,
        broadcast_stage: bool,
    ) -> Self {
        Self {
            chain_code,
            raw_tx_present,
            tx_hash_present,
            last_broadcast_at_present,
            broadcast_stage,
        }
    }

    pub(crate) fn broadcast_stage(
        chain_code: &'a str,
        raw_tx_present: bool,
        tx_hash_present: bool,
        last_broadcast_at_present: bool,
    ) -> Self {
        Self::new(chain_code, raw_tx_present, tx_hash_present, last_broadcast_at_present, true)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BroadcastFailureDecision {
    RebuildSolBlockhash,
    RebuildTronExpired,
    MarkBroadcastExecutedForDuplicate,
    MarkBroadcastExecutedForNonceTooLow,
    FailAfterNonceTooLowSelfHeal,
    Unhandled,
}

pub(crate) fn classify_broadcast_failure(
    err: &ServiceError,
    facts: BroadcastFailureFacts<'_>,
) -> BroadcastFailureDecision {
    if facts.broadcast_stage
        && facts.chain_code.eq_ignore_ascii_case("sol")
        && is_blockhash_not_found_error(err)
        && facts.raw_tx_present
        && facts.tx_hash_present
        && !facts.last_broadcast_at_present
    {
        return BroadcastFailureDecision::RebuildSolBlockhash;
    }

    if facts.broadcast_stage
        && should_invalidate_tron_expired_broadcast(
            facts.chain_code,
            err,
            facts.raw_tx_present,
            facts.tx_hash_present,
            facts.last_broadcast_at_present,
        )
    {
        return BroadcastFailureDecision::RebuildTronExpired;
    }

    if facts.broadcast_stage
        && is_duplicate_broadcast_error(err)
        && facts.raw_tx_present
        && facts.tx_hash_present
    {
        return BroadcastFailureDecision::MarkBroadcastExecutedForDuplicate;
    }

    if facts.broadcast_stage
        && should_treat_nonce_too_low_as_broadcast_success(
            facts.chain_code,
            err,
            facts.raw_tx_present,
            facts.tx_hash_present,
        )
    {
        return BroadcastFailureDecision::MarkBroadcastExecutedForNonceTooLow;
    }

    if is_nonce_too_low_error(err) {
        return BroadcastFailureDecision::FailAfterNonceTooLowSelfHeal;
    }

    BroadcastFailureDecision::Unhandled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_processed_should_be_treated_as_duplicate() {
        let err = ServiceError::Parameter(
            "Transaction simulation failed: This transn has already been processed".to_string(),
        );
        assert!(is_duplicate_broadcast_error(&err));
        assert!(!is_recoverable_sol_broadcast_error(&err));
    }

    #[test]
    fn blockhash_not_found_should_not_be_recoverable_simulation_failure() {
        let err = ServiceError::Parameter(
            "node response: Transaction simulation failed: Blockhash not found".to_string(),
        );
        assert!(is_blockhash_not_found_error(&err));
        assert!(!is_recoverable_sol_broadcast_error(&err));
    }

    #[test]
    fn tron_transaction_expired_error_is_detected() {
        let err = ServiceError::Parameter(
            "Chain interact error: `rpc error Transaction expired`".to_string(),
        );
        assert!(is_transaction_expired_error(&err));
        assert!(!is_blockhash_not_found_error(&err));
    }

    #[test]
    fn tron_transaction_expired_invalidates_raw_for_rebuild() {
        let err = ServiceError::Parameter(
            "Chain interact error: `rpc error Transaction expired`".to_string(),
        );

        assert!(should_invalidate_tron_expired_broadcast("tron", &err, true, true, false,));
        assert!(!should_invalidate_tron_expired_broadcast("tron", &err, true, true, true,));
        assert!(!should_invalidate_tron_expired_broadcast("tron", &err, true, false, false,));
        assert!(!should_invalidate_tron_expired_broadcast("sol", &err, true, true, false,));
    }

    #[test]
    fn rent_shortage_should_not_be_recoverable_simulation_failure() {
        let err = ServiceError::Parameter(
            "node response: Transaction simulation failed: Transaction results in an account (0) with insufficient funds for rent"
                .to_string(),
        );
        assert!(!is_recoverable_sol_broadcast_error(&err));
    }

    #[test]
    fn generic_sol_simulation_failure_should_fail_fast() {
        let err = ServiceError::Parameter(
            "node response: Transaction simulation failed: custom program error: 0x1".to_string(),
        );
        assert!(!is_recoverable_sol_broadcast_error(&err));
    }

    #[test]
    fn duplicate_tx_hash_should_prefer_expected_hash() {
        let picked = resolve_duplicate_tx_hash(Some("  expected_tx_hash  "), Some("fallback"));
        assert_eq!(picked.as_deref(), Some("expected_tx_hash"));
    }

    #[test]
    fn sol_tx_hash_hint_must_not_use_raw_sol_payload_field() {
        let raw = RawTx::Sol("not_a_signature".to_string(), "serialized_or_fee".to_string());
        let hint = select_broadcast_tx_hash_hint(&raw, Some("real_sol_signature"));
        assert_eq!(hint.as_deref(), Some("real_sol_signature"));

        let missing = select_broadcast_tx_hash_hint(&raw, None);
        assert!(missing.is_none());
    }

    #[test]
    fn evm_nonce_too_low_with_local_tx_state_should_be_treated_as_broadcast_success() {
        let err = ServiceError::Parameter(
            "node response: Node response error: code=-32000, rpc=https://example, message=Some(\"nonce too low: next nonce 3, tx nonce 2\")".to_string(),
        );

        assert!(should_treat_nonce_too_low_as_broadcast_success("eth", &err, true, true,));
    }

    #[test]
    fn nonce_too_low_without_local_tx_state_should_not_be_treated_as_broadcast_success() {
        let err = ServiceError::Parameter(
            "node response: Node response error: code=-32000, rpc=https://example, message=Some(\"nonce too low: next nonce 3, tx nonce 2\")".to_string(),
        );

        assert!(!should_treat_nonce_too_low_as_broadcast_success("eth", &err, false, true,));
        assert!(!should_treat_nonce_too_low_as_broadcast_success("sol", &err, true, true,));
    }

    #[test]
    fn classify_broadcast_failure_prefers_rebuildable_sol_blockhash() {
        let err = ServiceError::Parameter(
            "node response: Transaction simulation failed: Blockhash not found".to_string(),
        );

        let decision = classify_broadcast_failure(
            &err,
            BroadcastFailureFacts::broadcast_stage("sol", true, true, false),
        );

        assert_eq!(decision, BroadcastFailureDecision::RebuildSolBlockhash);
    }

    #[test]
    fn classify_broadcast_failure_keeps_nonce_failure_available_outside_broadcast_stage() {
        let err = ServiceError::Parameter(
            "node response: Node response error: code=-32000, message=nonce too low".to_string(),
        );

        let decision = classify_broadcast_failure(
            &err,
            BroadcastFailureFacts::new("eth", true, true, false, false),
        );

        assert_eq!(decision, BroadcastFailureDecision::FailAfterNonceTooLowSelfHeal);
    }

    #[test]
    fn classify_broadcast_failure_marks_duplicate_only_with_local_tx_facts() {
        let err = ServiceError::Parameter("rpc error Transaction already exists.".to_string());

        let decision = classify_broadcast_failure(
            &err,
            BroadcastFailureFacts::broadcast_stage("tron", true, true, false),
        );

        assert_eq!(decision, BroadcastFailureDecision::MarkBroadcastExecutedForDuplicate);
    }
}
