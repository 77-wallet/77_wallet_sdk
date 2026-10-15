//! Construct a manager over an isolated test Context without workers or global initialization.
use crate::{Context, manager::WalletManager};

static NOOP_CONTEXTS: once_cell::sync::Lazy<dashmap::DashMap<usize, ()>> =
    once_cell::sync::Lazy::new(dashmap::DashMap::new);

pub struct TaskGuard(usize);
impl Drop for TaskGuard {
    fn drop(&mut self) {
        NOOP_CONTEXTS.remove(&self.0);
    }
}
pub fn manager(ctx: &'static Context) -> (WalletManager, TaskGuard) {
    let id = ctx as *const Context as usize;
    NOOP_CONTEXTS.insert(id, ());
    (WalletManager { ctx, handles: None }, TaskGuard(id))
}
pub(crate) fn is_noop_context(ctx: &Context) -> bool {
    NOOP_CONTEXTS.contains_key(&(ctx as *const Context as usize))
}
