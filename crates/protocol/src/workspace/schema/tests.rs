use super::*;
use serde_json::json;

#[derive(Debug, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
enum Sandbox {
    Existing {
        sandbox_id: String,
    },
    New {
        image: String,
        #[serde(default = "default_cpu")]
        #[schemars(range(min = 1))]
        cpu: u32,
    },
}

fn default_cpu() -> u32 {
    2
}

#[test]
fn provider_parameters_use_tagged_variants_and_serde_defaults() {
    let schema = generate::<Sandbox>();
    assert!(schema.get("oneOf").is_some());
    let validator = Validator::new(&schema).expect("generated schema");
    let existing = json!({"mode":"existing","sandbox_id":"box-1"});
    validator.validate(&existing).expect("existing sandbox");
    assert!(
        validator
            .validate(&json!({"mode":"existing","image":"image"}))
            .is_err()
    );
    assert!(
        validator
            .validate(&json!({"mode":"new","image":"image","cpu":0}))
            .is_err()
    );
    let new: Sandbox = validator
        .decode(json!({"mode":"new","image":"image"}))
        .expect("defaults");
    assert_eq!(
        new,
        Sandbox::New {
            image: "image".into(),
            cpu: 2
        }
    );
}

#[test]
fn directory_semantics_survive_generation_and_external_refs_are_not_loaded() {
    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct Parameters {
        directory: ServerDirectoryPath,
    }
    let schema = generate::<Parameters>();
    assert_eq!(
        schema["properties"]["directory"]["x-bas"]["type"],
        SERVER_DIRECTORY_PATH
    );
    let validator = Validator::new(&schema).expect("schema");
    assert!(validator.validate(&json!({"directory":""})).is_err());
    assert!(Validator::new(&json!({"$schema":DIALECT,"$ref":"file:///etc/passwd"})).is_err());
}
