//! Round 5 review fixes: the model base URL is checked again at call time.

use eunomia_backend::llm_net;
use std::net::IpAddr;

/// Env switches are process-global: tests that set one take this lock.
static ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn host(name: &str, ip: &str) {
    llm_net::TEST_HOSTS.lock().unwrap().push((name.into(), vec![ip.parse::<IpAddr>().unwrap()]));
}

/// The error text of a GET through the guarded client (the connection itself would be refused).
async fn get_err(url: &str) -> String {
    let c = llm_net::client(url).await.map_err(|e| e.message);
    match c {
        Err(m) => m,
        Ok(c) => format!("{:?}", c.get(url).timeout(std::time::Duration::from_secs(3)).send().await.unwrap_err()),
    }
}

#[tokio::test]
async fn a_name_that_resolves_somewhere_forbidden_is_refused_at_call_time() {
    let _env = ENV.lock().await;
    host("rebind.test", "127.0.0.1");
    host("meta.test", "169.254.169.254");

    // private allowed (default): loopback passes the guard (and fails only at connect), metadata never does
    unsafe { std::env::remove_var("ALLOW_PRIVATE_LLM_URL") };
    assert!(!get_err("http://rebind.test:9/v1").await.contains("private network"));
    assert!(get_err("http://meta.test:9/v1").await.contains("link-local"));
    assert!(get_err("http://169.254.169.254/v1").await.contains("link-local"));

    // private forbidden: the same name, accepted when saved, is now refused when called
    unsafe { std::env::set_var("ALLOW_PRIVATE_LLM_URL", "0") };
    let e = get_err("http://rebind.test:9/v1").await;
    unsafe { std::env::remove_var("ALLOW_PRIVATE_LLM_URL") };
    assert!(e.contains("private network"), "{e}");
}
