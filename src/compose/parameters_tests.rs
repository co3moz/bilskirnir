//! Pins down what each parameter type accepts, which displays go with it, and the text the
//! setup script receives: a value the create form or an agent sends is refused here, before a
//! script ever sees it, when it is not one its parameter would offer.
use super::*;
use serde_json::json;

fn parameter(value: serde_json::Value) -> Parameter {
    let mut value = value;
    value["name"] = json!("VALUE");
    serde_json::from_value(value).unwrap()
}

fn flavors() -> serde_json::Value {
    json!([{"value": "vanilla", "label": "Vanilla"}, {"value": "chocolate"}])
}

#[test]
fn every_type_has_its_own_displays_and_fields() {
    for valid in [
        json!({}),
        json!({"display": "multiline"}),
        json!({"type": "number", "display": "slider", "min": 0, "max": 10}),
        json!({"type": "boolean", "display": "checkbox"}),
        json!({"type": "choice", "display": "radio", "options": flavors()}),
        json!({"type": "choice", "options": flavors(), "allow_custom": true}),
        json!({"type": "choices", "display": "checkboxes", "options": flavors()}),
        json!({"type": "volume", "path": "/opt/unity"}),
        json!({"type": "volume", "display": "select", "path": "/home/desktop/Unity", "default": "unity-installs"}),
    ] {
        parameter(valid.clone())
            .validate()
            .unwrap_or_else(|e| panic!("{valid}: {e:#}"));
    }
    for (invalid, reason) in [
        (json!({"type": "volume"}), "The path of VALUE"),
        (
            json!({"path": "/opt/unity"}),
            "Only volume parameters have a path",
        ),
        (
            json!({"type": "volume", "path": "/opt/unity", "display": "radio"}),
            "cannot be shown as radio",
        ),
        (
            json!({"type": "volume", "path": "/opt/unity", "secret": true}),
            "Only text parameters can be secret",
        ),
        (
            json!({"type": "volume", "path": "/opt/unity", "default": "-unity"}),
            "The default of VALUE",
        ),
        (json!({"display": "slider"}), "cannot be shown as slider"),
        (
            json!({"type": "boolean", "display": "radio"}),
            "boolean parameter",
        ),
        (
            json!({"type": "choices", "display": "select", "options": flavors()}),
            "shown as",
        ),
        (
            json!({"type": "number", "secret": true}),
            "Only text parameters can be secret",
        ),
        (
            json!({"options": flavors()}),
            "Only choice parameters have options",
        ),
        (
            json!({"type": "choices", "options": flavors(), "allow_custom": true}),
            "single choice",
        ),
        (
            json!({"type": "boolean", "min": 1}),
            "Only number parameters",
        ),
        (
            json!({"type": "number", "display": "slider", "max": 10}),
            "needs a min and a max",
        ),
        (
            json!({"type": "number", "min": 5, "max": 1}),
            "above its max",
        ),
        (json!({"type": "number", "step": 0}), "must be positive"),
    ] {
        let error = parameter(invalid.clone())
            .validate()
            .unwrap_err()
            .to_string();
        assert!(error.contains(reason), "{invalid}: {error}");
    }
    // A bound JSON cannot spell still has to be a number a form can show.
    let infinite = Parameter {
        kind: Kind::Number,
        max: Some(f64::INFINITY),
        ..parameter(json!({}))
    };
    assert!(infinite.validate().is_err());
    // A misspelled type is an error, not a text parameter.
    assert!(serde_json::from_value::<Parameter>(json!({"name": "A", "type": "select"})).is_err());
}

