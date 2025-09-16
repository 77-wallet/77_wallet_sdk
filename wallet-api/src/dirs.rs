use std::path::PathBuf;

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

    pub fn new(root_dir: &str) -> Result<Dirs, crate::error::ServiceError> {
        let wallet_dir = Self::join_path(root_dir, "wallet_data");

        let db_dir = Self::join_path(root_dir, "db");
        let export_dir = Self::join_path(root_dir, "export");
        let log_dir = Self::join_path(root_dir, "log");

        for dir in [&db_dir, &export_dir, &log_dir, &wallet_dir] {
            wallet_utils::file_func::create_dir_all(dir)?;
        }

        Ok(Dirs { root_dir: PathBuf::from(root_dir), wallet_dir, export_dir, db_dir, log_dir })
    }

    pub fn get_wallet_dir(&self, address: Option<&str>) -> std::path::PathBuf {
        address.map_or_else(
            || PathBuf::from(&self.wallet_dir),
            |addr| PathBuf::from(&self.wallet_dir).join(addr),
        )
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
    ) -> Result<std::path::PathBuf, crate::error::ServiceError> {
        let root_dir = self.wallet_dir.join(wallet_address).join("root");

        wallet_utils::file_func::create_dir_all(&root_dir)?;

        Ok(root_dir)
    }

    pub(crate) fn get_subs_dir(
        &self,
        wallet_address: &str,
    ) -> Result<std::path::PathBuf, crate::error::ServiceError> {
        let subs_dir = self.wallet_dir.join(wallet_address).join("subs");

        wallet_utils::file_func::create_dir_all(&subs_dir)?;

        Ok(subs_dir)
    }
}
