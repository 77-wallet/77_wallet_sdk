#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    // 波场活跃权限超过 8个
    #[error("actives permission more than eight")]
    ActivesPermissionMore,
    // 所有权重的和小于 阈值
    #[error("weight less tran threshold")]
    WeightLessThreshold,
}

impl PermissionError {
    pub(crate) fn get_status_code(&self) -> i64 {
        match self {
            PermissionError::ActivesPermissionMore => 4300,
            PermissionError::WeightLessThreshold => 4301,
        }
    }
}
