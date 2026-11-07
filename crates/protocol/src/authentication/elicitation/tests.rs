use super::*;
use serde_json::json;

fn request() -> Request {
    parse_request(&json!({"mode":"form","requestId":1,"message":"Configure","requestedSchema":{
        "type":"object","properties":{
            "name":{"type":"string","minLength":2,"pattern":"^[a-z]+$"},
            "count":{"type":"integer","minimum":1,"maximum":10},
            "enabled":{"type":"boolean"},
            "tools":{"type":"array","items":{"type":"string","enum":["rust","shell"]},"minItems":1,"maxItems":2}
        },"required":["name","count","enabled","tools"]
    }})).expect("supported schema")
}

#[test]
fn form_constraints_distinguish_false_fractional_numbers_and_duplicate_choices() {
    let request = request();
    let content = json!({"name":"atlas","count":2,"enabled":false,"tools":["rust"]});
    validate_response(&request, &json!({"action":"accept","content":content}))
        .expect("false is a provided value");
    let mut fractional = content.clone();
    fractional["count"] = json!(2.5);
    assert!(validate_response(&request, &json!({"action":"accept","content":fractional})).is_err());
    let mut duplicates = content.clone();
    duplicates["tools"] = json!(["rust", "rust"]);
    assert!(validate_response(&request, &json!({"action":"accept","content":duplicates})).is_err());
    let mut pattern = content;
    pattern["name"] = json!("A!");
    assert!(validate_response(&request, &json!({"action":"accept","content":pattern})).is_err());
    validate_response(&request, &json!({"action":"cancel","content":"unused"}))
        .expect("cancellation does not submit fields");
}

#[test]
fn incompatible_bounds_and_sensitive_fields_are_rejected_before_display() {
    assert!(
        parse_request(
            &json!({"mode":"form","requestId":1,"message":"Invalid","requestedSchema":{
                "properties":{"value":{"type":"string","minLength":3,"maxLength":2}}
            }})
        )
        .is_err()
    );
    assert!(
        parse_request(
            &json!({"mode":"form","requestId":1,"message":"Invalid","requestedSchema":{
                "properties":{"value":{"type":"string","writeOnly":true}}
            }})
        )
        .is_err()
    );
    assert!(
        parse_request(
            &json!({"mode":"form","requestId":1,"message":"Invalid","requestedSchema":{
                "properties":{"value":{"type":"array","items":{"enum":["a"]},"minItems":-1}}
            }})
        )
        .is_err()
    );
}
