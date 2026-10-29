//! Pins down a container's proxy: which addresses, user names and bypass lists are taken, what
//! the desktop's proxy is given for each choice, that the password follows the proxy it was
//! given for, and that programs are led to the desktop's proxy only while there is one.
use super::*;
use crate::model::ContainerSpec;
use serde_json::{Map, Value, json};

fn spec(proxy: Value) -> ContainerSpec {
    serde_json::from_value(json!({"name": "Proxied", "image": "sha256:a", "proxy": proxy})).unwrap()
}

fn custom(url: &str) -> Option<ProxyChoice> {
    Some(ProxyChoice::Custom {
        url: url.into(),
        username: Some("ana".into()),
        bypass: vec![],
        password: false,
    })
}

#[test]
fn an_address_is_a_scheme_a_host_and_a_port_alone() {
    for (url, scheme, host, port) in [
        (
            "http://proxy.example:3128",
            choice::Scheme::Http,
            "proxy.example",
            3128,
        ),
        (
            "HTTPS://Proxy.Example:443/",
            choice::Scheme::Https,
            "proxy.example",
            443,
        ),
        (
            "socks5://10.0.0.1:1080",
            choice::Scheme::Socks5,
            "10.0.0.1",
            1080,
        ),
        (
            "socks5://[fd00::1]:1080",
            choice::Scheme::Socks5,
            "fd00::1",
            1080,
        ),
    ] {
        let address = choice::address(url).unwrap();
        assert_eq!(
            (address.scheme, address.host.as_str(), address.port),
            (scheme, host, port),
            "{url}"
        );
    }
    for url in [
        "proxy.example:3128",
        "ftp://proxy.example:21",
        "http://proxy.example",
        "http://proxy.example:0",
        "http://proxy.example:3128/path",
        "http://proxy.example:3128?x=1",
        "http://pro xy:3128",
        "socks5://[not-v6]:1080",
    ] {
        assert!(choice::address(url).is_err(), "{url}");
    }
    // Credentials go beside the address, so the address can be shown.
    let error = choice::address("http://ana:secret@proxy.example:3128").unwrap_err();
    assert!(error.to_string().contains("proxy_password"), "{error}");
}

#[test]
fn user_names_passwords_and_bypass_lists_are_checked() {
    assert!(choice::username("ana").is_ok());
    for name in ["", "a:b", "line\nbreak", &"x".repeat(256)] {
        assert!(choice::username(name).is_err(), "{name:?}");
    }
    assert!(password("p:ss word$#").is_ok());
    assert!(password("").is_err() && password("a\rb").is_err());
    let entries = |list: &[&str]| list.iter().map(|e| e.to_string()).collect::<Vec<_>>();
    choice::bypass_list(&entries(&[
        "*",
        "intranet.example",
        ".corp.example",
        "*.dev.example",
        "10.0.0.0/8",
        "fd00::/8",
        "192.168.1.5",
    ]))
    .unwrap();
    for entry in ["10.0.0.0/33", "a b", "http://x", "", "*.", "a,b"] {
        assert!(
            choice::bypass_list(&entries(&[entry])).is_err(),
            "{entry:?}"
        );
    }
    assert!(choice::bypass_list(&vec!["a.example".to_string(); 65]).is_err());
}

