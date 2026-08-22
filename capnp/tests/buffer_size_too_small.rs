#![cfg(feature = "alloc")]

use capnp::serialize;

#[test]
fn buffer_segments_too_small() {
    let flat_slice: &[capnp::Word] = &[
        capnp::word(0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00), // 1 segment, 2 words
        capnp::word(0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00),
    ];
    let flat_slice: &[u8] = capnp::Word::words_to_bytes(flat_slice);
    let result = serialize::BufferSegments::new(flat_slice, Default::default());
    match result {
        Ok(_) => panic!("expected error"),
        Err(e) => match e.kind {
            capnp::ErrorKind::MessageEndsPrematurely(2, 1) => (),
            _ => panic!("wrong error"),
        },
    }
}
