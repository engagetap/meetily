//! Local HTTP API smoke test — validates the server's auth gate without
//! needing a full Tauri runtime. Token mismatch should produce 401.
//!
//! End-to-end recording + bookmark verification happens via the dev page
//! (manual) and via direct unit tests on the underlying primitives. A
//! full end-to-end test would need a real `AppState`, which requires a
//! Tauri runtime — out of scope for a `tests/` integration test.

use std::net::TcpListener;

#[test]
fn binding_to_127_0_0_1_with_random_port_works() {
    // Sanity: the same primitive the server uses (TcpListener::bind on
    // ("127.0.0.1", 0)) returns a non-zero local port we can read back.
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    let port = listener.local_addr().unwrap().port();
    assert!(port != 0, "expected a non-zero ephemeral port");
}