#[test]
fn the_installation_proxy_is_read_from_its_settings() {
    let read = |pairs: &[(&str, &str)]| {
        let map: std::collections::HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let mut notices = Vec::new();
        let proxy = installation(&|key: &str| map.get(key).cloned(), &mut notices);
        (proxy, notices)
    };
    let (proxy, notices) = read(&[
        ("PROXY_URL", "http://proxy.example:3128"),
        ("PROXY_USERNAME", "ana"),
        ("PROXY_PASSWORD", "s3cret"),
        ("PROXY_BYPASS", " intranet.example, 10.0.0.0/8 ,"),
    ]);
    let proxy = proxy.unwrap().unwrap();
    assert_eq!(proxy.address.port, 3128);
    assert_eq!(proxy.password.as_deref(), Some("s3cret"));
    assert_eq!(proxy.bypass, ["intranet.example", "10.0.0.0/8"]);
    assert!(notices.is_empty());
    // The others without an address have no use, which startup says.
    let (proxy, notices) = read(&[("PROXY_USERNAME", "ana"), ("PROXY_URL", "")]);
    assert_eq!(proxy.unwrap(), None);
    assert_eq!(notices.len(), 1, "{notices:?}");
    for wrong in [
        &[("PROXY_URL", "proxy.example:3128")][..],
        &[("PROXY_URL", "http://p:1"), ("PROXY_PASSWORD", "alone")],
        &[("PROXY_URL", "http://p:1"), ("PROXY_BYPASS", "a b")],
    ] {
        let error = read(wrong).0.unwrap_err();
        assert!(error.to_string().contains("PROXY_"), "{error}");
    }
}

#[test]
fn programs_are_led_to_the_desktops_proxy_only_while_there_is_one() {
    let with = environment(&spec(json!({"mode": "installation"})));
    assert!(with.contains(&("HTTPS_PROXY", "http://127.0.0.1:31280".into())));
    assert!(with.contains(&("no_proxy", "localhost,127.0.0.1,::1".into())));
    // Without a proxy every variable is there and empty, so one a commit kept is cleared.
    let without = environment(&spec(Value::Null));
    assert_eq!(without.len(), 6);
    assert!(without.iter().all(|(_, value)| value.is_empty()));
}

#[test]
fn the_desktops_proxy_is_given_the_chosen_proxy_with_its_credentials() {
    let mut app = crate::test_support::app(false);
    let config = &mut std::sync::Arc::get_mut(&mut app).unwrap().config;
    assert_eq!(settings(config, &spec(Value::Null), None, Vec::new()), None);
    // Following an installation without a proxy goes straight to every address.
    let installation = spec(json!({"mode": "installation"}));
    let direct = settings(config, &installation, None, Vec::new()).unwrap();
    assert_eq!((direct.upstream, direct.bypass.len()), (None, 0));
    let mut notices = Vec::new();
    config.proxy = super::installation(
        &|key: &str| {
            Some(
                match key {
                    "PROXY_URL" => "socks5://proxy.example:1080",
                    "PROXY_USERNAME" => "corp",
                    "PROXY_PASSWORD" => "corp-secret",
                    "PROXY_BYPASS" => "intranet.example",
                    _ => return None,
                }
                .into(),
            )
        },
        &mut notices,
    )
    .unwrap();
    let followed = settings(config, &installation, Some("ignored".into()), Vec::new()).unwrap();
    let upstream = followed.upstream.clone().unwrap();
    assert_eq!(upstream.scheme, choice::Scheme::Socks5);
    assert_eq!(upstream.password.as_deref(), Some("corp-secret"));
    assert_eq!(followed.bypass, ["intranet.example"]);
    assert_ne!(followed.version, direct.version);
    // A container's own proxy takes the password kept for it.
    let own = spec(
        json!({"mode": "custom", "url": "http://own.example:8080", "username": "ana", "bypass": ["*.lan"]}),
    );
    let given = settings(config, &own, Some("own-secret".into()), Vec::new()).unwrap();
    let upstream = given.upstream.clone().unwrap();
    assert_eq!(
        (
            upstream.host.as_str(),
            upstream.port,
            upstream.username.as_deref()
        ),
        ("own.example", 8080, Some("ana"))
    );
    assert_eq!(upstream.password.as_deref(), Some("own-secret"));
    // A new password is a new version, so the desktop's proxy is given it.
    let changed = settings(config, &own, Some("other".into()), Vec::new()).unwrap();
    assert_ne!(changed.version, given.version);
    assert_eq!(
        settings(config, &own, Some("own-secret".into()), Vec::new())
            .unwrap()
            .version,
        given.version
    );
    // The members of its groups are reached straight, and joining one is a new version too.
    let grouped = settings(config, &own, Some("own-secret".into()), vec!["web".into()]).unwrap();
    assert_eq!(grouped.bypass, ["*.lan", "web"]);
    assert_ne!(grouped.version, given.version);
}

