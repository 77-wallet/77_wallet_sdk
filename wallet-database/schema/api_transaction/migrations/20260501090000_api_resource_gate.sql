-- Add TRON resource gate facts to original API wallet transactions.

ALTER TABLE api_collect ADD COLUMN resource_check_at TIMESTAMP NULL; -- 最近一次资源决策/检查时间，仅用于观测、排障和恢复扫描节流，不作为 CanBuild 放行条件
ALTER TABLE api_collect ADD COLUMN resource_gate_released_at TIMESTAMP NULL; -- 资源闸门释放时间；TRON 原交易进入 BuildTx 的资源前置事实
ALTER TABLE api_collect ADD COLUMN resource_gate_result INTEGER NULL; -- 资源闸门释放结果（内部枚举）
ALTER TABLE api_collect ADD COLUMN resource_block_reason INTEGER NULL; -- 当前资源阻塞原因（内部枚举）
ALTER TABLE api_collect ADD COLUMN resource_dependency_trade_no TEXT NULL; -- 当前依赖的资源任务号，用于从原交易反查资源代理/回收事实
ALTER TABLE api_collect ADD COLUMN resource_dependency_type INTEGER NULL; -- 当前依赖资源任务类型（内部枚举）

ALTER TABLE api_withdraws ADD COLUMN resource_check_at TIMESTAMP NULL; -- 最近一次资源决策/检查时间，仅用于观测、排障和恢复扫描节流，不作为 CanBuild 放行条件
ALTER TABLE api_withdraws ADD COLUMN resource_gate_released_at TIMESTAMP NULL; -- 资源闸门释放时间；TRON 原交易进入 BuildTx 的资源前置事实
ALTER TABLE api_withdraws ADD COLUMN resource_gate_result TEXT NULL; -- 资源闸门释放原因，如 resource_ready / fallback_allowed / legacy_bypass / resource_failed
ALTER TABLE api_withdraws ADD COLUMN resource_block_reason TEXT NULL; -- 当前资源阻塞原因，如 need_platform_delegate / self_resource_insufficient
ALTER TABLE api_withdraws ADD COLUMN resource_dependency_trade_no TEXT NULL; -- 当前依赖的资源任务号，用于从原交易反查资源代理/回收事实
ALTER TABLE api_withdraws ADD COLUMN resource_dependency_type TEXT NULL; -- 当前依赖资源任务类型，如 platform_delegate / local_delegate / platform_reclaim / local_reclaim

CREATE INDEX api_collect_resource_gate_scan
    ON api_collect (chain_code, resource_gate_released_at, resource_dependency_trade_no);

CREATE INDEX api_withdraws_resource_gate_scan
    ON api_withdraws (chain_code, trade_type, resource_gate_released_at, resource_dependency_trade_no);