#[test]
fn a_volume_path_is_where_a_volume_setting_could_be_mounted() {
    let at = |path: &str| parameter(json!({"type": "volume", "path": path})).validate();
    for valid in [
        "/opt/unity",
        "/unity-install-location",
        "/home/desktop/Unity",
        "/srv/cache/unity_v1",
    ] {
        at(valid).unwrap_or_else(|e| panic!("{valid}: {e:#}"));
    }
    // Relative or unnormalized paths, the system folders and what the manager mounts itself:
    // its tmpfs folders, the GPU libraries under /usr/lib/wsl and shared memory under /dev.
    for invalid in [
        "",
        "opt/unity",
        "/",
        "/opt/unity/",
        "/opt//unity",
        "/opt/./unity",
        "/opt/../etc",
        "/opt",
        "/home",
        "/tmp/unity",
        "/run/desktop",
        "/usr/lib/wsl",
        "/dev/shm",
        "/etc/unity",
        "/opt/VirtualGL",
        "/opt/a,b",
    ] {
        assert!(at(invalid).is_err(), "{invalid:?} was accepted");
    }
}

#[test]
fn a_folder_is_a_canonical_desktop_folder_outside_the_system_ones() {
    let project = Parameter {
        name: "UNITY_PROJECT".into(),
        label: "Unity project".into(),
        ..parameter(json!({"type": "folder", "default": "/home/desktop/UnityProjects/MyGame"}))
    };
    project.validate().unwrap();
    for valid in [
        "/home/desktop/UnityProjects/My Game",
        "/home/desktop/.local/share/godot",
        "/srv/projects",
        "/opt/unity/projects",
        "/workspace,2026",
    ] {
        project
            .check(valid)
            .unwrap_or_else(|e| panic!("{valid}: {e:#}"));
    }
    for invalid in [
        "relative/folder",
        "/",
        "/home",
        "/home/desktop/",
        "/home/desktop//Projects",
        "/home/desktop/../root",
        "/etc/unity",
        "/tmp/build",
        "/usr/local/projects",
        "/opt",
        "/opt/VirtualGL/x",
        "/home/desktop/two\nlines",
        "C:\\Projects",
    ] {
        let error = project.check(invalid).unwrap_err().to_string();
        assert!(
            error.contains("Unity project must be an absolute folder"),
            "{invalid:?}: {error}"
        );
    }
    // The script gets the folder as it is, and only a required folder has to be given.
    assert_eq!(
        project.environment("/srv/projects"),
        [("UNITY_PROJECT".to_string(), "/srv/projects".to_string())]
    );
    let optional = Parameter {
        default: String::new(),
        ..project.clone()
    };
    assert_eq!(optional.resolve(None).unwrap(), "");
    let required = Parameter {
        required: true,
        ..optional
    };
    assert!(
        required
            .resolve(None)
            .unwrap_err()
            .to_string()
            .contains("Enter a value")
    );
    for (invalid, reason) in [
        (
            json!({"type": "folder", "secret": true}),
            "Only text parameters can be secret",
        ),
        (
            json!({"type": "folder", "display": "multiline"}),
            "cannot be shown as multiline",
        ),
        (
            json!({"type": "folder", "pattern": "/srv/.*"}),
            "Only text parameters have a pattern",
        ),
        (
            json!({"type": "folder", "path": "/srv"}),
            "Only volume parameters have a path",
        ),
        (
            json!({"type": "folder", "default": "/etc"}),
            "The default of VALUE",
        ),
    ] {
        let error = parameter(invalid.clone())
            .validate()
            .unwrap_err()
            .to_string();
        assert!(error.contains(reason), "{invalid}: {error}");
    }
}

