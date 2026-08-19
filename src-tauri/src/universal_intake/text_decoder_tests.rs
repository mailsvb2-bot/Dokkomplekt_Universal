use super::decode_text_bytes;

#[test]
fn plain_text_decoder_preserves_utf8_bom_and_windows_1251() {
    let mut utf8_bom = vec![0xEF, 0xBB, 0xBF];
    utf8_bom.extend_from_slice("Привет".as_bytes());
    assert_eq!(decode_text_bytes(&utf8_bom), "Привет");
    assert_eq!(
        decode_text_bytes(&[0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2]),
        "Привет"
    );
}
