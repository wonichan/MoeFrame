//! 手工生成小型真实损坏 PNG；只破坏 PLTE 长度，其余块与校验保持有效。
//! 复现锁定 png 0.18.1 的 create_rgba_palette 路径：
//! https://docs.rs/crate/png/0.18.1/source/src/decoder/transform/palette.rs
pub fn malformed_palette_png() -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    append_chunk(&mut png, *b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 3, 0, 0, 0]);
    append_chunk(&mut png, *b"PLTE", &[10, 20, 30, 40]);
    // zlib stored block: filter byte 0, palette index 0; Adler32 = 0x00020001.
    append_chunk(
        &mut png,
        *b"IDAT",
        &[0x78, 1, 1, 2, 0, 0xfd, 0xff, 0, 0, 0, 2, 0, 1],
    );
    append_chunk(&mut png, *b"IEND", &[]);
    png
}
fn append_chunk(png: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    png.extend_from_slice(&kind);
    png.extend_from_slice(data);
    let mut crc = u32::MAX;
    for byte in kind.iter().chain(data) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 == 1 { 0xedb8_8320 } else { 0 };
        }
    }
    png.extend_from_slice(&(!crc).to_be_bytes());
}
