# Standard Wallet Transfer Fee Assertions

## Scroll ordinary transfer quote (2.2 Batch 1)

| Case | Test entrypoint | Invariant |
| --- | --- | --- |
| Scroll native transfer L1 quote | `scroll_l1_fee_rpc_uses_transfer_payload_and_parses_wei` | RPC receives the transfer object and returns a wei-denominated L1 fee. |
| Scroll ERC-20 transfer L1 quote | `scroll_l1_fee_rpc_includes_erc20_transfer_calldata` | RPC receives the token contract address and ABI-encoded transfer calldata, not a plain ETH transfer. |
| Invalid L1 fee response | `scroll_l1_fee_rpc_rejects_invalid_fee_instead_of_zero` | A malformed fee fails the quote; it is not silently replaced with zero. |
| Unsupported L1 fee RPC | `scroll_l1_fee_rpc_rejects_json_rpc_error` | A node JSON-RPC error fails the quote rather than silently omitting the extra fee. |
| Zero L1 fee response | `scroll_l1_fee_rpc_rejects_zero_fee_for_nonempty_transfer` | A zero result for a nonempty transfer fails the quote rather than understating the fee. |
| Chain-specific fee policy | `only_scroll_requires_additional_fee_rpc_in_this_batch` | Scroll selects the L1 RPC; Ethereum, BNB, Polygon, Avalanche, Arbitrum, and Base retain execution-only quotes in this batch. |
| Three-tier total and execution settings | `scroll_l1_fee_is_added_to_each_quote_without_changing_execution_fee_setting` | The same L1 data fee is added to estimated and maximum amounts for each tier, while EIP-1559 execution parameters and existing JSON fields remain unchanged. |

Scope: standard wallet ordinary native/ERC-20 transfer fee estimation only. The backend gas tracker remains the source of L2 execution prices. No durable transaction facts or broadcast side effects are changed by this quote path. Live RPC verification is separate from these offline tests.