#[test]
fn a_password_belongs_to_the_proxy_it_was_given_for() {
    let request = |value: Value| {
        let mut map = Map::new();
        map.insert("proxy_password".into(), value);
        PasswordChange::requested(&map)
    };
    assert_eq!(
        PasswordChange::requested(&Map::new()).unwrap(),
        PasswordChange::Keep
    );
    assert_eq!(request(json!("")).unwrap(), PasswordChange::Clear);
    assert_eq!(request(Value::Null).unwrap(), PasswordChange::Clear);
    assert_eq!(
        request(json!("pw")).unwrap(),
        PasswordChange::Set("pw".into())
    );
    assert!(request(json!(5)).is_err() && request(json!("a\nb")).is_err());

    let flag = |proxy: &Option<ProxyChoice>| match proxy {
        Some(ProxyChoice::Custom { password, .. }) => *password,
        _ => false,
    };
    // A request cannot claim a password it did not give.
    let mut claimed = Some(ProxyChoice::Custom {
        url: "http://p.example:1".into(),
        username: None,
        bypass: vec![],
        password: true,
    });
    PasswordChange::Keep.settle(None, &mut claimed).unwrap();
    assert!(!flag(&claimed));
    let mut set = custom("http://p.example:1");
    PasswordChange::Set("pw".into())
        .settle(None, &mut set)
        .unwrap();
    assert!(flag(&set));
    // Kept while the host stays, also on another port; dropped for another host.
    let mut port = custom("http://p.example:2");
    PasswordChange::Keep
        .settle(set.as_ref(), &mut port)
        .unwrap();
    assert!(flag(&port));
    let mut host = custom("http://q.example:1");
    PasswordChange::Keep
        .settle(set.as_ref(), &mut host)
        .unwrap();
    assert!(!flag(&host));
    let mut cleared = custom("http://p.example:1");
    PasswordChange::Clear
        .settle(set.as_ref(), &mut cleared)
        .unwrap();
    assert!(!flag(&cleared));
    // Only a proxy of the container's own takes one.
    for mut other in [None, Some(ProxyChoice::Installation)] {
        assert!(
            PasswordChange::Set("pw".into())
                .settle(None, &mut other)
                .is_err()
        );
        PasswordChange::Clear
            .settle(set.as_ref(), &mut other)
            .unwrap();
    }
}

#[test]
fn the_password_is_kept_apart_from_the_container_and_follows_its_settings() {
    let app = crate::test_support::app(false);
    let mut own = custom("http://p.example:1");
    PasswordChange::Set("pw".into())
        .settle(None, &mut own)
        .unwrap();
    app.save_proxy_password("c1", own.as_ref(), &PasswordChange::Set("pw".into()))
        .unwrap();
    assert_eq!(app.proxy_password("c1").as_deref(), Some("pw"));
    // A change that keeps it leaves it, one that drops it removes it.
    app.save_proxy_password("c1", own.as_ref(), &PasswordChange::Keep)
        .unwrap();
    assert_eq!(app.proxy_password("c1").as_deref(), Some("pw"));
    app.save_proxy_password(
        "c1",
        Some(&ProxyChoice::Installation),
        &PasswordChange::Keep,
    )
    .unwrap();
    assert_eq!(app.proxy_password("c1"), None);
    // The container record says only whether there is one.
    let record = serde_json::to_string(&spec(serde_json::to_value(own).unwrap())).unwrap();
    assert!(
        record.contains("\"password\":true") && !record.contains("pw\""),
        "{record}"
    );
}
