use std::path::{Component, Path, PathBuf};

use crate::error::service::ServiceError;

#[derive(Debug, Clone, Default)]
pub struct Dirs {
    pub root_dir: PathBuf,
    pub wallet_dir: PathBuf,
    pub export_dir: PathBuf,
    pub db_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl Dirs {
    pub fn join_path(root_dir: &str, sub_path: &str) -> PathBuf {
        PathBuf::from(root_dir).join(sub_path)
    }

    pub fn new(root_dir: &str) -> Result<Dirs, crate::error::service::ServiceError> {
        let wallet_dir = Self::join_path(root_dir, "wallet_data");

        let db_dir = Self::join_path(root_dir, "db");
        let export_dir = Self::join_path(root_dir, "export");
        let log_dir = Self::join_path(root_dir, "log");

        for dir in [&db_dir, &export_dir, &log_dir, &wallet_dir] {
            wallet_utils::file_func::create_dir_all(dir)?;
        }

        Ok(Dirs { root_dir: PathBuf::from(root_dir), wallet_dir, export_dir, db_dir, log_dir })
    }

    pub fn get_wallet_dir(
        &self,
        address: Option<&str>,
    ) -> Result<std::path::PathBuf, ServiceError> {
        match address {
            Some(addr) => Ok(PathBuf::from(&self.wallet_dir)
                .join(Self::validate_path_segment(addr, "wallet address")?)),
            None => Ok(PathBuf::from(&self.wallet_dir)),
        }
    }

    pub fn get_export_dir(&self) -> std::path::PathBuf {
        self.export_dir.clone()
    }

    pub fn get_log_dir(&self) -> std::path::PathBuf {
        self.log_dir.clone()
    }

    pub(crate) fn get_root_dir(
        &self,
        wallet_address: &str,
    ) -> Result<std::path::PathBuf, ServiceError> {
        let root_dir = self
            .wallet_dir
            .join(Self::validate_path_segment(wallet_address, "wallet address")?)
            .join("root");

        wallet_utils::file_func::create_dir_all(&root_dir)?;

        Ok(root_dir)
    }

    pub(crate) fn get_subs_dir(
        &self,
        wallet_address: &str,
    ) -> Result<std::path::PathBuf, ServiceError> {
        let subs_dir = self
            .wallet_dir
            .join(Self::validate_path_segment(wallet_address, "wallet address")?)
            .join("subs");

        wallet_utils::file_func::create_dir_all(&subs_dir)?;

        Ok(subs_dir)
    }

    pub(crate) fn validate_path_segment<'a>(
        segment: &'a str,
        label: &str,
    ) -> Result<&'a str, ServiceError> {
        let path = Path::new(segment);
        let mut components = path.components();
        let Some(Component::Normal(_)) = components.next() else {
            return Err(ServiceError::Parameter(format!("invalid {label} path segment")));
        };
        if components.next().is_some() {
            return Err(ServiceError::Parameter(format!("invalid {label} path segment")));
        }
        if segment.as_bytes().contains(&0) {
            return Err(ServiceError::Parameter(format!("invalid {label} path segment")));
        }
        Ok(segment)
    }
}

#[cfg(test)]
mod tests {
    use super::Dirs;

    #[test]
    fn validate_path_segment_accepts_plain_wallet_address() {
        let segment = Dirs::validate_path_segment("TXYZ1234567890", "wallet address")
            .expect("plain address should be valid");

        assert_eq!(segment, "TXYZ1234567890");
    }

    #[test]
    fn validate_path_segment_rejects_traversal() {
        let err = Dirs::validate_path_segment("../secret", "wallet address")
            .expect_err("traversal must be rejected");

        assert!(err.to_string().contains("invalid wallet address path segment"));
    }

    #[test]
    fn validate_path_segment_rejects_nested_path() {
        let err = Dirs::validate_path_segment("wallet/root", "wallet address")
            .expect_err("nested path must be rejected");

        assert!(err.to_string().contains("invalid wallet address path segment"));
    }
}
