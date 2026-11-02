// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian
use serde_json::Value;
use symvoulio_core::{AuthorResponse, Review};

fn strict(value: &Value) {
    if let Some(object) = value.as_object() {
        if object.get("type").and_then(Value::as_str) == Some("object") {
            assert_eq!(
                object.get("additionalProperties"),
                Some(&Value::Bool(false))
            );
            let properties = object["properties"].as_object().unwrap();
            let required = object["required"].as_array().unwrap();
            assert_eq!(properties.len(), required.len());
            for name in properties.keys() {
                assert!(required.contains(&Value::String(name.clone())));
            }
        }
        for child in object.values() {
            strict(child);
        }
    } else if let Some(array) = value.as_array() {
        for child in array {
            strict(child);
        }
    }
}
#[test]
fn all_public_schemas_are_strict() {
    for text in [
        include_str!("../../../schemas/review.schema.json"),
        include_str!("../../../schemas/author-response.schema.json"),
        include_str!("../../../schemas/run-manifest.schema.json"),
    ] {
        strict(&serde_json::from_str::<Value>(text).unwrap());
    }
}
#[test]
fn review_fixture_validates_and_round_trips() {
    let value: Value =
        serde_json::from_str(include_str!("../../../samples/fixtures/review_01.json")).unwrap();
    let schema = serde_json::from_str(include_str!("../../../schemas/review.schema.json")).unwrap();
    jsonschema::validator_for(&schema)
        .unwrap()
        .validate(&value)
        .unwrap();
    let review: Review = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(review).unwrap(), value);
}
#[test]
fn author_fixture_validates_and_round_trips() {
    let value: Value = serde_json::from_str(include_str!(
        "../../../samples/fixtures/author_response_01.json"
    ))
    .unwrap();
    let schema =
        serde_json::from_str(include_str!("../../../schemas/author-response.schema.json")).unwrap();
    jsonschema::validator_for(&schema)
        .unwrap()
        .validate(&value)
        .unwrap();
    let author: AuthorResponse = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(author).unwrap(), value);
}
#[test]
fn provider_contracts_reject_unknown_fields() {
    let mut value: Value =
        serde_json::from_str(include_str!("../../../samples/fixtures/review_01.json")).unwrap();
    value["issues"][0]["extra"] = Value::Bool(true);
    assert!(serde_json::from_value::<Review>(value).is_err());
    let mut value: Value = serde_json::from_str(include_str!(
        "../../../samples/fixtures/author_response_01.json"
    ))
    .unwrap();
    value["responses"][0]["extra"] = Value::Bool(true);
    assert!(serde_json::from_value::<AuthorResponse>(value).is_err());
}
