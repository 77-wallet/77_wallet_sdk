pub struct UserProperty {
    #[allow(dead_code)]
    pub(crate) content: String,
    pub(crate) client_id: String,
    pub(crate) username: String,
    pub(crate) password: String,
}

impl UserProperty {
    pub fn new(content: &str, client_id: &str, username: &str, password: &str) -> Self {
        Self {
            content: content.to_string(),
            client_id: client_id.to_string(),
            username: username.to_string(),
            password: password.to_string(),
        }
    }

    pub fn to_vec(&self) -> Vec<(String, String)> {
        let up = vec![
            ("username".to_string(), self.username.clone()),
            ("password".to_string(), self.password.clone()),
        ];

        up
    }
}
