use wallet_entity::resources::{
    account::Account, assets::Assets, chain::Chain, device::Device,
    system_notification::SystemNotification, wallet::Wallet,
};

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub enum Resources {
    Wallet(resource::Command<resource::GeneralAction<Wallet>>),
    Account(resource::Command<resource::GeneralAction<Account>>),
    Coin(resource::Command<resource::GeneralAction<Assets>>),
    Chain(resource::Command<resource::GeneralAction<Chain>>),
    Device(resource::Command<resource::GeneralAction<Device>>),
    SystemNotification(resource::Command<resource::GeneralAction<SystemNotification>>),
}

impl resource::Action for Resources {
    async fn execute<'c, E>(&self, executor: E) -> Result<(), resource::Error>
    where
        E: sqlx::prelude::Executor<'c, Database = sqlx::Sqlite>,
    {
        match self {
            // Account
            Resources::Wallet(r) => r.execute(executor).await,
            Resources::Account(r) => r.execute(executor).await,
            Resources::Coin(r) => r.execute(executor).await,
            Resources::Chain(r) => r.execute(executor).await,
            Resources::Device(r) => r.execute(executor).await,
            Resources::SystemNotification(r) => r.execute(executor).await,
        }
    }
}

impl resource::Resources for Resources {}
