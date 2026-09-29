# EVM multisig chain rollout assertion matrix

| Flow | Success assertion | Failure assertion | Evidence |
| --- | --- | --- | --- |
| Polygon adapter selection | `MultisigAdapter::new` selects `Ethereum(EthChain)` and preserves `ChainCode::Polygon` | Fantom remains rejected in this five-chain rollout | `evm_chain_tests` in `multisig_adapter.rs`, offline |
| Polygon initiator address status | Polygon status lookup skips Tron-only on-chain account inspection, so it does not require a Polygon transaction adapter | Tron still takes the on-chain account inspection branch; non-Tron chains do not start this RPC | `evm_multisig_address_status_tests` in `service/multisig_account.rs`, offline |
| Polygon Safe account preflight | Before charging a deployment service fee, deriving an address, deploying, or estimating deploy gas, configured RPC reports chain ID 137 and valid nonempty bytecode at canonical Safe factory and singleton addresses | Wrong chain ID, missing code, or malformed code stops the operation before transaction broadcast; service-fee ordering is enforced in `deploy_multisig_account` | Core `safe_preflight` fake-RPC tests, SDK adapter wrong-chain test, and SDK `integration::multisig_account` preflight/payer-gate tests, offline; a successful paid deployment remains for the account-flow batch |
| Avalanche Safe account preflight | SDK routes Avalanche through the EVM multisig adapter; configured RPC reports chain ID 43114 and valid Safe factory and singleton bytecode before service-fee handling | Wrong RPC rejects deployment with no fee attempt or account mutation | Core `safe_preflight` fake-RPC tests; SDK adapter and `integration::multisig_account` preflight/payer-gate tests, offline; paid deployment and transaction execution remain pending |
| Avalanche Safe address derivation | Adapter checks chain ID and both Safe contracts before calling the factory; its decoded address and salt are returned without changing the account | Missing singleton stops before `eth_call` and leaves the account unchanged | SDK `integration::multisig_account` address-derivation fake-RPC tests, offline |
| Polygon account creation and deployment | Safe address derives and deploys with the configured chain RPC | Wrong chain ID, missing Safe contract, and failed deployment do not mark account deployed | Pending next account-flow batch |
| Polygon proposal, signature, execution | Native and ERC-20 proposals gather threshold signatures and execute once | Failed execution keeps queue recoverable | Pending transaction-flow batch |
| Arbitrum, Base, Scroll | Each chain passes RPC bytecode preflight, account flow, and transaction flow | Chain remains disabled on bytecode, fee, or receipt mismatch | Pending one-chain-at-a-time batches |

This first batch does not enable ordinary transfer, API wallet transfer,
collect, or withdrawal for a new chain. Their new enum cases fail closed or
remain uninitialized until their own flow is implemented and tested.
