# Chain configuration precedence

Scope: `wallet-api` default chain selection for Core and API Wallet.

| Invariant | Offline test |
| --- | --- |
| A chain returned disabled by the backend stays disabled in dev | `dev_does_not_restore_backend_disabled_local_chain` |
| A locally active chain absent from the backend response remains available for early dev testing | `dev_restores_local_chain_missing_from_backend` |
| Test and prod use only the backend-enabled chain selection | `test_and_prod_keep_backend_chain_selection` |
| The five requested EVM chains remain confined to testnet defaults; excluded Fantom has no local seed | `added_evm_chains_are_testnet_only`, `added_evm_chains_have_native_testnet_coins`, `added_evm_chains_have_active_testnet_nodes` |
| An enabled chain missing from any wallet/account mapping requests chain synchronization | `enabled_chain_is_missing_when_any_wallet_account_lacks_it` |
| An enabled chain already present for every wallet/account mapping does not request redundant synchronization | `enabled_chain_is_complete_when_every_wallet_account_has_it` |

Test layer: unit. The tests use static local configuration and synthetic backend chain-code lists;
they perform no backend calls, database writes, or network requests.

Failure invariant: local dev fallback may supplement a missing backend chain, but must never
override an explicit backend disable. Core and API Wallet pass both the complete returned chain
set and the enabled subset through the same selection rule.
Fantom remains a recognized chain code for compatibility when supplied by the backend, but is
not locally auto-enabled as part of 2.2.

Known coverage gap: repository-level status persistence is covered by existing chain repository
tests rather than repeated here.

Run:

- `cargo test -p wallet-api --lib --no-default-features default_data::chain::tests`
- `cargo test -p wallet-api --lib --no-default-features domain::chain::tests`
