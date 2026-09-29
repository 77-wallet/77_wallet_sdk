use std::{
    collections::HashSet,
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};

use wallet_api::domain::chain::adapter::MultisigAdapter;
use wallet_database::{
    entities::{
        api_chain::NodeBindType,
        chain::{ChainCreateVo, ChainWithNode},
        multisig_account::NewMultisigAccountEntity,
        multisig_member::MemberVo,
        node::NodeCreateVo,
    },
    repositories::{
        chain::ChainRepo, multisig_account::MultisigAccountRepo,
        multisig_member::MultisigMemberRepo, node::NodeRepo,
    },
};
use wallet_types::chain::chain::ChainCode;

use crate::harness::{TestEnv, ensure_env, next_tag, open_core_pool};

pub(super) struct EvmDeployScenario {
    env: &'static TestEnv,
    account_id: String,
    chain_code: String,
    rpc_url: String,
}

impl EvmDeployScenario {
    pub(super) async fn given(chain_code: &str, chain_name: &str, rpc_url: &str) -> Self {
        let env = ensure_env().await;
        let pool = open_core_pool(&env.db_dir).await;
        let node_id = next_tag("evm-safe-node");
        ChainRepo::add(
            &pool,
            ChainCreateVo::new(
                chain_name,
                chain_code,
                &["evm".to_string()],
                NodeBindType::ManualUser,
                "SAFE",
            ),
        )
        .await
        .expect("seed EVM chain");
        NodeRepo::upsert(
            &pool,
            NodeCreateVo::new(&node_id, "Safe fake RPC", chain_code, rpc_url, None),
        )
        .await
        .expect("seed EVM node");
        ChainRepo::set_chain_node_with_type(&pool, chain_code, &node_id, NodeBindType::ManualUser)
            .await
            .expect("select EVM node");

        let account_id = next_tag("evm-safe-account");
        let account = NewMultisigAccountEntity::new(
            Some(account_id.clone()),
            format!("{chain_name} preflight test"),
            "0x1111111111111111111111111111111111111111".to_string(),
            "".to_string(),
            chain_code.to_string(),
            1,
            "".to_string(),
            vec![MemberVo::new(
                "owner".to_string(),
                "0x1111111111111111111111111111111111111111".to_string(),
            )],
            &HashSet::new(),
        );
        MultisigAccountRepo::create_with_member(&pool, &account)
            .await
            .expect("seed EVM multisig account");
        Self { env, account_id, chain_code: chain_code.to_string(), rpc_url: rpc_url.to_string() }
    }

    pub(super) async fn when_derive_address(&self) -> Result<(String, String), String> {
        let pool = open_core_pool(&self.env.db_dir).await;
        let account = MultisigAccountRepo::find_by_id(&pool, &self.account_id)
            .await
            .map_err(|error| format!("{error:?}"))?
            .expect("account exists");
        let members = MultisigMemberRepo::list_by_account_id(&pool, &self.account_id)
            .await
            .map_err(|error| format!("{error:?}"))?;
        let node = ChainWithNode {
            chain_code: self.chain_code.clone(),
            rpc_url: self.rpc_url.clone(),
            network: "mainnet".to_string(),
            ..Default::default()
        };
        let chain_code =
            ChainCode::try_from(self.chain_code.as_str()).map_err(|error| format!("{error:?}"))?;
        let adapter =
            MultisigAdapter::new(chain_code, node, None).map_err(|error| format!("{error:?}"))?;
        let address = adapter
            .multisig_address(&account, &members)
            .await
            .map_err(|error| format!("{error:?}"))?;
        Ok((address.multisig_address, address.salt))
    }

    pub(super) async fn when_deploy_without_payer(&self) -> Result<(), String> {
        self.env
            .manager
            .deploy_multisig_account(self.account_id.clone(), None, None, "unused".to_string())
            .await
            .map_err(|error| format!("{error:?}"))
    }

    pub(super) async fn then_account_unchanged(&self) {
        let pool = open_core_pool(&self.env.db_dir).await;
        let account = MultisigAccountRepo::find_by_id(&pool, &self.account_id)
            .await
            .expect("load account")
            .expect("account exists");
        assert_eq!(account.pay_status, 0);
        assert!(account.fee_hash.is_empty());
        assert!(account.deploy_hash.is_empty());
        assert!(account.salt.is_empty());
    }
}

pub(super) struct FakeEvmRpc {
    pub(super) url: String,
    worker: thread::JoinHandle<Vec<String>>,
}

impl FakeEvmRpc {
    pub(super) fn new(responses: &[(&'static str, &'static str)]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake EVM RPC");
        listener.set_nonblocking(true).expect("nonblocking fake RPC");
        let url = format!("http://{}", listener.local_addr().unwrap());
        let responses = responses.to_vec();
        let worker = thread::spawn(move || {
            let mut methods = Vec::new();
            for (expected_method, result) in responses {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "RPC request was not received");
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => panic!("fake RPC accept failed: {error}"),
                    }
                };
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                let (body_start, body_len) = loop {
                    let n = socket.read(&mut buffer).expect("read RPC request");
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                        let headers = std::str::from_utf8(&bytes[..end]).unwrap();
                        let len = headers
                            .lines()
                            .find_map(|line| {
                                let (key, value) = line.split_once(':')?;
                                key.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().unwrap())
                            })
                            .unwrap();
                        break (end + 4, len);
                    }
                };
                while bytes.len() < body_start + body_len {
                    let n = socket.read(&mut buffer).expect("read RPC body");
                    assert!(n > 0);
                    bytes.extend_from_slice(&buffer[..n]);
                }
                let request: serde_json::Value =
                    serde_json::from_slice(&bytes[body_start..body_start + body_len]).unwrap();
                let method = request["method"].as_str().unwrap();
                assert_eq!(method, expected_method);
                methods.push(method.to_string());
                let response = serde_json::json!({
                    "jsonrpc": "2.0", "id": request["id"], "result": result
                })
                .to_string();
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
            }
            methods
        });
        Self { url, worker }
    }

    pub(super) fn finish(self) -> Vec<String> {
        self.worker.join().expect("fake RPC worker")
    }
}
