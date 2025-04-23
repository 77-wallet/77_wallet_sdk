use crate::get_manager;
use sqlx::types::chrono::{TimeZone, Utc};
use wallet_api::request::transaction::ServiceFeePayer;
use wallet_database::entities::{
    multisig_account::{MultisigAccountData, MultisigAccountEntity},
    multisig_member::{MultisigMemberEntities, MultisigMemberEntity},
};

#[tokio::test]
async fn test_fetch_deposit_address() {
    let wallet_manager = get_manager().await;
    let chain_code = "tron".to_string();
    let fee = wallet_manager.fetch_deposit_address(chain_code).await;
    tracing::info!("{}", serde_json::to_string(&fee).unwrap());
}

#[tokio::test]
async fn test_get_service_fee() {
    let wallet_manager = get_manager().await;

    let chain_code = "tron".to_string();
    let pay_chain = "tron".to_string();
    let pay_address = "TFkcfwVQpB6HySzHqNiXSWcsp2g2c9qduX".to_string();
    let info = wallet_manager
        .get_multisig_service_fee(pay_chain, chain_code, pay_address)
        .await;

    tracing::info!("{}", serde_json::to_string(&info).unwrap());
}

#[tokio::test]
async fn test_deploy_multisig_fee() {
    let wallet_manager = get_manager().await;
    let account_id = "193751654670143488".to_string();
    let fee = wallet_manager.get_account_fee(account_id).await;

    tracing::info!("{}", serde_json::to_string(&fee).unwrap());
}

#[tokio::test]
async fn test_deploy_multisig_account() {
    let wallet_manager = get_manager().await;

    let account_id = "249341508602433536".to_string();

    let fee_setting = r#"{"gasLimit": 262975,"baseFee": "3329262291","priorityFee": "0","maxFeePerGas": "3995114749"}"#.to_string();
    let fee_setting = Some(fee_setting);
    // let fee_setting = None;

    let payer = ServiceFeePayer {
        from: "TNAAhuax96f8j1Azy2kVayYVcBCW8y6aYo".to_string(),
        chain_code: "tron".to_string(),
        symbol: "USDT".to_string(),
        fee_setting: fee_setting.clone(),
        request_resource_id: None,
    };
    let deploy_fee = fee_setting;
    let password = "123456".to_string();

    let res = wallet_manager
        .deploy_multisig_account(account_id, deploy_fee, Some(payer), password)
        .await;
    tracing::info!("部署多签合约{:?}", res);
}

#[tokio::test]
async fn test_multisig_account_list() {
    let wallet_manager = get_manager().await;

    let chain_code = Some("sol".to_string());
    let list = wallet_manager
        .multisig_account_lists(true, chain_code, 0, 1)
        .await;

    tracing::info!("account list{}", serde_json::to_string(&list).unwrap());
}

#[tokio::test]
async fn test_cancel_account() {
    let wallet_manager = get_manager().await;

    let id = "253661045695057920".to_string();
    let list = wallet_manager.cancel_multisig(id).await;
    println!("{:?}", list);
}

#[tokio::test]
async fn test_multisig_account_info() {
    let wallet_manager = get_manager().await;

    let address = "187341364088934400".to_string();
    let info = wallet_manager.multisig_account_by_id(address).await;

    tracing::info!("{}", serde_json::to_string(&info).unwrap());
}

#[tokio::test]
async fn test_multisig_account_info_by_address() {
    let wallet_manager = get_manager().await;

    let address = "TXDK1qjeyKxDTBUeFyEQiQC7BgDpQm64g1".to_string();
    let info = wallet_manager.multisig_account_by_address(address).await;

    tracing::info!("{}", serde_json::to_string(&info).unwrap());
}

#[tokio::test]
async fn test_check_participant_exists() {
    let wallet_manager = get_manager().await;

    let id = "185879020414570496".to_string();
    let info = wallet_manager.check_participant_exists(id).await;

    tracing::info!("{}", serde_json::to_string(&info).unwrap());
}

#[tokio::test]
async fn test_confirm_participation() {
    let wallet_manager = get_manager().await;

    let id = "245308480033001472".to_string();
    let info = wallet_manager.confirm_participation(id).await;

    tracing::info!("{}", serde_json::to_string(&info).unwrap());
}

#[tokio::test]
async fn test_update_multisig_name() {
    let wallet_manager = get_manager().await;

    let id = "173938720146329600".to_string();
    let name = "name11".to_string();
    let info = wallet_manager.update_multisig_name(id, name).await;

    tracing::info!("{:?}", serde_json::to_string(&info).unwrap());
}

