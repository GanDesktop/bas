use super::*;

fn event(cursor: u64, size: usize) -> pb::Event {
    pb::Event {
        cursor,
        kind: Some(pb::event::Kind::Failed(pb::Failed {
            message: "x".repeat(size),
        })),
        ..Default::default()
    }
}

#[test]
fn continuation_budget_handles_a_varint_boundary_without_skipping_events() {
    let first = event(127, 128);
    let second = event(128, 128);
    let budget = pb::LoadHistoryResponse {
        events: vec![first.clone()],
        next: Some(127),
    }
    .encoded_len();
    let mut page = PageEncoder::new(budget);
    assert!(page.push(first.clone(), true).expect("first event fits"));
    assert!(
        !page
            .push(second.clone(), false)
            .expect("next page required")
    );
    assert_eq!(
        page.finish(),
        pb::LoadHistoryResponse {
            events: vec![first],
            next: Some(127)
        }
    );
    let mut page = PageEncoder::new(budget);
    assert!(
        page.push(second.clone(), false)
            .expect("remaining event fits")
    );
    assert_eq!(
        page.finish(),
        pb::LoadHistoryResponse {
            events: vec![second],
            next: None
        }
    );
}

#[test]
fn oversized_event_preserves_the_previous_page_and_fails_at_its_own_cursor() {
    let mut page = PageEncoder::new(100);
    assert!(page.push(event(1, 5), true).expect("small event"));
    assert!(
        !page
            .push(event(2, 200), false)
            .expect("stop before oversized event")
    );
    assert_eq!(page.finish().next, Some(1));
    assert_eq!(
        PageEncoder::new(100)
            .push(event(2, 200), false)
            .expect_err("oversized event cannot be skipped")
            .code(),
        tonic::Code::ResourceExhausted
    );
}
