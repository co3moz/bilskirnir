//! Pins down the subnet a desktop's VNC server is told to listen in: the IPv4 one of its private
//! network, or none.
use super::*;

#[test]
fn vnc_listens_in_the_ipv4_subnet_of_the_private_network() {
    let network = |configs: serde_json::Value| serde_json::json!({"IPAM": {"Config": configs}});
    let dual = network(serde_json::json!([
        {"Subnet": "fd00:7468::/64", "Gateway": "fd00:7468::1"},
        {"Subnet": "172.30.0.0/16"},
    ]));
    assert_eq!(private_subnet(&dual), Some("172.30.0.0/16"));
    // Without an IPv4 subnet the desktop is told nothing and listens on every interface.
    let ipv6 = network(serde_json::json!([{"Subnet": "fd00:7468::/64"}]));
    assert_eq!(private_subnet(&ipv6), None);
    assert_eq!(private_subnet(&network(serde_json::Value::Null)), None);
}
