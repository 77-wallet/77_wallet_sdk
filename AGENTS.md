# AGENTS.md (root)

## Boundaries

- Workspace crates: wallet-api / wallet-transport-backend / wallet-database / wallet-oss / wallet-tree / wallet-ecdh
- Docs under `docs/`; tests under each crate `tests/`
- Write build outputs only to `target/`; do not commit temp runtime files

## Rule Discovery

- Submodules have their own `AGENTS.md`
- Always resolve and follow nearest rules in `root -> leaf` order

## Mandatory References

- Testing rules are defined in `docs/codex/testing.md`.
- PR acceptance criteria are defined in `docs/codex/checklists/pr-definition-of-done.md`.
- Commit message rules are defined in `docs/codex/commit-message.md`.
- Codex must follow these documents when generating or modifying tests.
- Before non-trivial test changes, read `docs/codex/testing.md` first.
- Every PR must pass `docs/codex/checklists/pr-definition-of-done.md`
- For non-trivial tasks, create or update `PLANS.md` before implementation

## Change Size Gate

- If estimated change spans `>=2` crates or `>=10` files, do not execute the full change in one round.
- In that case, stop and split into batches before implementation. Update `PLANS.md` with:
  - batch scope
  - target validation commands
  - stop condition for the round
- Each batch should focus on one module and one flow, and keep verification to the smallest affected command set.
- For test-first tasks, do not expand architecture refactors until the target regression tests are added and passing for that batch.

## Security

- Never commit or print private keys, mnemonics, credentials, or production config

## Commit Message Policy

- Follow `docs/codex/commit-message.md`.
