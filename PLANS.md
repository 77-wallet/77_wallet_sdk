# PLANS

Current task execution plan.
Refs: `docs/codex/testing.md`, `docs/codex/workflows.md`.

## Task

- Name: repoctx decoupling (batch 7: address_book service path)
- Goal:
  - 将 `AddressBookService` 从 `RepositoryFactory::address_book_repo()` 返回的 repo 实例依赖中解耦
  - 统一到“显式具体 DB pool + repository 静态方法”风格
  - 保持业务行为不变，仅收敛调用与结构

## Scope

### In

- `wallet-database/src/repositories/address_book.rs`
- `wallet-api/src/service/address_book.rs`
- `wallet-api/src/api/address_book.rs`
- `PLANS.md`

### Out

- `CoinService/AssetsService/AccountService/WalletService` 的改造
- `RepoCtx` 主体结构改造
- DAO/SQL 语义变更与事务模型调整

## Constraints

- Keep behavior unchanged
- Batch-by-batch compile validation
- Offline validation only

## Plan

1. Convert `AddressBookRepo` to stateless static APIs with `&CoreDbPool`
2. Refactor `AddressBookService` to hold/get concrete `CoreDbPool` (no repo instance)
3. Adapt `api/address_book.rs` call sites to new service constructor
4. Run offline checks for `wallet-database` and `wallet-api`

## Validation Commands

- `cargo check -p wallet-database --offline`
- `cargo check -p wallet-api --offline`

## Progress Checklist

- [x] Convert AddressBookRepo static APIs
- [x] Refactor AddressBookService constructor and calls
- [x] Adapt address_book API call sites
- [x] Run focused offline validation