#[test]
fn a_volume_parameter_takes_a_volume_and_gives_the_script_its_path() {
    let unity = Parameter {
        name: "UNITY_HOME".into(),
        label: "Unity installs".into(),
        ..parameter(json!({"type": "volume", "path": "/opt/unity"}))
    };
    for valid in ["unity-installs", "ab", "Unity_2022.3"] {
        unity
            .check(valid)
            .unwrap_or_else(|e| panic!("{valid}: {e:#}"));
    }
    for invalid in ["a", "-unity", ".unity", "with space", "a/b", "two\nlines"] {
        assert!(unity.check(invalid).is_err(), "{invalid:?}");
    }
    // No volume is a choice of its own, unless one is required.
    assert_eq!(unity.resolve(None).unwrap(), "");
    let required = Parameter {
        required: true,
        ..unity.clone()
    };
    assert_eq!(
        required.resolve(None).unwrap_err().to_string(),
        "Choose a volume for Unity installs"
    );
    // The script reads the path either way, and the volume beside it.
    let variables = |value: &str| unity.environment(value);
    assert_eq!(
        variables("unity-installs"),
        [
            ("UNITY_HOME".into(), "/opt/unity".into()),
            ("UNITY_HOME_VOLUME".into(), "unity-installs".into()),
        ]
    );
    assert_eq!(
        variables("")[1],
        ("UNITY_HOME_VOLUME".into(), String::new())
    );
    assert_eq!(
        unity.mount("unity-installs"),
        Some(crate::model::Mount {
            volume: "unity-installs".into(),
            target: "/opt/unity".into(),
            readonly: false,
        })
    );
    assert_eq!(unity.mount(""), None);
    // Other types pass their value through and mount nothing.
    let text = parameter(json!({}));
    assert_eq!(text.environment("x"), [("VALUE".into(), "x".into())]);
    assert_eq!(text.mount("unity-installs"), None);
    assert_eq!(text.volume_variable(), None);
}

#[test]
fn options_are_short_unique_lines() {
    let with = |options: serde_json::Value| {
        parameter(json!({"type": "choice", "options": options})).validate()
    };
    assert!(with(json!([])).is_err());
    assert!(with(json!([{"value": ""}])).is_err());
    assert!(with(json!([{"value": "a\nb"}])).is_err());
    assert!(with(json!([{"value": "a"}, {"value": "a"}])).is_err());
    assert!(with(json!([{"value": "a", "label": "two\nlines"}])).is_err());
    assert!(
        with(json!(
            (0..101)
                .map(|i| json!({"value": i.to_string()}))
                .collect::<Vec<_>>()
        ))
        .is_err()
    );
    with(json!(
        (0..100)
            .map(|i| json!({"value": i.to_string()}))
            .collect::<Vec<_>>()
    ))
    .unwrap();
}

#[test]
fn a_default_must_be_a_value_the_parameter_accepts() {
    for invalid in [
        json!({"type": "number", "default": "ten"}),
        json!({"type": "number", "min": 1, "default": "0"}),
        json!({"type": "boolean", "default": "yes"}),
        json!({"type": "choice", "options": flavors(), "default": "strawberry"}),
        json!({"type": "choices", "options": flavors(), "default": "vanilla\nstrawberry"}),
        json!({"default": "two\nlines"}),
        json!({"secret": true, "default": "hunter2"}),
    ] {
        assert!(parameter(invalid.clone()).validate().is_err(), "{invalid}");
    }
    for valid in [
        json!({"type": "number", "min": 1, "default": "1"}),
        json!({"type": "boolean", "default": "true"}),
        json!({"type": "choice", "options": flavors(), "allow_custom": true, "default": "mint"}),
        json!({"type": "choices", "options": flavors(), "default": "chocolate\nvanilla"}),
        json!({"display": "multiline", "default": "two\nlines"}),
    ] {
        parameter(valid.clone())
            .validate()
            .unwrap_or_else(|e| panic!("{valid}: {e:#}"));
    }
}

