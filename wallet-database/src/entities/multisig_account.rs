use super::{
    has_expiration,
    multisig_member::{MemberVo, MultisigMemberEntities, MultisigMemberEntity, NewMemberEntity},
};
use sqlx::types::chrono::{DateTime, Utc};
use wallet_types::chain::address::{category::BtcAddressCategory, r#type::BtcAddressType};

#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MultisigAccountEntity {
    pub id: String,
    /// 多签钱包名称
    pub name: String,
    /// 发起方地址
    pub initiator_addr: String,
    /// 多签钱包地址
    pub address: String,
    pub address_type: String,
    /// 管理地址(sol有)
    pub authority_addr: String,
    /// 确认状态(0失败1确认中2确认完成3上链)
    pub status: i8,
    /// 服务费状态(0未支付1已支付)
    pub pay_status: i8,
    /// 所有者(0不是 1是,2,即使参与方也是所有者)
    pub owner: i8,
    pub chain_code: String,
    /// 阈值
    pub threshold: i32,
    /// 成员数量
    pub member_num: i32,
    /// salt
    // #[serde(skip_serializing)]
    pub salt: String,
    /// 部署交易hash
    // #[serde(skip_serializing)]
    pub deploy_hash: String,
    /// 服务费交易hash
    pub fee_hash: String,
    // 部署费用在那个链上
    // #[serde(skip_deserializing)]
    pub fee_chain: String,
    pub is_del: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl MultisigAccountEntity {
    // 无交易
    pub const NONE_TRANS_HASH: &str = "NONE_TRANS";

    pub fn address_type(&self) -> Option<String> {
        (!self.address_type.is_empty()).then(|| self.address_type.clone())
    }

    pub fn address_type_to_category(&mut self) {
        if !self.address_type.is_empty() {
            let address_type = BtcAddressType::try_from(self.address_type.as_ref()).unwrap();
            let category = BtcAddressCategory::from(address_type);
            self.address_type = category.to_string();
        }
    }

    // 是否过期验证(使用了最后的更新时间)
    pub fn expiration_check(&self) -> bool {
        let chain_code = if !self.fee_chain.is_empty() {
            wallet_types::chain::chain::ChainCode::try_from(self.fee_chain.as_str()).unwrap()
        } else {
            wallet_types::chain::chain::ChainCode::try_from(self.chain_code.as_str()).unwrap()
        };

        let timestamp = self
            .updated_at
            .unwrap_or(wallet_utils::time::now())
            .timestamp();

        has_expiration(timestamp, chain_code)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MultisigAccountStatus {
    // 等待确认
    Pending = 1,
    // 确认完成(待部署)
    Confirmed,
    // 上链(成功)
    OnChain,
    // 上链失败
    OnChainFail,
    // 上链确认中
    OnChianPending,
}
impl MultisigAccountStatus {
    pub fn to_i8(&self) -> i8 {
        match self {
            MultisigAccountStatus::Pending => 1,
            MultisigAccountStatus::Confirmed => 2,
            MultisigAccountStatus::OnChain => 3,
            MultisigAccountStatus::OnChainFail => 4,
            MultisigAccountStatus::OnChianPending => 5,
        }
    }
}

impl TryFrom<i8> for MultisigAccountStatus {
    type Error = crate::Error;
    fn try_from(value: i8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(MultisigAccountStatus::Pending),
            2 => Ok(MultisigAccountStatus::Confirmed),
            3 => Ok(MultisigAccountStatus::OnChain),
            4 => Ok(MultisigAccountStatus::OnChainFail),
            5 => Ok(MultisigAccountStatus::OnChianPending),
            _ => Err(crate::Error::Other(format!(
                "account status {} not support",
                value
            ))),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MultisigAccountPayStatus {
    // 未支付
    Unpaid,
    // 已支付
    Paid,
    // 支付失败
    PaidFail,
    // 支付确认中
    PaidPending,
}
impl MultisigAccountPayStatus {
    pub fn to_i8(&self) -> i8 {
        match self {
            MultisigAccountPayStatus::Unpaid => 0,
            MultisigAccountPayStatus::Paid => 1,
            MultisigAccountPayStatus::PaidFail => 2,
            MultisigAccountPayStatus::PaidPending => 3,
        }
    }
}

impl TryFrom<i8> for MultisigAccountPayStatus {
    type Error = crate::Error;
    fn try_from(value: i8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(MultisigAccountPayStatus::Unpaid),
            1 => Ok(MultisigAccountPayStatus::Paid),
            2 => Ok(MultisigAccountPayStatus::PaidFail),
            3 => Ok(MultisigAccountPayStatus::PaidPending),
            _ => Err(crate::Error::Other(format!(
                "account pay status status {} not support",
                value
            ))),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MultiAccountOwner {
    Participant,
    Owner,
    Both,
}
impl MultiAccountOwner {
    pub fn to_i8(&self) -> i8 {
        match self {
            // 参与方
            MultiAccountOwner::Participant => 0,
            // 自己是创建者
            MultiAccountOwner::Owner => 1,
            // 自己又是创建者，还有一个账号是参与方
            MultiAccountOwner::Both => 2,
        }
    }
}

#[derive(Debug)]
pub struct NewMultisigAccountEntity {
    pub id: String,
    pub name: String,
    pub initiator_addr: String,
    pub address: String,
    pub authority_addr: String,
    pub address_type: String,
    pub status: MultisigAccountStatus,
    pub owner: MultiAccountOwner,
    pub pay_status: MultisigAccountPayStatus,
    pub chain_code: String,
    pub threshold: i32,
    pub member_num: i32,
    pub salt: String,
    pub is_del: i64,
    pub deploy_hash: String,
    pub fee_hash: String,
    pub fee_chain: String,
    pub member_list: Vec<NewMemberEntity>,
    pub create_at: DateTime<Utc>,
}

impl NewMultisigAccountEntity {
    pub fn new(
        id: Option<String>,
        name: String,
        initiator_addr: String,
        address: String,
        chain_code: String,
        threshold: i32,
        address_type: String,
        member_list: Vec<MemberVo>,
        uids: &std::collections::HashSet<String>,
    ) -> Self {
        let id = id.unwrap_or_else(|| {
            let id = wallet_utils::snowflake::get_uid().unwrap();
            id.to_string()
        });

        let mut member = Vec::new();
        for item in member_list {
            let address = item.address;
            let name = item.name;
            let confirmed = item.confirmed;
            let is_self = if uids.contains(&item.uid) { 1 } else { 0 };
            let pubkey = item.pubkey;
            let uid = item.uid;
            member.push(NewMemberEntity {
                account_id: id.clone(),
                address,
                name,
                confirmed,
                is_self,
                pubkey,
                uid,
            });
        }

        NewMultisigAccountEntity {
            id,
            name,
            initiator_addr,
            address,
            authority_addr: "".to_string(),
            address_type,
            status: MultisigAccountStatus::Pending,
            pay_status: MultisigAccountPayStatus::Unpaid,
            owner: MultiAccountOwner::Owner,
            chain_code,
            threshold,
            salt: "".to_string(),
            deploy_hash: "".to_string(),
            fee_hash: "".to_string(),
            fee_chain: "".to_string(),
            is_del: 0,
            member_num: member.len() as i32,
            member_list: member,
            create_at: Utc::now(),
        }
    }

    pub fn with_authority_addr(mut self, authority_addr: String) -> Self {
        self.authority_addr = authority_addr;
        self
    }

    pub fn with_deploy_hash(mut self, deploy_hash: &str) -> Self {
        self.deploy_hash = deploy_hash.to_string();
        self
    }

    pub fn with_fee_hash(mut self, fee_hash: &str) -> Self {
        self.fee_hash = fee_hash.to_string();
        self
    }

    pub fn with_status(mut self, status: MultisigAccountStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_pay_status(mut self, pay_status: MultisigAccountPayStatus) -> Self {
        self.pay_status = pay_status;
        self
    }

    pub fn with_address_type(mut self, address_type: String) -> Self {
        self.address_type = address_type;
        self
    }

    pub fn with_salt(mut self, salt: String) -> Self {
        self.salt = salt;
        self
    }

    pub fn owner_list(&self) -> Vec<String> {
        let mut owners = self
            .member_list
            .iter()
            .map(|x| x.address.to_string())
            .collect::<Vec<String>>();
        owners.sort();
        owners
    }

    pub fn to_multisig_account_data(&self) -> MultisigAccountData {
        let account = MultisigAccountEntity {
            id: self.id.clone(),
            name: self.name.clone(),
            initiator_addr: self.initiator_addr.clone(),
            address: self.address.clone(),
            address_type: self.address_type.clone(),
            authority_addr: self.authority_addr.clone(),
            status: self.status.to_i8(),
            pay_status: self.pay_status.to_i8(),
            owner: self.owner.to_i8(),
            chain_code: self.chain_code.clone(),
            threshold: self.threshold,
            member_num: self.member_num,
            salt: self.salt.clone(),
            deploy_hash: "".to_string(),
            fee_hash: "".to_string(),
            fee_chain: self.fee_chain.to_string(),
            is_del: 0,
            created_at: wallet_utils::time::now(),
            updated_at: None,
        };

        let mut member = vec![];
        for item in self.member_list.iter() {
            let m = MultisigMemberEntity {
                account_id: self.id.clone(),
                address: item.address.clone(),
                name: item.name.clone(),
                confirmed: item.confirmed,
                is_self: item.is_self,
                pubkey: item.pubkey.clone(),
                uid: item.uid.clone(),
                created_at: wallet_utils::time::now(),
                updated_at: None,
            };
            member.push(m);
        }

        MultisigAccountData {
            account,
            members: MultisigMemberEntities(member),
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize, sqlx::FromRow, Clone)]
pub struct MultisigAccountData {
    pub account: MultisigAccountEntity,
    pub members: MultisigMemberEntities,
}

impl MultisigAccountData {
    pub fn new(account: MultisigAccountEntity, members: MultisigMemberEntities) -> Self {
        Self { account, members }
    }

    pub fn to_string(&self) -> Result<String, crate::Error> {
        Ok(wallet_utils::hex_func::bincode_encode(self)?)
    }

    pub fn from_string(data: &str) -> Result<Self, crate::Error> {
        Ok(wallet_utils::hex_func::bincode_decode(data)?)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_bincode_decode() {
        let raw = "12000000000000003235333938353334363536323432383932380e000000000000004d756c74697369672d6274632d312a00000000000000626331716d747a356138613564356a6a333639666b7a68636e717138636366376736616c7077356477303e00000000000000626331703064336d386778303330786377306839766e3375767264716e7730706832746333326e387071796b7779336467323932653775737533713279380700000000000000703274722d7368400000000000000063623936613930376631343439336535653735346663333238666265303962386366333465323462656435656234356361643235633561643266313366343339030102030000000000000062746302000000020000008c00000000000000323031343966323464313932366666643161303338626364633964343465393233666639633331623763333332353737643663343337303033633766356264326262616332303632393965353431643238626336666263306133333632333866333833386437333037383136383065653732316432656635626536656165376261306631616462613532613200000000000000000000000000000000000000000000000000000000000000001400000000000000323032352d30342d32345430363a35323a30355a011400000000000000323032352d30342d32345430373a33303a30385a020000000000000012000000000000003235333938353334363536323432383932382a000000000000006263317167716a6b76306c7230783476716a74396c6b38376b6e6877753775683068336a6878703665630300000000000000626f62010142000000000000003033313439663234643139323666666431613033386263646339643434653932336666396333316237633333323537376436633433373030336337663562643262624000000000000000323166383761306634356166636639336337613561326337643334623638396638313039326136373431643432323363666138313136386538616438303731661400000000000000323032352d30342d32345430363a35323a30355a0012000000000000003235333938353334363536323432383932382a00000000000000626331716d747a356138613564356a6a333639666b7a68636e717138636366376736616c7077356477300500000000000000616c696365010142000000000000003033363239396535343164323862633666626330613333363233386633383338643733303738313638306565373231643265663562653665616537626130663161644000000000000000323166383761306634356166636639336337613561326337643334623638396638313039326136373431643432323363666138313136386538616438303731661400000000000000323032352d30342d32345430363a35323a30355a00";
        // let raw = "12000000000000003139383534343834373239393238343939321200000000000000e68891e79a84e5a49ae7adbee8b4a6e688b7220000000000000054514a53415a6a3454357139424862513148677750484d7264385048683831765165220000000000000054514a53415a6a3454357139424862513148677750484d726438504868383176516500000000000000000000000000000000020001040000000000000074726f6e0200000003000000000000000000000000000000000000000000000000000000000000000000000001000000000000001400000000000000323032342d31312d32325430373a31313a32305a011400000000000000323032342d31312d32325430373a31313a32315a030000000000000012000000000000003139383534343834373239393238343939322200000000000000544669626356586e556977346138447835356558466e50436a6f66675a4e627242760600000000000000e794bbe794bb0101820000000000000030343035373531333142393331464541443236424230373034313538413030394539464438413635423133463132454230323845424533364639303736394546353533314530393541313930453434333938383035433643353132373330303531443744364337414245443731373631443133334341383242384536413431463844200000000000000063343437333138623934313739363134643730653530363434323333623330611400000000000000323032342d31312d32325430373a31313a32305a001200000000000000313938353434383437323939323834393932220000000000000054486f6f34644150467379483373636b6d47626e4d734b4e7434476f72373972384a0600000000000000e6809de6809d0101820000000000000030344536393834334444444442434244343330454243314435413836333443384643334130373643333138353141443632384435364334353337383842444443393437384245343330383743374335314636343444433741413030324239444634313230394146453144384346383642373043313237363835314245303635414330200000000000000063343437333138623934313739363134643730653530363434323333623330611400000000000000323032342d31312d32325430373a31313a32305a001200000000000000313938353434383437323939323834393932220000000000000054514a53415a6a3454357139424862513148677750484d72643850486838317651650900000000000000e58f91e8b5b7e4baba0101820000000000000030343835354138383432313141424644393936463042443335363139433334443834363445323437413441423645393445303442363635324442323738384144333431353131363136433736443431343944423944344339393830304542333242314531303833324233334135354439314144363730374435413843304134383346200000000000000063343437333138623934313739363134643730653530363434323333623330611400000000000000323032342d31312d32325430373a31313a32305a00";
        let res = wallet_utils::hex_func::bincode_decode::<MultisigAccountData>(raw);

        println!("{:#?}", res);
    }
}
