use wallet_database::{
    entities::{
        account::{self, AccountEntity},
        permission_user::PermissionUserEntity,
    },
    DbPool,
};
use wallet_types::constant::chain_code;

pub struct PermissionDomain;

impl PermissionDomain {
    pub async fn mark_user_isself(
        pool: &DbPool,
        users: &mut [PermissionUserEntity],
    ) -> Result<(), crate::ServiceError> {
        for user in users.iter_mut() {
            let req = account::QueryReq::new_address_chain(&user.address, chain_code::TRON);
            let account = AccountEntity::detail(pool.as_ref(), &req).await?;
            if account.is_some() {
                user.is_self = 1;
            }
        }
        Ok(())
    }
}