#[test]
fn numbers_are_plain_decimals_within_their_range_and_steps() {
    let any = parameter(json!({"type": "number"}));
    for valid in ["42", "-1.5", "0.25", "007", "0"] {
        any.check(valid)
            .unwrap_or_else(|e| panic!("{valid}: {e:#}"));
    }
    // Shell arithmetic and most tools read neither exponents nor signs, spaces or bare points.
    for invalid in [
        "1e3", "+1", " 4", "4 ", "4.", ".5", "-", "NaN", "inf", "0x10", "1,5",
    ] {
        assert!(any.check(invalid).is_err(), "{invalid}");
    }
    let halves = parameter(json!({"type": "number", "min": 0, "max": 2, "step": 0.5}));
    assert!(halves.check("1.5").is_ok() && halves.check("2").is_ok());
    assert!(halves.check("1.25").is_err() && halves.check("2.5").is_err());
    assert!(halves.check("-0.5").is_err());
    // Steps count from the minimum, and a decimal step survives floating point.
    let odd = parameter(json!({"type": "number", "min": 1, "step": 2}));
    assert!(odd.check("3").is_ok() && odd.check("4").is_err());
    let tenths = parameter(json!({"type": "number", "step": 0.1}));
    assert!(tenths.check("0.3").is_ok() && tenths.check("0.35").is_err());
}

#[test]
fn a_value_left_out_becomes_what_the_script_would_expect() {
    let toggle = parameter(json!({"type": "boolean"}));
    assert_eq!(toggle.resolve(None).unwrap(), "false");
    assert_eq!(toggle.resolve(Some(&"true".into())).unwrap(), "true");
    assert!(toggle.resolve(Some(&"yes".into())).is_err());
    let on = parameter(json!({"type": "boolean", "default": "true"}));
    assert_eq!(on.resolve(None).unwrap(), "true");
    // A required boolean is an agreement: it has to be turned on.
    let agree = parameter(json!({"type": "boolean", "required": true, "label": "Accept"}));
    assert_eq!(
        agree
            .resolve(Some(&"false".into()))
            .unwrap_err()
            .to_string(),
        "Turn on Accept"
    );
    let pick = parameter(json!({"type": "choice", "options": flavors(), "required": true}));
    assert!(
        pick.resolve(None)
            .unwrap_err()
            .to_string()
            .starts_with("Choose a value")
    );
    let count = parameter(json!({"type": "number", "required": true}));
    assert!(
        count
            .resolve(Some(&String::new()))
            .unwrap_err()
            .to_string()
            .starts_with("Enter")
    );
    // An optional value can stay empty, whatever its type.
    let optional = parameter(json!({"type": "choice", "options": flavors()}));
    assert_eq!(optional.resolve(None).unwrap(), "");
}

#[test]
fn choices_take_their_options_and_other_text_only_when_allowed() {
    let strict = parameter(json!({"type": "choice", "options": flavors()}));
    assert!(strict.check("vanilla").is_ok());
    assert!(strict.check("mint").is_err() && strict.check("Vanilla").is_err());
    let open = parameter(json!({"type": "choice", "options": flavors(), "allow_custom": true}));
    assert!(open.check("mint").is_ok());
    assert!(open.check("mint\nchip").is_err());
    let many = parameter(json!({"type": "choices", "options": flavors()}));
    assert!(many.check("vanilla").is_ok() && many.check("chocolate\nvanilla").is_ok());
    for invalid in [
        "vanilla\nvanilla",
        "vanilla\n",
        "mint",
        "vanilla,chocolate",
        "vanilla\r\nchocolate",
    ] {
        assert!(many.check(invalid).is_err(), "{invalid:?}");
    }
    let required = parameter(json!({"type": "choices", "options": flavors(), "required": true}));
    assert!(required.resolve(Some(&String::new())).is_err());
    // Text is one line unless the form shows a text area for it.
    assert!(parameter(json!({})).check("a\nb").is_err());
    assert!(
        parameter(json!({"display": "multiline"}))
            .check("a\nb")
            .is_ok()
    );
}

