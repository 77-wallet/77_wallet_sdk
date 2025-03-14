pub struct UserProperty {
    #[allow(dead_code)]
    pub(crate) content: String,
    pub(crate) client_id: String,
    pub(crate) username: String,
    pub(crate) password: String,
    pub(crate) app_version: String,
}

impl UserProperty {
    pub fn new(
        content: &str,
        client_id: &str,
        username: &str,
        password: &str,
        app_version: &str,
    ) -> Self {
        Self {
            content: content.to_string(),
            client_id: client_id.to_string(),
            username: username.to_string(),
            password: password.to_string(),
            app_version: app_version.to_string(),
        }
    }

    pub fn to_vec(&self) -> Vec<(String, String)> {
        let up = vec![
            // ("content".to_string(), self.content.clone()),
            // ("clientId".to_string(), self.client_id.clone()),
            ("username".to_string(), self.username.clone()),
            ("password".to_string(), self.password.clone()),
            ("appVersion".to_string(), self.app_version.clone()),
        ];

        up
    }
}
