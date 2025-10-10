use crate::api_response::ApiBackendData;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ApiBackendRequestBody{
    pub key: String,
    pub data: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ApiBackendRequest {
    pub sn: String,
    pub sign: String,
    pub body: ApiBackendRequestBody,
}