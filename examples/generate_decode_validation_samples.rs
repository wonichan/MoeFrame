//! Generates reproducible synthetic fixtures; generation is NOT a validation-process memory run.
//! All output is opt-in and lives under the supplied, initially empty directory.
use image::{Rgb, RgbImage};
use std::{env, error::Error, fs, io::Write, path::Path};
#[path = "../tests/support/decode_samples.rs"]
mod decode_samples;

fn main() -> Result<(), Box<dyn Error>> {
    let output = env::args_os()
        .nth(1)
        .ok_or("provide an empty output directory")?;
    let root = Path::new(&output);
    if root.exists() && fs::read_dir(root)?.next().is_some() {
        return Err("refusing to overwrite an existing nonempty sample directory".into());
    }
    fs::create_dir_all(root)?;
    let mut manifest = fs::File::create_new(root.join("samples.tsv"))?;
    writeln!(manifest, "path\twidth\theight\tencoded_bytes\tcase")?;
    let mut ordinary = Vec::new();
    let mut mixed = Vec::new();
    for (tier, width, height) in [
        ("1080p", 1920, 1080),
        ("2k", 2560, 1440),
        ("4k", 3840, 2160),
    ] {
        let image = pattern(width, height);
        for extension in ["png", "jpg", "webp", "bmp"] {
            let name = format!("{tier}-{width}x{height}.{extension}");
            image.save(root.join(&name))?;
            record(&mut manifest, root, &name, width, height, "ordinary_valid")?;
            ordinary.push(name.clone());
            mixed.push(name);
        }
        // At most one generator-owned full-resolution image per tier; not the measured process.
    }
    // The additional 2048x1080 case is explicit; dimensions, not tier labels, are evidence.
    let name = "2k-2048x1080.png";
    pattern(2048, 1080).save(root.join(name))?;
    record(&mut manifest, root, name, 2048, 1080, "ordinary_valid")?;
    ordinary.push(name.into());
    mixed.push(name.into());

    let name = "large-100mp-10000x10000.png";
    pattern(10000, 10000).save(root.join(name))?;
    record(
        &mut manifest,
        root,
        name,
        10000,
        10000,
        "valid_100mp_allowed_to_be_rejected",
    )?;
    mixed.insert(1, name.into());
    let name = "extreme-aspect-10000x1.png";
    pattern(10000, 1).save(root.join(name))?;
    record(
        &mut manifest,
        root,
        name,
        10000,
        1,
        "valid_extreme_fill_coverage",
    )?;
    mixed.insert(3, name.into());

    let source = fs::read(root.join(&ordinary[0]))?;
    fs::write(root.join("truncated.png"), &source[..source.len() / 2])?;
    record(
        &mut manifest,
        root,
        "truncated.png",
        1920,
        1080,
        "truncated_png",
    )?;
    fs::write(root.join("png-disguised.jpg"), &source)?;
    record(
        &mut manifest,
        root,
        "png-disguised.jpg",
        1920,
        1080,
        "extension_mismatch",
    )?;
    fs::write(root.join("bad.jpg"), b"not an image\0real corrupt file\n")?;
    record(&mut manifest, root, "bad.jpg", 0, 0, "corrupt_header")?;
    fs::write(
        root.join("bad-palette.png"),
        decode_samples::malformed_palette_png(),
    )?;
    record(
        &mut manifest,
        root,
        "bad-palette.png",
        1,
        1,
        "png0181_palette_unwinding_panic",
    )?;
    mixed.insert(0, "bad-palette.png".into());
    mixed.insert(4, "truncated.png".into());
    mixed.insert(6, "bad.jpg".into());
    mixed.insert(8, "png-disguised.jpg".into());

    // Logs must retain the exact relative names; no fixture whitelist or silent filtering.
    fs::write(root.join("ordinary.txt"), ordinary.join("\n") + "\n")?;
    fs::write(root.join("mixed.txt"), mixed.join("\n") + "\n")?;
    fs::write(
        root.join("large-and-valid.txt"),
        "large-100mp-10000x10000.png\n1080p-1920x1080.png\n4k-3840x2160.jpg\n",
    )?;
    fs::write(
        root.join("panic-and-valid.txt"),
        "bad-palette.png\n1080p-1920x1080.png\n",
    )?;
    fs::write(
        root.join("all-bad.txt"),
        "bad-palette.png\ntruncated.png\npng-disguised.jpg\nbad.jpg\n",
    )?;
    println!(
        "Generated {} ordinary and {} mixed paths in {}",
        ordinary.len(),
        mixed.len(),
        root.display()
    );
    Ok(())
}
fn pattern(width: u32, height: u32) -> RgbImage {
    RgbImage::from_fn(width, height, |x, y| {
        if x < width / 12 {
            return Rgb([240, 30, 30]);
        }
        if x > width - width / 12 {
            return Rgb([30, 30, 240]);
        }
        if x > width / 3 && x < width * 2 / 3 && y > height / 3 && y < height * 2 / 3 {
            return Rgb([20, 220, 40]);
        }
        Rgb([
            (x * 255 / width) as u8,
            (y * 255 / height) as u8,
            (60 + ((x / 32 + y / 32) % 2) * 100) as u8,
        ])
    })
}
fn record(
    file: &mut fs::File,
    root: &Path,
    name: &str,
    width: u32,
    height: u32,
    case: &str,
) -> std::io::Result<()> {
    writeln!(
        file,
        "{name}\t{width}\t{height}\t{}\t{case}",
        fs::metadata(root.join(name))?.len()
    )
}
