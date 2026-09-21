use std::fs;
use std::path::Path;

fn write_default_windows_icon() {
    let icon_dir = Path::new("icons");
    let icon_path = icon_dir.join("icon.ico");

    if icon_path.exists() {
        return;
    }

    fs::create_dir_all(icon_dir).expect("failed to create icons directory");

    const WIDTH: u32 = 16;
    const HEIGHT: u32 = 16;
    const PIXELS: usize = (WIDTH as usize) * (HEIGHT as usize);
    const AND_MASK_BYTES: usize = (WIDTH as usize).div_ceil(32) * 4 * (HEIGHT as usize);

    let dib_size = 40usize;
    let xor_size = PIXELS * 4;
    let image_size = dib_size + xor_size + AND_MASK_BYTES;

    let mut ico = Vec::with_capacity(6 + 16 + image_size);

    // ICONDIR
    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());

    // ICONDIRENTRY
    ico.push(WIDTH as u8);
    ico.push(HEIGHT as u8);
    ico.push(0);
    ico.push(0);
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&32u16.to_le_bytes());
    ico.extend_from_slice(&(image_size as u32).to_le_bytes());
    ico.extend_from_slice(&(22u32).to_le_bytes());

    // BITMAPINFOHEADER
    ico.extend_from_slice(&40u32.to_le_bytes());
    ico.extend_from_slice(&WIDTH.to_le_bytes());
    ico.extend_from_slice(&(HEIGHT * 2).to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&32u16.to_le_bytes());
    ico.extend_from_slice(&0u32.to_le_bytes());
    ico.extend_from_slice(&((xor_size + AND_MASK_BYTES) as u32).to_le_bytes());
    ico.extend_from_slice(&2835u32.to_le_bytes());
    ico.extend_from_slice(&2835u32.to_le_bytes());
    ico.extend_from_slice(&0u32.to_le_bytes());
    ico.extend_from_slice(&0u32.to_le_bytes());

    // BGRA pixel data, bottom-up. The simple mark is a dark-blue square with a white D.
    for y in (0..HEIGHT).rev() {
        for x in 0..WIDTH {
            let outer = x == 0 || y == 0 || x == WIDTH - 1 || y == HEIGHT - 1;
            let d_shape = x >= 4 && x <= 11 && y >= 3 && y <= 12 && (x <= 6 || y <= 4 || y >= 11 || x >= 10);
            let (b, g, r, a) = if outer {
                (0x55, 0x2A, 0x00, 0xFF)
            } else if d_shape {
                (0xFF, 0xFF, 0xFF, 0xFF)
            } else {
                (0x55, 0x2A, 0x00, 0xFF)
            };
            ico.extend_from_slice(&[b, g, r, a]);
        }
    }

    // Fully transparent AND mask because the 32-bit alpha channel is authoritative.
    ico.extend(std::iter::repeat_n(0u8, AND_MASK_BYTES));

    fs::write(&icon_path, ico).expect("failed to write generated Windows icon");
}

fn main() {
    write_default_windows_icon();
    tauri_build::build();
}
