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
| Scroll ERC-20 SDK quote | SDK `scroll_erc20_quote_keeps_transfer_calldata_and_adds_core_fee` | The SDK sends ERC-20 calldata through Core and includes the L1 fee in the returned quote. |
| Scroll RPC failure in SDK quote | SDK `scroll_additional_fee_rpc_error_fails_sdk_quote` | A Core additional-fee error aborts the SDK quote before execution gas estimation. |
| Three-tier total and execution settings | SDK `evm_additional_fee_is_added_to_each_quote_without_changing_execution_fee_setting` | The same additional fee is added to estimated and maximum amounts for each tier, while execution `feeSetting` and existing JSON fields remain unchanged. |

Scope: standard wallet ordinary native/ERC-20 transfer fee estimation only. The backend gas tracker remains the source of L2 execution prices. No durable transaction facts or broadcast side effects are changed by this quote path. Live RPC verification is separate from these offline tests.

The Base fee is a conservative upper-bound before signing, not the exact final L1 charge. Core owns the chain-specific RPC; SDK adds its result to displayed totals while leaving `feeSetting` unchanged. Live Base/Scroll-node support remains to be verified separately.
