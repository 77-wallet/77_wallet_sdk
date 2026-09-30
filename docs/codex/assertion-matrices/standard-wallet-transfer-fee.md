# Standard Wallet Transfer Fee Assertions

## EVM additional fee: Core protocol and SDK quote (2.2)

| Case | Test entrypoint | Invariant |
| --- | --- | --- |
| Scroll native transfer L1 quote | Core `evm_additional_fee_scroll_native_uses_same_provider_and_auth` | The selected provider sends the transfer object with its auth header and parses wei. |
| Scroll ERC-20 transfer L1 quote | Core `evm_additional_fee_scroll_erc20_contains_transfer_calldata` | The token contract and ABI-encoded transfer calldata are sent, not a plain ETH transfer. |
| Base native and ERC-20 upper bound | Core `evm_additional_fee_base_native_and_erc20_query_oracle_upper_bound` | Both transfer shapes query Base GasPriceOracle and parse wei. |
| Invalid, zero, or RPC-error response | Core `evm_additional_fee_invalid_zero_and_rpc_errors_fail` | Neither chain silently omits or zeroes a failed additional-fee estimate. |
| Chain-specific fee policy | Core `evm_additional_fee_non_l2_chains_do_not_call_rpc` | Ethereum/BNB/Polygon/Avalanche/Arbitrum retain execution-only fees without extra RPC. |
| Base native SDK quote | SDK `base_native_quote_uses_core_fee_without_changing_execution_settings` | The SDK requests Core's Base fee before estimating execution gas and includes it in the returned quote. |
| Base ERC-20 SDK quote | SDK `base_erc20_quote_keeps_transfer_calldata_and_adds_core_fee` | The SDK checks token balance, keeps ERC-20 transfer calldata, and asks the Base oracle for the larger unsigned transaction size. |
| Polygon/Avalanche/Arbitrum SDK quote | SDK `non_l1_data_fee_chains_quote_three_tiers_without_extra_rpc` | Each chain retains three execution-fee tiers and makes no separate L1-data-fee RPC. |
| Scroll native SDK quote | SDK `scroll_native_quote_uses_core_fee_without_changing_execution_settings` | The SDK sends the native transfer shape to Scroll's L1-data-fee RPC and includes the result without changing execution settings. |
| Scroll ERC-20 SDK quote | SDK `scroll_erc20_quote_keeps_transfer_calldata_and_adds_core_fee` | The SDK sends ERC-20 calldata through Core and includes the L1 fee in the returned quote. |
| Scroll RPC failure in SDK quote | SDK `scroll_additional_fee_rpc_error_fails_sdk_quote` | A Core additional-fee error aborts the SDK quote before execution gas estimation. |
| Three-tier total and execution settings | SDK `evm_additional_fee_is_added_to_each_quote_without_changing_execution_fee_setting` | The same additional fee is added to estimated and maximum amounts for each tier, while execution `feeSetting` and existing JSON fields remain unchanged. |
| Base/Scroll send preflight fee | SDK `send_preflight_reserves_additional_fee_for_base_and_scroll` | Before broadcast, the same transfer shape requires execution fee plus Core's additional L1 fee in native balance. |
| Base/Scroll transfer entry rejects missing L1 fee | SDK `send_entry_rejects_base_and_scroll_when_only_l1_fee_is_missing` | The actual transfer entry returns `InsufficientFeeBalance` when native balance covers the EIP-1559 execution cap but not its additional L1 fee; only balance and L1 quote RPCs occur, with no signing or broadcast. |
| Exact wei send balance boundary | SDK `send_preflight_requires_the_full_fee_at_the_wei_boundary` | Exact native balance passes, and one wei less returns `InsufficientFeeBalance` before broadcast. |
| EIP-1559 send cap reserve | SDK `send_preflight_reserves_submitted_eip1559_fee_cap` | ETH and five 2.2 EVM chains reserve `gasLimit × maxFeePerGas` (matching Core's submitted cap), plus any L1 fee; BNB keeps its historical special calculation. |
| Base/Scroll ERC-20 send preflight | SDK `send_preflight_uses_erc20_transfer_shape_for_base_and_scroll` | The Base unsigned-size bound accounts for ERC-20 calldata; Scroll receives the token contract call rather than a native transfer. |
| Other EVM send preflight fee | SDK `send_preflight_keeps_execution_fee_on_non_l2_chain` | Polygon retains execution-only fee and makes no additional-fee RPC. |
| Scroll send preflight failure | SDK `send_preflight_aborts_when_scroll_additional_fee_rpc_fails` | The additional-fee RPC error aborts before any broadcast attempt. |

Scope: standard wallet ordinary native/ERC-20 transfer fee estimation and pre-broadcast fee guard. The backend gas tracker remains the source of L2 execution prices. No durable transaction facts or broadcast side effects are changed by the quote path. Live RPC verification is separate from these offline tests.

The Base fee is a conservative upper-bound before signing, not the exact final L1 charge. Core owns the chain-specific RPC; SDK adds its result to displayed totals while leaving `feeSetting` unchanged.

Live read-only verification (2026-09-29): the authenticated 77 Scroll mainnet node returned chain ID 534352 and nonzero `scroll_estimateL1DataFee` results for native-transfer and ERC-20-transfer-shaped requests. The authenticated 77 Base mainnet node returned chain ID 8453 and nonzero `GasPriceOracle.getL1FeeUpperBound(uint256)` results for unsigned sizes 160 and 228 bytes. Base Sepolia also returned a nonzero oracle result for 160 bytes. No transaction was signed or broadcast. This verifies node method availability and response shape, not the complete app/backend tracker-to-UI quote flow or final charged fee.

Dev backend tracker probe (2026-09-30): `token/findGasTracker` returned three positive gas-price tiers for `base`, but returned business error `999` for `scroll`. On the authenticated 77 Scroll node, `eth_feeHistory(20, latest, [25, 50, 75])` returned 21 base-fee entries, 20 reward rows of length 3, and 20 gas-used ratios; `eth_getBlockByNumber` and `eth_gasPrice` also returned values. Thus the backend Scroll tracker failure needs backend-side diagnosis; it is not evidence that the node lacks these RPC methods. SDK `ChainTransDomain::gas_oracle` falls back to a node-derived propose tier if the backend tracker errors, so a full three-tier Scroll quote is not established by this probe. No app quote request or transaction broadcast was performed.

The local Scroll Sepolia fallback in `wallet-api/data/config/node.testnet.toml` is stale because Scroll shut down that testnet after 2026-07-30. It does **not** block dev App validation against a returned 77 mainnet node: dev permits both network types, and automatic node binding prefers an enabled backend node over a local fallback. The 2026-09-28 simulator database snapshot already has both Base and Scroll enabled and auto-bound to active `77_Nodes` mainnet backend nodes (`is_local = 0`); confirm the current runtime binding if the database has since changed.
