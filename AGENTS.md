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
- Codex must follow these documents when generating or modifying tests.
- Before non-trivial test changes, read `docs/codex/testing.md` first.
- Every PR must pass `docs/codex/checklists/pr-definition-of-done.md`
- For non-trivial tasks, create or update `PLANS.md` before implementation

## Security

- Never commit or print private keys, mnemonics, credentials, or production config
