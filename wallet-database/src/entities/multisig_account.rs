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

        let timestamp = self.updated_at.unwrap_or(wallet_utils::time::now()).timestamp();

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
            _ => Err(crate::Error::Other(format!("account status {} not support", value))),
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
            _ => {
                Err(crate::Error::Other(format!("account pay status status {} not support", value)))
            }
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
        let mut owners =
            self.member_list.iter().map(|x| x.address.to_string()).collect::<Vec<String>>();
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

        MultisigAccountData { account, members: MultisigMemberEntities(member) }
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
        let raw = "1200000000000000333730393733373536303630393935353834070000000000000033e5a49ae7adbe2a000000000000006263317170343772377536723437726d647a35343963656c7737346a70337a6d6c6d7338746a6d6a64732a000000000000006263317170343772377536723437726d647a35343963656c7737346a70337a6d6c6d7338746a6d6a647305000000000000007032777368000000000000000003010103000000000000006274630200000002000000000000000000000000000000000000000000000000000000000000000000000000000000000000001400000000000000323032362d30332d31335430323a35373a31315a011400000000000000323032362d30332d31335430333a30323a34325a02000000000000001200000000000000333730393733373536303630393935353834220000000000000031466270786836654c564e3257727731796d42516d4d4a524e6a63686663736b396b0000000000000000010042000000000000003033616165373031333339376135663763633032633661653630613732666333613762613366333232386434623235323066343136333766373365353834373234614000000000000000316430643433363038633136313132383963373634336236323439663964386566636235626261303132613633623234383535636433323164333537646131311400000000000000323032362d30332d31335430323a35373a31315a0012000000000000003337303937333735363036303939353538342a000000000000006263317170343772377536723437726d647a35343963656c7737346a70337a6d6c6d7338746a6d6a64730900000000000000e58f91e8b5b7e4baba010142000000000000003032366334653530653836386461313736623162386165383736376564303464656633623139323934333038383134363964383133333837366164383739313438624000000000000000633235626362623063626536356235356435373962666438343834306661626332386131323834313034326535636436666234333331616161363736396466331400000000000000323032362d30332d31335430323a35373a31315a00";
        // let raw = "12000000000000003139383534343834373239393238343939321200000000000000e68891e79a84e5a49ae7adbee8b4a6e688b7220000000000000054514a53415a6a3454357139424862513148677750484d7264385048683831765165220000000000000054514a53415a6a3454357139424862513148677750484d726438504868383176516500000000000000000000000000000000020001040000000000000074726f6e0200000003000000000000000000000000000000000000000000000000000000000000000000000001000000000000001400000000000000323032342d31312d32325430373a31313a32305a011400000000000000323032342d31312d32325430373a31313a32315a030000000000000012000000000000003139383534343834373239393238343939322200000000000000544669626356586e556977346138447835356558466e50436a6f66675a4e627242760600000000000000e794bbe794bb0101820000000000000030343035373531333142393331464541443236424230373034313538413030394539464438413635423133463132454230323845424533364639303736394546353533314530393541313930453434333938383035433643353132373330303531443744364337414245443731373631443133334341383242384536413431463844200000000000000063343437333138623934313739363134643730653530363434323333623330611400000000000000323032342d31312d32325430373a31313a32305a001200000000000000313938353434383437323939323834393932220000000000000054486f6f34644150467379483373636b6d47626e4d734b4e7434476f72373972384a0600000000000000e6809de6809d0101820000000000000030344536393834334444444442434244343330454243314435413836333443384643334130373643333138353141443632384435364334353337383842444443393437384245343330383743374335314636343444433741413030324239444634313230394146453144384346383642373043313237363835314245303635414330200000000000000063343437333138623934313739363134643730653530363434323333623330611400000000000000323032342d31312d32325430373a31313a32305a001200000000000000313938353434383437323939323834393932220000000000000054514a53415a6a3454357139424862513148677750484d72643850486838317651650900000000000000e58f91e8b5b7e4baba0101820000000000000030343835354138383432313141424644393936463042443335363139433334443834363445323437413441423645393445303442363635324442323738384144333431353131363136433736443431343944423944344339393830304542333242314531303833324233334135354439314144363730374435413843304134383346200000000000000063343437333138623934313739363134643730653530363434323333623330611400000000000000323032342d31312d32325430373a31313a32305a00";
        let res = wallet_utils::hex_func::bincode_decode::<MultisigAccountData>(raw);

        println!("{:#?}", res);
    }
}
