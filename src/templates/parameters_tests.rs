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
    ] {
        parameter(valid.clone())
            .validate()
            .unwrap_or_else(|e| panic!("{valid}: {e:#}"));
    }
    for (invalid, reason) in [
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
}

#[test]
fn the_interface_offers_the_displays_this_module_accepts() {
    // The editor offers what this list allows, so a display added or dropped here has to reach
    // web/src/lib/templates.ts in the same change.
    let interface = include_str!("../../web/src/lib/templates.ts");
    for kind in [
        Kind::Text,
        Kind::Number,
        Kind::Boolean,
        Kind::Choice,
        Kind::Choices,
    ] {
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
            "web/src/lib/templates.ts must list {line}"
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
