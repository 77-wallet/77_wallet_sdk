use wallet_transport_backend::request::SendMsgConfirm;

pub(crate) struct TaskQueueDomain;

impl TaskQueueDomain {
    pub async fn send_msg_confirm(ids: Vec<SendMsgConfirm>) -> Result<(), crate::ServiceError> {
        if !ids.is_empty() {
            const BATCH_SIZE: usize = 500;
            for chunk in ids.chunks(BATCH_SIZE) {
                let api = crate::Context::get_global_backend_api()?;
                let aes_cbc_cryptor = crate::Context::get_global_aes_cbc_cryptor()?;
                // tracing::info!("send_msg_confirm: {}", chunk.len());
                api.send_msg_confirm(
                    aes_cbc_cryptor,
                    &wallet_transport_backend::request::SendMsgConfirmReq::new(chunk.to_vec()),
                )
                .await?;
            }
        }
        Ok(())
    }
}
