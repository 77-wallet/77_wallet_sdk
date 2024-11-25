use super::SeedWallet;

pub(crate) struct SeedEncryptorBuilder<'a, P, R, B, S> {
    keypath: P,
    rng: &'a mut R,
    data: B,
    password: S,
    name: Option<&'a str>,
}

impl<'a, P, R, B, S> SeedEncryptorBuilder<'a, P, R, B, S>
where
    P: AsRef<std::path::Path>,
    R: rand::Rng + rand::CryptoRng,
    B: AsRef<[u8]>,
    S: AsRef<[u8]>,
{
    pub(crate) fn new(
        keypath: P,
        rng: &'a mut R,
        data: B,
        password: S,
        name: Option<&'a str>,
    ) -> Self {
        SeedEncryptorBuilder {
            keypath,
            rng,
            data,
            password,
            name,
        }
    }
}

impl<P, R, B, S> crate::wallet::WalletEncrypt for SeedEncryptorBuilder<'_, P, R, B, S>
where
    P: AsRef<std::path::Path>,
    R: rand::Rng + rand::CryptoRng,
    B: AsRef<[u8]>,
    S: AsRef<[u8]>,
{
    type Output = (SeedWallet, String);

    fn encrypt_keystore(self) -> Result<Self::Output, crate::Error> {
        let data = self.data.as_ref();
        let uuid = crate::eth_keystore::encrypt_data(
            self.keypath,
            self.rng,
            data,
            self.password,
            self.name,
        )?;
        Ok((SeedWallet::from_seed(data.to_vec())?, uuid))
    }
}

pub(crate) struct SeedDecryptorBuilder<P, S> {
    keypath: P,
    password: S,
}

impl<P, S> SeedDecryptorBuilder<P, S>
where
    P: AsRef<std::path::Path>,
    S: AsRef<[u8]>,
{
    pub(crate) fn new(keypath: P, password: S) -> Self {
        SeedDecryptorBuilder { keypath, password }
    }
}

impl<'a, P, S> crate::wallet::WalletDecrypt for SeedDecryptorBuilder<P, S>
where
    P: AsRef<std::path::Path>,
    S: AsRef<[u8]>,
{
    type Output = SeedWallet;

    fn decrypt_keystore(self) -> Result<Self::Output, crate::Error> {
        let seed = crate::eth_keystore::decrypt_data(self.keypath, self.password)?;
        Ok(SeedWallet::from_seed(seed)?)
    }
}
