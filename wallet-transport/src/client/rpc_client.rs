use crate::{errors::TransportError, request_builder::ReqBuilder};
use reqwest::header::{self, HeaderMap, HeaderName, HeaderValue};
use serde::Serialize;
use std::{collections::HashMap, fmt::Debug, str::FromStr};

pub struct RpcClient {
    base_url: String,
    client: reqwest::Client,
    base_auth: Option<BaseAuth>,
}

pub struct BaseAuth {
    name: String,
    password: Option<String>,
}

impl RpcClient {
    pub fn new(
        base_url: &str,
        header_opt: Option<HashMap<String, String>>,
    ) -> Result<Self, TransportError> {
        let mut headers = HeaderMap::new();

        headers.append(header::ACCEPT, "application/json".parse().unwrap());
        headers.append(header::CONTENT_TYPE, "application/json".parse().unwrap());

        if let Some(opt) = header_opt {
            for (key, value) in opt {
                headers.append(
                    HeaderName::from_str(&key).unwrap(),
                    HeaderValue::from_str(&value).unwrap(),
                );
            }
        };

        let client = reqwest::ClientBuilder::new()
            .default_headers(headers)
            .build()
            .map_err(|e| crate::TransportError::Utils(wallet_utils::Error::Http(e.into())))?;

        Ok(Self {
            base_url: base_url.to_owned(),
            client,
            base_auth: None,
        })
    }

    pub fn new_with_base_auth(
        base_url: &str,
        username: &str,
        password: &str,
    ) -> Result<Self, TransportError> {
        let mut headers = HeaderMap::new();

        headers.append(header::ACCEPT, "application/json".parse().unwrap());
        headers.append(header::CONTENT_TYPE, "application/json".parse().unwrap());

        let base_auth = Some(BaseAuth {
            name: username.to_owned(),
            password: Some(password.to_owned()),
        });

        let client = reqwest::ClientBuilder::new()
            .default_headers(headers)
            .build()
            .map_err(|e| crate::TransportError::Utils(wallet_utils::Error::Http(e.into())))?;

        Ok(Self {
            base_url: base_url.to_owned(),
            client,
            base_auth,
        })
    }

    pub fn set_params<T: Serialize + Debug>(&self, p: T) -> ReqBuilder {
        tracing::info!("[rpc request] = {:?}", p);
        tracing::info!("[url] = {:?}", self.base_url);
        let build = if let Some(auth) = &self.base_auth {
            self.client
                .post(&self.base_url)
                .basic_auth(&auth.name, auth.password.clone())
                .json(&p)
        } else {
            self.client.post(&self.base_url).json(&p)
        };

        ReqBuilder(build)
    }
}
