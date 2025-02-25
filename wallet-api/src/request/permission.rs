use wallet_chain_interact::tron::operations::{
    multisig::{Keys, Permission},
    permisions::PermissionTypes,
};

pub struct PermissionReq {
    pub address: String,
    pub name: String,
    pub permission_id: Option<i8>,
    pub threshold: i8,
    pub operations: Vec<i8>,
    pub keys: Vec<KeysReq>,
}

impl PermissionReq {
    pub fn check_threshold(&self) -> Result<(), crate::BusinessError> {
        let weight = self.keys.iter().map(|k| k.weight).sum::<i8>();
        if weight < self.threshold {
            return Err(crate::BusinessError::Permisison(
                crate::PermissionError::WeightLessThreshold,
            ))?;
        }
        Ok(())
    }
}

impl TryFrom<&PermissionReq> for Permission {
    type Error = crate::ServiceError;

    fn try_from(value: &PermissionReq) -> Result<Self, Self::Error> {
        let operations = PermissionTypes::from_i8(value.operations.clone())?;

        let mut keys = vec![];
        for item in value.keys.iter() {
            let key = Keys::new(&item.address, item.weight)?;
            keys.push(key);
        }

        Ok(Permission::new_actives(
            value.name.clone(),
            operations.to_hex(),
            value.threshold as u8,
            keys,
        ))
    }
}

pub struct KeysReq {
    pub address: String,
    pub weight: i8,
}
