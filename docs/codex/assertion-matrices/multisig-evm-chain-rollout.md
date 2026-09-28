# EVM multisig chain rollout assertion matrix

| Flow | Success assertion | Failure assertion | Evidence |
| --- | --- | --- | --- |
| Polygon adapter selection | `MultisigAdapter::new` selects `Ethereum(EthChain)` and preserves `ChainCode::Polygon` | Fantom remains rejected in this five-chain rollout | `evm_chain_tests` in `multisig_adapter.rs`, offline |
| Polygon initiator address status | Polygon status lookup skips Tron-only on-chain account inspection, so it does not require a Polygon transaction adapter | Tron still takes the on-chain account inspection branch; non-Tron chains do not start this RPC | `evm_multisig_address_status_tests` in `service/multisig_account.rs`, offline |
| Polygon account creation and deployment | Safe address derives and deploys with the configured chain RPC | Wrong chain ID, missing Safe contract, and failed deployment do not mark account deployed | Pending next account-flow batch |
| Polygon proposal, signature, execution | Native and ERC-20 proposals gather threshold signatures and execute once | Failed execution keeps queue recoverable | Pending transaction-flow batch |
| Avalanche, Arbitrum, Base, Scroll | Each chain passes RPC bytecode preflight, account flow, and transaction flow | Chain remains disabled on bytecode, fee, or receipt mismatch | Pending one-chain-at-a-time batches |

This first batch does not enable ordinary transfer, API wallet transfer,
collect, or withdrawal for a new chain. Their new enum cases fail closed or
remain uninitialized until their own flow is implemented and tested.