#[test]
fn a_parameter_is_written_back_as_it_was_given() {
    let full = json!({
        "name": "UNITY_EDITOR", "label": "Editor", "description": "Pick the version the project uses.
    Other versions must be installed first.",
        "type": "choice", "display": "radio",
        "required": true, "secret": false, "default": "6000.0.23f1",
        "options": [{"value": "6000.0.23f1", "label": "Unity 6"}, {"value": "2022.3.50f1", "label": ""}],
        "allow_custom": true
    });
    let parsed: Parameter = serde_json::from_value(full.clone()).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), full);
    // A bare name is a text parameter; the fields of other types are left out.
    let text: Parameter = serde_json::from_value(json!({"name": "A"})).unwrap();
    assert_eq!(
        serde_json::to_value(&text).unwrap(),
        json!({"name": "A", "label": "", "description": "", "type": "text", "required": false, "secret": false, "default": ""})
    );
    // A volume parameter keeps its path, which configurations and archives carry along.
    let volume = json!({
        "name": "UNITY_HOME", "label": "Unity installs", "description": "", "type": "volume",
        "required": false, "secret": false, "default": "unity-installs", "path": "/opt/unity"
    });
    let parsed: Parameter = serde_json::from_value(volume.clone()).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), volume);
    // So does a text parameter its pattern and the message that goes with it.
    let address = json!({
        "name": "ACCELERATOR", "label": "Accelerator", "description": "", "type": "text",
        "required": false, "secret": false, "default": "",
        "pattern": "[^:]+:[0-9]+", "pattern_message": "host:port"
    });
    let parsed: Parameter = serde_json::from_value(address.clone()).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), address);
}

#[test]
fn the_interface_offers_the_displays_this_module_accepts() {
    // The editor offers what this list allows, so a display added or dropped here has to reach
    // web/src/lib/compose.ts in the same change.
    let interface = include_str!("../../web/src/lib/compose.ts");
    for kind in kinds() {
        let displays = kind
            .displays()
            .iter()
            .map(|display| format!("\"{}\"", wire(display)));
        let line = format!(
            "{}: [{}],",
            wire(kind),
            displays.collect::<Vec<_>>().join(", ")
        );
        assert!(
            interface.contains(&line),
            "web/src/lib/compose.ts must list {line}"
        );
    }
}

/// Every type. The match stops compiling when a type is added, so the list is looked at then.
fn kinds() -> [Kind; 7] {
    let kinds = [
        Kind::Text,
        Kind::Number,
        Kind::Boolean,
        Kind::Choice,
        Kind::Choices,
        Kind::Volume,
        Kind::Folder,
    ];
    for kind in kinds {
        match kind {
            Kind::Text | Kind::Number | Kind::Boolean | Kind::Choice | Kind::Choices => {}
            Kind::Volume | Kind::Folder => {}
        }
    }
    kinds
}

#[test]
fn agents_and_the_api_guide_are_offered_every_type_and_display() {
    // The MCP schema lists the types and displays by hand, and the guide has a row per type.
    let mcp = include_str!("../mcp.rs");
    let types = kinds().map(|kind| format!("\"{}\"", wire(kind))).join(",");
    assert!(
        mcp.contains(&format!("\"enum\":[{types}]")),
        "the compose schema in src/mcp.rs must offer the types [{types}]"
    );
    let mut displays = Vec::new();
    for display in kinds().iter().flat_map(|kind| kind.displays()) {
        let display = format!("\"{}\"", wire(display));
        if !displays.contains(&display) {
            displays.push(display);
        }
    }
    let displays = displays.join(",");
    assert!(
        mcp.contains(&format!("\"enum\":[{displays}]")),
        "the compose schema in src/mcp.rs must offer the displays [{displays}]"
    );
    let guide = include_str!("../../docs/API.md");
    for kind in kinds() {
        let row = format!("\n| `{}`", wire(kind));
        assert!(guide.contains(&row), "docs/API.md has no row for {row:?}");
    }
}

