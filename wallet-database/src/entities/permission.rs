use crate::dao::permission;

use super::permission_user::PermissionUserEntity;
use chrono::{DateTime, Utc};

#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PermissionEntity {
    pub id: String,
    pub name: String,
    pub grantor_addr: String,
    pub types: String,
    pub active_id: i64,
    pub threshold: i64,
    pub member: i64,
    pub chain_code: String,
    pub operations: String,
    pub is_del: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl PermissionEntity {
    pub fn get_id(address: &str, operations: &str, member: &[&str]) -> String {
        let mut params = vec![address, operations];
        params.extend(member);

        wallet_utils::snowflake::gen_hash_uid(params)
    }
}

pub struct PermissionWithuserEntity {
    pub permission: PermissionEntity,
    pub user: Vec<PermissionUserEntity>,
}

impl PermissionWithuserEntity {
    // check whether user has changed
    pub fn user_has_changed(&self, user: &[PermissionUserEntity]) -> bool {
        if self.permission.member != user.len() as i64 {
            return true;
        }

        for old_user in self.user.iter() {
            if user
                .iter()
                .find(|u| u.address == old_user.address)
                .is_none()
            {
                return true;
            }
        }
        false
    }
}
