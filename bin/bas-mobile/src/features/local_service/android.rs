use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JObject, JString, JValue},
};
use std::sync::OnceLock;

struct Bridge {
    vm: JavaVM,
    object: GlobalRef,
}
static BRIDGE: OnceLock<Bridge> = OnceLock::new();

/// Called by the Activity after NSH loads the Rust library.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_gandesktop_bas_runtime_RuntimeBridge_initialize(
    mut env: JNIEnv<'_>,
    object: JObject<'_>,
) {
    let result = (|| -> jni::errors::Result<()> {
        let bridge = Bridge {
            vm: env.get_java_vm()?,
            object: env.new_global_ref(object)?,
        };
        let _ = BRIDGE.set(bridge);
        Ok(())
    })();
    if let Err(error) = result {
        let _ = env.throw_new("java/lang/IllegalStateException", error.to_string());
    }
}

pub(super) fn call(action: &str) -> anyhow::Result<super::Status> {
    let bridge = BRIDGE
        .get()
        .ok_or_else(|| anyhow::anyhow!("Android runtime bridge is unavailable"))?;
    let mut env = bridge.vm.attach_current_thread()?;
    let input = env.new_string(action)?;
    let result = env.call_method(
        bridge.object.as_obj(),
        "call",
        "(Ljava/lang/String;)Ljava/lang/String;",
        &[JValue::Object(&input)],
    );
    if env.exception_check()? {
        let exception = env.exception_occurred()?;
        env.exception_clear()?;
        let message = env
            .call_method(exception, "toString", "()Ljava/lang/String;", &[])?
            .l()?;
        let message: String = env.get_string(&JString::from(message))?.into();
        anyhow::bail!(message);
    }
    let result = JString::from(result?.l()?);
    let json: String = env.get_string(&result)?.into();
    Ok(serde_json::from_str(&json)?)
}
