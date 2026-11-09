use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};

thread_local! {
    static CHINESE: FluentBundle<FluentResource> = bundle("zh-CN", include_str!("i18n/assets/zh-CN.ftl"));
    static ENGLISH: FluentBundle<FluentResource> = bundle("en", include_str!("i18n/assets/en.ftl"));
}

fn bundle(language: &str, source: &str) -> FluentBundle<FluentResource> {
    let mut bundle = FluentBundle::new(vec![language.parse().expect("valid bundled locale")]);
    bundle
        .add_resource(
            FluentResource::try_new(source.to_owned()).expect("valid bundled Fluent resource"),
        )
        .expect("unique bundled translation keys");
    bundle
}

pub(crate) fn translate(key: &str) -> String {
    format(key, None)
}
pub(crate) fn count(key: &str, count: usize) -> String {
    let mut args = FluentArgs::new();
    args.set("count", count);
    format(key, Some(&args))
}
fn format(key: &str, args: Option<&FluentArgs>) -> String {
    let format = |bundle: &FluentBundle<FluentResource>| {
        bundle
            .get_message(key)
            .and_then(|m| m.value())
            .map(|value| bundle.format_pattern(value, args, &mut vec![]).into_owned())
            .unwrap_or_else(|| key.to_owned())
    };
    let language = super::preferences::state().get().language;
    if language.starts_with("zh")
        || (language == "system" && nsh_core::prelude::locale().to_string().starts_with("zh"))
    {
        CHINESE.with(format)
    } else {
        ENGLISH.with(format)
    }
}

macro_rules! tr {
    ($key:expr, $count:expr) => {
        $crate::shared::i18n::count($key, $count)
    };
    ($key:expr) => {
        $crate::shared::i18n::translate($key)
    };
}
pub(crate) use tr;
