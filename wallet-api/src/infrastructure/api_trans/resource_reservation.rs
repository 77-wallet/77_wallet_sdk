use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ResourceBudget {
    pub(crate) energy: u64,
    pub(crate) bandwidth: u64,
}

impl ResourceBudget {
    pub(crate) fn new(energy: u64, bandwidth: u64) -> Self {
        Self { energy, bandwidth }
    }

    fn saturating_sub(self, other: Self) -> Self {
        Self {
            energy: self.energy.saturating_sub(other.energy),
            bandwidth: self.bandwidth.saturating_sub(other.bandwidth),
        }
    }

    fn add_assign(&mut self, other: Self) {
        self.energy = self.energy.saturating_add(other.energy);
        self.bandwidth = self.bandwidth.saturating_add(other.bandwidth);
    }

    fn sub_assign_saturating(&mut self, other: Self) {
        self.energy = self.energy.saturating_sub(other.energy);
        self.bandwidth = self.bandwidth.saturating_sub(other.bandwidth);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ResourceShortfall {
    pub(crate) energy: u64,
    pub(crate) bandwidth: u64,
    pub(crate) effective_available: ResourceBudget,
    pub(crate) reserved: ResourceBudget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResourceReservationDecision {
    Reserved { effective_available: ResourceBudget, reserved: ResourceBudget },
    NeedMore(ResourceShortfall),
}

#[derive(Debug, Default)]
struct ResourceReservationState {
    totals: HashMap<String, ResourceBudget>,
    by_trade_no: HashMap<String, (String, ResourceBudget)>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct WithdrawResourceReservationManager {
    inner: Arc<Mutex<ResourceReservationState>>,
}

impl WithdrawResourceReservationManager {
    pub(crate) fn reserve_if_available(
        &self,
        resource_key: impl Into<String>,
        trade_no: impl Into<String>,
        required: ResourceBudget,
        available: ResourceBudget,
    ) -> ResourceReservationDecision {
        let resource_key = resource_key.into();
        let trade_no = trade_no.into();
        let mut state = self.inner.lock().expect("resource reservation lock poisoned");
        if let Some((existing_key, existing_budget)) = state.by_trade_no.get(&trade_no).cloned() {
            if existing_key == resource_key {
                let reserved = state.totals.get(&resource_key).copied().unwrap_or_default();
                return ResourceReservationDecision::Reserved {
                    effective_available: available.saturating_sub(reserved),
                    reserved,
                };
            }
            Self::release_locked(&mut state, &trade_no);
        }

        let reserved = state.totals.get(&resource_key).copied().unwrap_or_default();
        let effective_available = available.saturating_sub(reserved);
        let energy_shortfall = required.energy.saturating_sub(effective_available.energy);
        let bandwidth_shortfall = required.bandwidth.saturating_sub(effective_available.bandwidth);

        state.totals.entry(resource_key.clone()).or_default().add_assign(required);
        state.by_trade_no.insert(trade_no, (resource_key, required));

        if energy_shortfall == 0 && bandwidth_shortfall == 0 {
            ResourceReservationDecision::Reserved { effective_available, reserved }
        } else {
            ResourceReservationDecision::NeedMore(ResourceShortfall {
                energy: energy_shortfall,
                bandwidth: bandwidth_shortfall,
                effective_available,
                reserved,
            })
        }
    }

    pub(crate) fn release(&self, trade_no: &str) {
        let mut state = self.inner.lock().expect("resource reservation lock poisoned");
        Self::release_locked(&mut state, trade_no);
    }

    #[cfg(test)]
    fn reserved_for(&self, resource_key: &str) -> ResourceBudget {
        self.inner
            .lock()
            .expect("resource reservation lock poisoned")
            .totals
            .get(resource_key)
            .copied()
            .unwrap_or_default()
    }

    fn release_locked(state: &mut ResourceReservationState, trade_no: &str) {
        let Some((key, budget)) = state.by_trade_no.remove(trade_no) else {
            return;
        };

        if let Some(total) = state.totals.get_mut(&key) {
            total.sub_assign_saturating(budget);
            if *total == ResourceBudget::default() {
                state.totals.remove(&key);
            }
        }
    }
}

#[cfg(test)]
mod withdraw_resource_reservation_tests {
    use super::*;

    #[test]
    fn same_address_pending_orders_reserve_against_shared_available_energy() {
        let manager = WithdrawResourceReservationManager::default();
        let key = "TAy4";
        let available = ResourceBudget::new(20_000, 1_000);
        let required = ResourceBudget::new(13_532, 345);

        assert!(matches!(
            manager.reserve_if_available(key, "A", required, available),
            ResourceReservationDecision::Reserved { .. }
        ));

        let decision = manager.reserve_if_available(key, "B", required, available);
        assert_eq!(
            decision,
            ResourceReservationDecision::NeedMore(ResourceShortfall {
                energy: 7_064,
                bandwidth: 0,
                effective_available: ResourceBudget::new(6_468, 655),
                reserved: required,
            })
        );
        assert_eq!(manager.reserved_for(&key), ResourceBudget::new(27_064, 690));
    }

    #[test]
    fn different_addresses_reserve_independently() {
        let manager = WithdrawResourceReservationManager::default();
        let available = ResourceBudget::new(20_000, 1_000);
        let required = ResourceBudget::new(13_532, 345);

        assert!(matches!(
            manager.reserve_if_available("TAy4", "A", required, available),
            ResourceReservationDecision::Reserved { .. }
        ));
        assert!(matches!(
            manager.reserve_if_available("TBxx", "B", required, available),
            ResourceReservationDecision::Reserved { .. }
        ));
    }

    #[test]
    fn release_removes_pending_budget() {
        let manager = WithdrawResourceReservationManager::default();
        let key = "TAy4";
        let available = ResourceBudget::new(20_000, 1_000);
        let required = ResourceBudget::new(13_532, 345);

        manager.reserve_if_available(key, "A", required, available);
        manager.release("A");

        assert_eq!(manager.reserved_for(&key), ResourceBudget::default());
        assert!(matches!(
            manager.reserve_if_available(key, "B", required, available),
            ResourceReservationDecision::Reserved { .. }
        ));
    }
}
