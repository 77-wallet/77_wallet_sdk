use crate::{consts::BASE_URL, response::BackendResponse};

// const BASE_URL: &str = "https://api.hhxe43.com";
// const BASE_URL: &str = "http://api.wallet.net";

pub(crate) async fn send_request(
    method: &str,
    base_url: Option<String>,
    path: &str,
    // body: Option<String>,
    args: Option<std::collections::HashMap<String, String>>,
    // headers: Option<Vec<(&str, &str)>>,
    headers: Option<std::collections::HashMap<String, String>>,
    cryptor: wallet_utils::cbc::AesCbcCryptor,
) -> Result<std::collections::HashMap<String, serde_json::Value>, crate::Error> {
    let method = wallet_utils::parse_func::method_from_str(method)?;

    let base_url = base_url.unwrap_or(BASE_URL.to_string());
    let url = format!("{base_url}/{path}");
    let client = reqwest::Client::new();
    // let mut request = client.request(method, url);

    // 如果方法是 GET，则将参数添加到 URL
    let mut request = if method == reqwest::Method::GET {
        let args = args
            .unwrap_or_default()
            .into_iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect::<Vec<String>>()
            .join("&");
        let url_with_params =
            if !args.is_empty() { format!("{}?{}", url, args) } else { url.to_string() };
        client.request(method, url_with_params)
    } else {
        // 如果不是 GET 方法，则将参数作为请求体发送
        let body_content = wallet_utils::serde_func::serde_to_string(&args)?;
        client.request(method, url).body(body_content)
    };

    for (key, value) in headers.unwrap_or_default() {
        request = request.header(key, value);
    }

    let response = request.send().await.map_err(|e| wallet_utils::Error::Http(e.into()))?;

    if !response.status().is_success() {
        return Err(wallet_utils::Error::Http(wallet_utils::HttpError::NonSuccessStatus(
            response.status(),
        ))
        .into());
    }
    let res = response.json::<String>().await.map_err(|e| wallet_utils::Error::Http(e.into()))?;
    let res: BackendResponse = wallet_utils::serde_func::serde_from_str(&res)?;

    res.process(&cryptor)
}

#[cfg(test)]
mod test {

    #[tokio::test]
    async fn test() -> Result<(), Box<dyn std::error::Error>> {
        let client = reqwest::Client::builder().build()?;

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse()?);

        let data = r#"{
        "deviceType": "ANDROID",
        "sn": "2",
        "code": "2",
        "systemVer": "3",
        "iemi": "4",
        "meid": "5",
        "iccid": "6",
        "mem": "7"
    }"#;

        let json: serde_json::Value = serde_json::from_str(&data)?;

        let request = client
            .request(reqwest::Method::POST, "http://api.wallet.net/device/init")
            .headers(headers)
            .json(&json);

        let response = request.send().await?;
        let body = response.text().await?;

        println!("{}", body);

        Ok(())
    }
}