#[test]
fn a_pattern_has_to_match_the_whole_value() {
    let address = parameter(json!({
        "label": "Accelerator", "pattern": r"[0-9]{1,3}(\.[0-9]{1,3}){3}:[0-9]{1,5}",
        "pattern_message": "host:port, for example 192.168.1.20:10080"
    }));
    address.validate().unwrap();
    address.check("192.168.1.20:10080").unwrap();
    // A value that only contains a match is not one, whatever the pattern says about its ends.
    for invalid in [
        "192.168.1.20",
        "x192.168.1.20:10080",
        "192.168.1.20:10080 ",
        "1922.168.1.20:10080",
    ] {
        assert_eq!(
            address.check(invalid).unwrap_err().to_string(),
            "Accelerator does not have the expected format: host:port, for example 192.168.1.20:10080",
            "{invalid:?}"
        );
    }
    // The whole value, not the first alternative that matches at its start.
    let alternatives = parameter(json!({"pattern": "a|ab"}));
    assert!(alternatives.check("ab").is_ok() && alternatives.check("abc").is_err());
    // Flags in the pattern stay inside it: (?m) does not let ^ and $ match at each line.
    let lines = parameter(json!({"display": "multiline", "pattern": "(?m)^[a-z]+$"}));
    assert!(lines.check("abc").is_ok() && lines.check("abc\ndef").is_err());
    // A multiline value is matched as a whole too, so a pattern spells out the line breaks.
    let each_line = parameter(json!({"display": "multiline", "pattern": r"[a-z]+(\n[a-z]+)*"}));
    assert!(each_line.check("abc\ndef").is_ok() && each_line.check("abc\n").is_err());
    // Without a message the error still names the field.
    assert_eq!(
        parameter(json!({"pattern": "[0-9]+"}))
            .check("12a")
            .unwrap_err()
            .to_string(),
        "VALUE does not have the expected format"
    );
}

#[test]
fn an_empty_value_is_left_to_required_and_a_default_must_match() {
    let port = parameter(json!({"label": "Port", "pattern": "[0-9]+"}));
    assert_eq!(port.resolve(None).unwrap(), "");
    assert_eq!(port.resolve(Some(&String::new())).unwrap(), "");
    let required = parameter(json!({"label": "Port", "pattern": "[0-9]+", "required": true}));
    assert_eq!(
        required.resolve(None).unwrap_err().to_string(),
        "Enter a value for Port"
    );
    assert!(required.resolve(Some(&"80a".into())).is_err());
    assert_eq!(required.resolve(Some(&"80".into())).unwrap(), "80");
    parameter(json!({"pattern": "[0-9]+", "default": "8080"}))
        .validate()
        .unwrap();
    let error = parameter(json!({"pattern": "[0-9]+", "default": "http"}))
        .validate()
        .unwrap_err();
    assert_eq!(
        format!("{error:#}"),
        "The default of VALUE is not a value it accepts: VALUE does not have the expected format"
    );
}

#[test]
fn a_secret_value_stays_out_of_the_pattern_error() {
    let token = parameter(json!({
        "label": "Token", "secret": true, "required": true,
        "pattern": "ghp_[A-Za-z0-9]{36}", "pattern_message": "A GitHub token starting with ghp_"
    }));
    token.validate().unwrap();
    let error = token
        .resolve(Some(&"hunter2-not-a-token".into()))
        .unwrap_err();
    let error = format!("{error:#} {error:?}");
    assert!(!error.contains("hunter2"), "{error}");
    assert!(
        error.contains("A GitHub token starting with ghp_"),
        "{error}"
    );
    let valid = format!("ghp_{}", "a".repeat(36));
    assert_eq!(token.resolve(Some(&valid)).unwrap(), valid);
}

#[test]
fn only_text_has_a_pattern_and_its_message_needs_one() {
    for (invalid, reason) in [
        (
            json!({"type": "number", "pattern": "[0-9]+"}),
            "Only text parameters have a pattern",
        ),
        (
            json!({"type": "choice", "options": flavors(), "allow_custom": true, "pattern": "[a-z]+"}),
            "Only text parameters have a pattern",
        ),
        (
            json!({"type": "volume", "path": "/opt/unity", "pattern_message": "x"}),
            "Only text parameters have a pattern",
        ),
        (
            json!({"pattern_message": "host:port"}),
            "has a pattern message but no pattern",
        ),
        (
            json!({"pattern": "[0-9]+", "pattern_message": "x".repeat(201)}),
            "A pattern message is one line",
        ),
        (
            json!({"pattern": "[0-9]+", "pattern_message": "two\nlines"}),
            "A pattern message is one line",
        ),
    ] {
        let error = parameter(invalid.clone())
            .validate()
            .unwrap_err()
            .to_string();
        assert!(error.contains(reason), "{invalid}: {error}");
    }
    // Characters, not bytes, as for the other text a compose shows.
    parameter(json!({"pattern": ".+", "pattern_message": "ğ".repeat(200)}))
        .validate()
        .unwrap();
}

