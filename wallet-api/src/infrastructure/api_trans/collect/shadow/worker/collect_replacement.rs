//! Scoped recovery stays in the existing collect worker lifecycle.
use super::ShadowCollectWorker;
use crate::{
    domain::api_wallet::{
        adapter::eth_tx::replacement::ReplacementFeeContext,
        adapter_factory::ApiChainAdapterFactory, trans::ApiTransDomain,
    },
    error::service::ServiceError,
};
use wallet_database::{
    entities::api_collect::ApiCollectEntity,
    repositories::api_wallet::collect_replacement::{
        CollectReplacementRepo, ReplacementResolution,
    },
};

pub(super) enum ReplacementProgress {
    Normal,
    Wait,
    ContinueNew,
    Stop,
}
impl ShadowCollectWorker {
    pub(super) async fn observe_collect_nonce(
        &self,
        req: &ApiCollectEntity,
    ) -> Result<(u64, i64, Option<ReplacementFeeContext>), ServiceError> {
        let pool = self.ctx.api_transaction_pool()?;
        let existing = CollectReplacementRepo::find(&pool, &req.trade_no)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?;
        let regular = self.get_nonce(&req.from_addr, &req.chain_code).await;
        if existing.is_none() {
            if let Ok((n, c)) = regular {
                return Ok((n, c, None));
            }
        }
        let chain_next = ApiTransDomain::nonce(self.ctx, &req.from_addr, &req.chain_code).await?;
        let chain_next = i64::try_from(chain_next)
            .map_err(|_| ServiceError::Parameter("nonce exceeds recovery range".into()))?;
        let r = if let Some(r) = existing {
            r
        } else {
            let Some(scope) = self.ctx.config().collect_replacement_recovery.as_ref().filter(|s| {
                s.chain_code == req.chain_code
                    && s.from_addr.trim().eq_ignore_ascii_case(req.from_addr.trim())
            }) else {
                return regular.map(|(n, c)| (n, c, None));
            };
            let Some(r) = CollectReplacementRepo::prepare(&pool, &req.trade_no, scope, chain_next)
                .await
                .map_err(|e| ServiceError::Database(e.into()))?
            else {
                return Err(ServiceError::Parameter(
                    "scoped replacement is not eligible or owned by another order".into(),
                ));
            };
            r
        };
        match self.reconcile_collect_replacement(req).await? {
            ReplacementProgress::Stop | ReplacementProgress::ContinueNew => {
                return Err(ServiceError::Parameter(
                    "replacement nonce already consumed; result reconciled".into(),
                ));
            }
            _ => {}
        }
        if chain_next != r.nonce {
            return Err(ServiceError::Parameter(
                "replacement nonce consumed but canonical winner not yet visible".into(),
            ));
        }
        Ok((
            r.nonce as u64,
            chain_next,
            Some(ReplacementFeeContext {
                old_raw: r.old_raw,
                old_hash: r.old_hash,
                from_addr: r.from_addr,
                nonce: r.nonce as u64,
            }),
        ))
    }
    pub(super) async fn reconcile_collect_replacement(
        &self,
        req: &ApiCollectEntity,
    ) -> Result<ReplacementProgress, ServiceError> {
        let pool = self.ctx.api_transaction_pool()?;
        let Some(r) = CollectReplacementRepo::find(&pool, &req.trade_no)
            .await
            .map_err(|e| ServiceError::Database(e.into()))?
        else {
            return Ok(ReplacementProgress::Normal);
        };
        if r.abandoned_at.is_some() {
            return Ok(ReplacementProgress::Stop);
        }
        if let Some(winner) = r.winner_hash {
            if r.new_hash.as_deref() == Some(winner.as_str()) && r.execution_success == Some(true) {
                return Ok(ReplacementProgress::ContinueNew);
            }
            self.advancer.try_advance(&req.trade_no).await;
            return Ok(ReplacementProgress::Stop);
        }
        let adapter =
            ApiChainAdapterFactory::get_transaction_adapter_with_ctx(self.ctx, &r.chain_code)
                .await?;
        let old = adapter.replacement_receipt(&r.old_hash, &r.from_addr, r.nonce as u64).await;
        let new = if let Some(hash) = r.new_hash.as_deref() {
            adapter.replacement_receipt(hash, &r.from_addr, r.nonce as u64).await
        } else {
            Ok(None)
        };
        let old = old.ok().flatten();
        let new = new.ok().flatten();
        if old.is_some() && new.is_some() {
            return Err(ServiceError::Parameter(
                "inconsistent replacement chain evidence: both identities included".into(),
            ));
        }
        let mut winner = old.or(new);
        if winner.is_none() && adapter.nonce(&r.from_addr).await.is_ok_and(|n| n > r.nonce as u64) {
            if let Ok(Some(conflict)) = adapter
                .finalized_nonce_conflict(&r.chain_code, &r.from_addr, r.nonce as i32, &r.old_hash)
                .await
            {
                winner = adapter
                    .replacement_receipt(
                        &conflict.conflicting_tx_hash,
                        &r.from_addr,
                        r.nonce as u64,
                    )
                    .await
                    .ok()
                    .flatten();
            }
        }
        let Some(w) = winner else { return Ok(ReplacementProgress::Wait) };
        let block = i64::try_from(w.block_number)
            .map_err(|_| ServiceError::Parameter("replacement proof height overflow".into()))?;
        CollectReplacementRepo::resolve(
            &pool,
            &req.trade_no,
            &ReplacementResolution {
                transaction_fee: &w.transaction_fee,
                resource_consume: &w.resource_consume,
                winner_hash: &w.tx_hash,
                success: w.success,
                block_hash: &w.block_hash,
                block_number: block,
                transaction_time: &w.transaction_time,
            },
        )
        .await
        .map_err(|e| ServiceError::Database(e.into()))?;
        self.advancer.try_advance(&req.trade_no).await;
        if r.new_hash.as_deref() == Some(w.tx_hash.as_str()) && w.success {
            Ok(ReplacementProgress::ContinueNew)
        } else {
            self.advancer.try_advance(&req.trade_no).await;
            Ok(ReplacementProgress::Stop)
        }
    }
}
