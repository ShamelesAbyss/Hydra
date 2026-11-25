use hydra::signer_nonce::NonceCoordinator;
const SIGNER: &str = "0x1111111111111111111111111111111111111111";
#[test]
fn process_nonce_child() {
    let Some(root) = std::env::var_os("HYDRA_NONCE_TEST_CHILD_ROOT") else {
        return;
    };
    let db =
        hydra::storage::Storage::open(std::env::var_os("HYDRA_NONCE_TEST_ACCOUNT_DB").unwrap())
            .unwrap();
    db.migrate().unwrap();
    db.bind_trading_account(&std::env::var("HYDRA_NONCE_TEST_ACCOUNT").unwrap(), false)
        .unwrap();
    let coordinator = NonceCoordinator::at_root(std::path::Path::new(&root));
    let values: Vec<_> = (0..10)
        .map(|_| coordinator.reserve(SIGNER).unwrap())
        .collect();
    println!("NONCE_VALUES={}", serde_json::to_string(&values).unwrap());
}
#[test]
fn processes_share_signer_history_independently_of_account_databases() {
    let d = tempfile::tempdir().unwrap();
    let children: Vec<_> = (0..4)
        .map(|n| {
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "process_nonce_child", "--nocapture"])
                .env("HYDRA_NONCE_TEST_CHILD_ROOT", d.path().join("coordinator"))
                .env(
                    "HYDRA_NONCE_TEST_ACCOUNT_DB",
                    d.path().join(format!("account-{n}.sqlite")),
                )
                .env("HYDRA_NONCE_TEST_ACCOUNT", format!("0x{:040x}", n + 2))
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    let mut all = Vec::<u64>::new();
    for child in children {
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8(out.stdout).unwrap();
        let line = text
            .lines()
            .find_map(|s| s.strip_prefix("NONCE_VALUES="))
            .unwrap();
        let values: Vec<u64> = serde_json::from_str(line).unwrap();
        assert!(values.windows(2).all(|p| p[1] > p[0]));
        all.extend(values);
    }
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), 40);
    let next = NonceCoordinator::at_root(&d.path().join("coordinator"))
        .reserve(SIGNER)
        .unwrap();
    assert!(next > *all.last().unwrap());
}
