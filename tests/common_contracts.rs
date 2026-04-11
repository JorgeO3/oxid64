use oxid64::Base64Decoder;
use oxid64::engine::common::{
    can_advance, can_read, decode_offsets, prepare_decode_output, remaining, safe_in_end_4,
    safe_in_end_for_width,
};
use oxid64::engine::scalar::encode_base64_fast;
use oxid64::engine::{Decoder, dispatch_decode};

unsafe fn stub_decode_prefix(input: &[u8], out: &mut [u8]) -> Option<(usize, usize)> {
    if input.len() < 4 || out.len() < 3 {
        return None;
    }
    out[..3].copy_from_slice(b"Man");
    Some((4, 3))
}

unsafe fn stub_decode_none(_input: &[u8], _out: &mut [u8]) -> Option<(usize, usize)> {
    None
}

#[test]
fn remaining_and_can_read_saturate_safely() {
    let buf = [0u8; 8];
    let start = buf.as_ptr();
    let end = unsafe { start.add(buf.len()) };

    assert_eq!(remaining(start, end), 8);
    assert!(can_read(start, end, 8));
    assert!(!can_read(start, end, 9));

    let mid = unsafe { start.add(5) };
    assert_eq!(remaining(mid, end), 3);

    let ptr_after_end = unsafe { start.add(6) };
    let smaller_end = unsafe { start.add(3) };
    assert_eq!(remaining(ptr_after_end, smaller_end), 0);
    assert!(!can_read(ptr_after_end, smaller_end, 1));
}

#[test]
fn can_advance_checks_both_input_and_output_contracts() {
    let in_buf = [0u8; 16];
    let mut out_buf = [0u8; 16];

    let in_ptr = in_buf.as_ptr();
    let in_end = unsafe { in_ptr.add(in_buf.len()) };
    let out_ptr = out_buf.as_mut_ptr();
    let out_end = unsafe { out_ptr.add(out_buf.len()) };

    assert!(can_advance(in_ptr, in_end, 8, out_ptr, out_end, 8));
    assert!(!can_advance(in_ptr, in_end, 17, out_ptr, out_end, 8));
    assert!(!can_advance(in_ptr, in_end, 8, out_ptr, out_end, 17));
}

#[test]
fn safe_end_helpers_match_expected_tail_start() {
    let short = [1u8, 2, 3];
    assert_eq!(safe_in_end_for_width(&short, 4), short.as_ptr());
    assert_eq!(safe_in_end_4(&short), short.as_ptr());

    let buf = [0u8; 10];
    let expected = unsafe { buf.as_ptr().add(6) };
    assert_eq!(safe_in_end_for_width(&buf, 4), expected);
    assert_eq!(safe_in_end_4(&buf), expected);
}

#[test]
fn prepare_decode_output_validates_capacity() {
    let input = b"TQ==";
    let too_small = [0u8; 0];
    let just_right = [0u8; 1];

    assert_eq!(prepare_decode_output(input, &too_small), None);
    assert_eq!(prepare_decode_output(input, &just_right), Some(1));
    assert_eq!(prepare_decode_output(b"A", &just_right), None);
}

#[test]
fn decode_offsets_returns_pointer_deltas() {
    let in_buf = [0u8; 12];
    let mut out_buf = [0u8; 12];

    let in_base = in_buf.as_ptr();
    let out_base = out_buf.as_mut_ptr();
    let in_ptr = unsafe { in_base.add(7) };
    let out_ptr = unsafe { out_base.add(5) };

    assert_eq!(decode_offsets(in_ptr, out_ptr, in_base, out_base), (7, 5));
}

#[test]
fn dispatch_decode_finishes_scalar_tail() {
    let input = b"TWFuTWFu";
    let mut out = [0u8; 6];
    let written = dispatch_decode(input, &mut out, stub_decode_prefix)
        .expect("tail fallback should complete");
    assert_eq!(written, 6);
    assert_eq!(&out[..written], b"ManMan");
}

#[test]
fn dispatch_decode_propagates_failure() {
    let mut out = [0u8; 4];
    assert_eq!(dispatch_decode(b"TWFu", &mut out, stub_decode_none), None);
}

#[test]
fn decoder_detect_roundtrips_small_payload() {
    let engine = Decoder::detect();
    let input = b"dispatch-contract";
    let encoded = engine.encode(input);
    let decoded = engine
        .decode(&encoded)
        .expect("detected engine must roundtrip valid input");
    assert_eq!(decoded, input);
}

#[test]
fn decoder_slice_api_respects_exact_windows() {
    let engine = Decoder::detect();
    let input = b"miri-window";
    let mut encoded = vec![0u8; input.len().div_ceil(3) * 4];
    let enc_written = encode_base64_fast(input, &mut encoded);
    encoded.truncate(enc_written);

    let mut out = vec![0u8; input.len() + 3];
    let written = engine
        .decode_to_slice(&encoded, &mut out[1..1 + input.len()])
        .expect("valid input should decode");
    assert_eq!(written, input.len());
    assert_eq!(&out[1..1 + written], input);
}
