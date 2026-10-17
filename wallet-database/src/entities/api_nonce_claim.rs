//! Persisted nonce ownership and deterministic unsent failure reuse.

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NonceClaim {
    pub kind: String,
    pub nonce: i64,
    pub raw_tx: Option<String>,
    pub tx_hash: Option<String>,
    pub err_code: Option<i64>,
    pub err_msg: Option<String>,
    pub failure_stage: Option<i64>,
    pub last_broadcast_at: Option<String>,
    pub uncertain: bool,
    pub executed: bool,
    pub terminal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WithdrawNonceDecision {
    Ready(i64),
    Wait,
    TerminalGap(i64),
}

impl NonceClaim {
    pub fn is_reusable_failure(&self) -> bool {
        // Confirmed current error rows are deterministic. An unresolved send
        // marker overrides an error, including future manual-review reports.
        self.kind == "withdraw"
            && self.err_code.is_some_and(|code| code > 0)
            && self.terminal
            && self.last_broadcast_at.is_none()
            && !self.uncertain
            && !self.executed
    }
}

pub fn withdraw_nonce_decision(
    chain_next: i64,
    local_last: i64,
    claims: &[NonceClaim],
) -> WithdrawNonceDecision {
    use WithdrawNonceDecision::*;
    if chain_next < 0 {
        return Wait;
    }
    if local_last < chain_next {
        return Ready(chain_next);
    }
    let mut slots: std::collections::BTreeMap<i64, bool> = std::collections::BTreeMap::new();
    for claim in claims.iter().filter(|c| c.nonce >= chain_next && c.nonce <= local_last) {
        if !claim.is_reusable_failure() && (claim.terminal || claim.executed) {
            return Wait;
        }
        *slots.entry(claim.nonce).or_insert(true) &= claim.is_reusable_failure();
    }
    let Some(required) = local_last.checked_sub(chain_next).and_then(|n| n.checked_add(1)) else {
        return Wait;
    };
    if slots.len() as i64 != required {
        // A counter without corresponding signed facts is an unknown reservation,
        // not evidence that it may be reused. Keep legacy allocator guards.
        if !slots.contains_key(&local_last) {
            return Wait;
        }
        let mut next = chain_next;
        for (&nonce, &reusable) in &slots {
            if nonce > next {
                return Ready(next);
            }
            if reusable {
                return TerminalGap(nonce);
            }
            next = match next.checked_add(1) {
                Some(n) => n,
                None => return Wait,
            };
        }
        return Wait;
    }
    let highest_live = slots.iter().rev().find_map(|(n, reusable)| (!reusable).then_some(*n));
    let Some(highest_live) = highest_live else {
        return Ready(chain_next);
    };
    if let Some((nonce, _)) = slots.iter().find(|(n, reusable)| **n < highest_live && **reusable) {
        return TerminalGap(*nonce);
    }
    highest_live.checked_add(1).map(Ready).unwrap_or(Wait)
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    #[test]
    fn shared_nonce_recovery_selects_unsigned_five_before_saved_six() {
        let saved = NonceClaim {
            kind: "collect".into(),
            nonce: 6,
            raw_tx: Some("saved-six".into()),
            tx_hash: Some("hash-six".into()),
            err_code: None,
            err_msg: None,
            failure_stage: None,
            last_broadcast_at: Some("sent".into()),
            uncertain: true,
            executed: false,
            terminal: false,
        };
        assert_eq!(withdraw_nonce_decision(5, 6, &[saved]), WithdrawNonceDecision::Ready(5));
    }
}

#[cfg(test)]
mod recovery_guards {
    use super::*;
    fn claim(nonce: i64) -> NonceClaim {
        NonceClaim {
            kind: "fee".into(),
            nonce,
            raw_tx: Some("bytes".into()),
            tx_hash: Some("hash".into()),
            err_code: None,
            err_msg: None,
            failure_stage: None,
            last_broadcast_at: None,
            uncertain: false,
            executed: false,
            terminal: false,
        }
    }
    #[test]
    fn shared_nonce_recovery_unknown_counter_is_not_unsigned_evidence() {
        assert_eq!(withdraw_nonce_decision(5, 6, &[]), WithdrawNonceDecision::Wait);
        assert_eq!(withdraw_nonce_decision(5, 7, &[claim(6)]), WithdrawNonceDecision::Wait);
    }
    #[test]
    fn shared_nonce_recovery_keeps_signed_five_and_uncertain_six_owned() {
        let mut six = claim(6);
        six.uncertain = true;
        assert_eq!(
            withdraw_nonce_decision(5, 6, &[claim(5), six]),
            WithdrawNonceDecision::Ready(7)
        );
    }
    #[test]
    fn shared_nonce_recovery_does_not_reuse_signed_terminal_slot() {
        let mut six = claim(6);
        six.terminal = true;
        six.err_code = Some(6008);
        assert_eq!(withdraw_nonce_decision(5, 6, &[six]), WithdrawNonceDecision::Wait);
    }
}
