use super::*;

#[test]
fn compressed_blocks_reject_inflation_size_mismatch_and_trailing_data() {
    let event = Event::new(
        bas_protocol::model::SessionId::new(),
        bas_protocol::model::Cursor::from_raw(1),
        bas_protocol::model::EventKind::TitleChanged {
            title: "content".repeat(1000),
        },
    );
    let encoded = encode(&[event]).expect("compressed block");
    assert_eq!(encoded.codec, ZLIB);
    assert!(decode(ZLIB, (MAX_BLOCK_BYTES + 1) as i64, &encoded.payload).is_err());
    assert!(decode(ZLIB, 1, &encoded.payload).is_err());
    assert!(
        decode(
            ZLIB,
            encoded.raw_bytes as i64,
            &encoded.payload[..encoded.payload.len() - 1]
        )
        .is_err()
    );
    let mut trailing = encoded.payload;
    trailing.push(0);
    assert!(decode(ZLIB, encoded.raw_bytes as i64, &trailing).is_err());
}
