use super::read_bounded;
use std::io::Cursor;

#[test]
fn bounded_reader_accepts_body_at_limit() {
    assert_eq!(read_bounded(Cursor::new(b"1234"), 4).unwrap(), b"1234");
}

#[test]
fn bounded_reader_rejects_body_over_limit() {
    assert!(read_bounded(Cursor::new(b"12345"), 4)
        .unwrap_err()
        .contains("exceeded 4 byte body limit"));
}
