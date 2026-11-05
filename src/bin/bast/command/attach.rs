use anyhow::{Context, Result};
use bas::service::{Client, pb};
use tokio_stream::StreamExt;

use crate::render;

/// Attaches to a session: prints the requested history, then streams events
/// until the process is interrupted.
///
/// # Errors
///
/// Returns an error when the daemon rejects the attachment or the stream fails.
pub(super) async fn run(
    client: &mut Client,
    session: &str,
    cursor: Option<u64>,
    replay: usize,
) -> Result<()> {
    let request = pb::AttachRequest {
        session: session.to_owned(),
        cursor,
        replay: u32::try_from(replay).unwrap_or(u32::MAX),
    };
    let mut protocol = client
        .observe_acp(request.clone())
        .await
        .context("ACP observation failed")?
        .into_inner();
    let mut management = client
        .attach(request)
        .await
        .context("management subscription failed")?
        .into_inner();
    let mut timeline = render::Timeline::default();
    let mut protocol_open = true;
    let mut management_open = true;
    loop {
        if !protocol_open && !management_open {
            break;
        }
        tokio::select! {
            event = protocol.next(), if protocol_open => {
                let Some(event) = event else { protocol_open = false; continue };
                for event in event.context("ACP observation failed")?.history_events()? {
                    timeline.event(&pb::Event::from(&event))?;
                }
            }
            event = management.next(), if management_open => {
                let Some(event) = event else { management_open = false; continue };
                let event = bas::model::Event::try_from(event.context("management subscription failed")?)?;
                timeline.event(&pb::Event::from(&event))?;
            }
        }
    }
    timeline.finish()
}
