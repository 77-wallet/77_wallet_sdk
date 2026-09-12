# Device protection — SDK contract batch

Scope: public platform-neutral contract only. No native implementation or storage routing yet.

| Scenario | Expected invariant | Test |
| --- | --- | --- |
| No provider configured | Both operations return NotConfigured; no pass-through | missing_provider_fails_both_operations |
| Injected provider | Operation, exact input and contextual AAD forwarded; provider result returned | forwards_bound_context_and_returns_only_provider_output |
| Native failure | Typed error preserved; input is not returned as a fallback | errors_propagate_without_returning_input |
| Context binding | Different scope/purpose pairs have unambiguous length-prefixed AAD; empty fields rejected | context_encoding_is_unambiguous_and_rejects_empty_fields |
| Deferred native result | Pending yields to executor, later poll delivers native result | pending_native_callback_yields_before_returning_result |

These fixture tests validate dispatch semantics, not encryption strength. Native adapters must enforce AAD, hardware security level, nonce generation and key lifecycle. No production storage protection claim until read/write paths are connected.

Validation of exact module without unrelated test dependencies:

```sh
rustc --edition=2021 --test wallet-api/src/device_protection.rs -o target/device-protection-contract/tests
target/device-protection-contract/tests
```

Production export compilation: `cargo check -p wallet-api --offline`.
