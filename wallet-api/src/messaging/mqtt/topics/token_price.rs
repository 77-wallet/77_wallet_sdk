use crate::{
    response_vo::standard_wallet::coin::TokenCurrencies,
    service::exchange_rate::ExchangeRateService,
};
use wallet_database::{
    entities::coin::CoinId,
    repositories::{api_wallet::coin::ApiCoinRepo, coin::CoinRepo},
};
use wallet_transport_backend::response_vo::coin::TokenPriceChangeBody;

// biz_type = TOKEN_PRICE_CHANGE
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TokenPriceChange {
    pub body: TokenPriceChangeBody,
}

impl TokenPriceChange {
    pub(crate) async fn exec(&self) -> Result<(), anyhow::Error> {
        let chain_code = &self.body.chain_code;
        let symbol = &self.body.symbol;
        let token_address = &self.body.token_address;
        // let name = &self.body.name;
        let price = self.body.price;
        let unit = self.body.unit;

        tracing::info!("TokenPriceChange: {:?}", self);
        // let asset_calc_actor_manager =
        //     crate::context::CONTEXT.get().unwrap().get_global_asset_calc_actor_manager().await?;
        // asset_calc_actor_manager
        //     .update_price(
        //         symbol,
        //         chain_code,
        //         name.as_deref().unwrap_or_default(),
        //         token_address.to_owned(),
        //         price,
        //         unit,
        //     )
        //     .await?;
        let pool = crate::get_context()?.api_wallet_pool()?;

        let coin_id = CoinId {
            chain_code: chain_code.to_string(),
            symbol: symbol.to_string(),
            token_address: token_address.clone(),
        };
        CoinRepo::update_price_unit(
            pool.into_inner(),
            &coin_id,
            &price.to_string(),
            Some(unit),
            None,
            self.body.swappable,
            None,
            None,
        )
        .await?;

        ApiCoinRepo::update_price_unit(
            &coin_id,
            &price.to_string(),
            Some(unit),
            None,
            None,
            None,
            &pool,
        )
        .await?;

        let app_state = crate::app_state::APP_STATE.read().await;
        let currency = app_state.currency();

        let repo = wallet_database::factory::RepositoryFactory::repo(pool.into_inner());
        let exchange_rate = ExchangeRateService::new(repo).detail(currency).await?;

        let res =
            TokenCurrencies::calculate_token_price_changes(&self.body, exchange_rate.rate).await?;
        let data = crate::messaging::notify::event::NotifyEvent::TokenPriceChange(res);
        crate::messaging::notify::FrontendNotifyEvent::new(data).send().await?;

        Ok(())
    }
}
