use crate::shared::i18n::tr;
use bas_client::acp;

pub(super) fn feedback(response: &acp::PromptResponse) -> Option<String> {
    // CodeBuddy reports model failures as a successful RPC with a refusal stop reason.
    if let Some(message) = response
        .meta
        .as_ref()
        .and_then(|meta| meta.get("codebuddy.ai/errorMessage"))
        .and_then(serde_json::Value::as_str)
        .filter(|message| !message.trim().is_empty())
    {
        let error = serde_json::from_str::<serde_json::Value>(message).ok();
        return Some(
            error
                .as_ref()
                .and_then(|error| {
                    error
                        .pointer("/data/details")
                        .or_else(|| error.get("message"))
                })
                .and_then(serde_json::Value::as_str)
                .unwrap_or(message)
                .to_owned(),
        );
    }
    match response.stop_reason {
        acp::StopReason::EndTurn => None,
        acp::StopReason::Refusal => Some(tr!("turn-refused")),
        acp::StopReason::MaxTokens | acp::StopReason::MaxTurnRequests => {
            Some(tr!("turn-limit-reached"))
        }
        acp::StopReason::Cancelled => Some(tr!("turn-cancelled")),
        _ => Some(tr!("turn-ended")),
    }
}