#[test]
fn a_pattern_the_manager_cannot_compile_is_refused_when_the_compose_is_saved() {
    let refused = |pattern: &str| {
        parameter(json!({"pattern": pattern}))
            .validate()
            .unwrap_err()
            .to_string()
    };
    for (pattern, reason) in [
        ("[0-9", "unclosed character class"),
        (
            "(?=x)y",
            "look-around, including look-ahead and look-behind, is not supported",
        ),
        (r"(a)\1", "backreferences are not supported"),
        // Balanced only once it is wrapped, which would let it match more than the value.
        ("a)|(b", "unopened group"),
    ] {
        assert_eq!(
            refused(pattern),
            format!("The pattern of VALUE is not a regular expression the manager reads: {reason}"),
            "{pattern}"
        );
    }
    // Short patterns that compile to megabytes, and long or multiline ones.
    for pattern in [r"\w{1,100}", "a{1000}{1000}"] {
        assert!(
            refused(pattern).contains("takes too much memory"),
            "{pattern}"
        );
    }
    parameter(json!({"pattern": r"\w{1,64}"}))
        .validate()
        .unwrap();
    parameter(json!({"pattern": "x".repeat(500)}))
        .validate()
        .unwrap();
    assert!(refused(&"x".repeat(501)).contains("at most 500 characters"));
    assert!(refused("a\nb").contains("one line"));
}

#[test]
fn every_field_reaches_agents_the_dashboard_and_the_guide() {
    // Every field spelled out, so a new one stops this compiling until it is listed here.
    let full = Parameter {
        name: "A".into(),
        label: "x".into(),
        description: "x".into(),
        kind: Kind::Text,
        display: Some(Display::Input),
        required: true,
        secret: true,
        default: "x".into(),
        options: vec![Choice {
            value: "x".into(),
            label: String::new(),
        }],
        allow_custom: true,
        min: Some(0.0),
        max: Some(1.0),
        step: Some(1.0),
        path: "/opt/x".into(),
        pattern: "x".into(),
        pattern_message: "x".into(),
    };
    // The MCP schema refuses fields it does not list, a pasted configuration keeps only those
    // the dashboard reads, and the guide explains each one.
    let mcp = include_str!("../mcp.rs");
    let reader = include_str!("../../web/src/lib/compose.ts");
    let guide = include_str!("../../docs/API.md");
    for field in serde_json::to_value(&full)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
    {
        assert!(
            mcp.contains(&format!("\"{field}\":{{")),
            "the compose schema in src/mcp.rs must list {field}"
        );
        assert!(
            reader.contains(&format!("\"{field}\""))
                || reader.contains(&format!("parameter.{field}")),
            "readParameter in web/src/lib/compose.ts must read {field}"
        );
        assert!(
            guide.contains(&format!("`{field}`")),
            "docs/API.md must explain {field}"
        );
    }
}

#[test]
fn a_description_is_help_of_a_few_sentences() {
    let with = |description: String| parameter(json!({"description": description})).validate();
    with("Find it under Unity ID, Account.\nIt is not your login name.".into()).unwrap();
    with("x".repeat(500)).unwrap();
    // Characters, not bytes: help text in another script is not cut shorter.
    with("ğ".repeat(500)).unwrap();
    assert!(with("x".repeat(501)).is_err());
    assert!(with("nul\0".into()).is_err());
}