#[tokio::test]
async fn test_whether_multisig_address() {
    let wallet_manager = get_manager().await;

    let address = "TQJSAZj4T5q9BHbQ1HgwPHMrd8PHh81vQe".to_string();
    let chain_code = "tron".to_string();

    let info = wallet_manager
        .whether_multisig_address(address, chain_code)
        .await;

    tracing::info!("{}", serde_json::to_string(&info).unwrap());
}

#[tokio::test]
async fn test_build_raw_data() {
    let member = vec![
        MultisigMemberEntity {
            account_id: "194550757817716736".to_string(),
            address: "TT4QgNx2rVD35tYU1LJ6tH5Ya1bxmannBK".to_string(),
            name: "发起者".to_string(),
            confirmed: 1,
            is_self: 1,
            pubkey: "TT4QgNx2rVD35tYU1LJ6tH5Ya1bxmannBK".to_string(),
            uid: "332bc9a8d2c52bd13a2f65ddbc393dacf0e2fab7bf6eaa0b787d465aa1dee897".to_string(),
            created_at: Utc.with_ymd_and_hms(2024, 11, 8, 12, 12, 11).unwrap(),
            updated_at: Some(Utc.with_ymd_and_hms(2024, 11, 8, 12, 12, 11).unwrap()),
        },
        MultisigMemberEntity {
            account_id: "194550757817716736".to_string(),
            address: "TRbHD77Y6WWDaz9X5esrVKwEVwRM4gTw6N".to_string(),
            name: "bob".to_string(),
            confirmed: 1,
            is_self: 0,
            pubkey: "TRbHD77Y6WWDaz9X5esrVKwEVwRM4gTw6N".to_string(),
            uid: "71512c7dcca484ad9a03a0f7798e7bdd45602891ed464e0a541657137328d92d".to_string(),
            created_at: Utc.with_ymd_and_hms(2024, 11, 8, 12, 12, 11).unwrap(),
            updated_at: Some(Utc.with_ymd_and_hms(2024, 11, 8, 12, 12, 11).unwrap()),
        },
        // MultisigMemberEntity {
        //     account_id: "194987302026612736".to_string(),
        //     address: "TSGj3fY4iVG84xcWsKEEY5MqxW4cEJ373z".to_string(),
        //     name: "".to_string(),
        //     confirmed: 1,
        //     is_self: 1,
        //     pubkey: "TSGj3fY4iVG84xcWsKEEY5MqxW4cEJ373z".to_string(),
        //     uid: "11e88fc2f20f44a1e48a10db95a45a2d30c8daacc38cb4b4fe8804f53d820258".to_string(),
        //     created_at: Utc.with_ymd_and_hms(2024, 11, 8, 12, 12, 11).unwrap(),
        //     updated_at: Some(Utc.with_ymd_and_hms(2024, 11, 8, 12, 12, 11).unwrap()),
        // },
    ];

    let members = MultisigMemberEntities(member);
    let multisig_data = MultisigAccountData {
        account: MultisigAccountEntity {
            id: "194550757817716736".to_string(),
            name: "恢复多签2.0".to_string(),
            initiator_addr: "TT4QgNx2rVD35tYU1LJ6tH5Ya1bxmannBK".to_string(),
            address: "TT4QgNx2rVD35tYU1LJ6tH5Ya1bxmannBK".to_string(),
            address_type: "".to_string(),
            authority_addr: "".to_string(),
            status: 3,
            pay_status: 1,
            owner: 1,
            chain_code: "tron".to_string(),
            threshold: 2,
            member_num: 2,
            salt: "".to_string(),
            deploy_hash: "e5a74e07ae423402f8ec3de04703c2461c05352e35f3d15b1f70f8a7d4009174"
                .to_string(),
            fee_hash: "dee015db66c559b6e19d120d2afdf9a0f6b02e5188227c620caec56d2bdbf580"
                .to_string(),
            fee_chain: "tron".to_string(),
            is_del: 0,
            created_at: Utc.with_ymd_and_hms(2024, 11, 8, 12, 12, 11).unwrap(),
            updated_at: Some(Utc.with_ymd_and_hms(2024, 11, 8, 12, 12, 11).unwrap()),
        },
        members,
    };
    println!("{}", multisig_data.to_string().unwrap())
}
