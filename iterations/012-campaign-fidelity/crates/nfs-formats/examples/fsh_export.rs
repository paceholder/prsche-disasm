//! Export every image of an FSH (EA SHPI) pack to RGBA PNGs.
//! Usage: fsh_export [--assemble] <in.fsh> <out_dir>
//!
//! `--assemble` additionally writes `<pack>_assembled.png`, composing every image of the
//! pack on one canvas at its header position (x_pos, y_pos).  If all positions are zero
//! the images are laid out on a grid in directory order instead (reported on stderr).
use nfs_formats::{parse_fsh_entries, Image};
use std::{fmt::Write as _, fs, io::BufWriter, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().collect();
    let assemble = args.iter().any(|a| a == "--assemble");
    args.retain(|a| a != "--assemble");
    if args.len() != 3 {
        eprintln!("usage: fsh_export [--assemble] <in.fsh> <out_dir>");
        std::process::exit(2);
    }
    let input = Path::new(&args[1]);
    let out = Path::new(&args[2]);
    let pack = input.file_stem().and_then(|s| s.to_str()).unwrap_or("pack");
    fs::create_dir_all(out)?;
    let entries = parse_fsh_entries(&fs::read(input)?)?;
    let mut list = String::new();
    for e in &entries {
        match &e.image {
            Ok(img) => {
                let name: String = e
                    .name
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || c == '_' {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect();
                let file = out.join(format!("{pack}_{}_{name}.png", e.index));
                let mut enc = png::Encoder::new(
                    BufWriter::new(fs::File::create(file)?),
                    img.width,
                    img.height,
                );
                enc.set_color(png::ColorType::Rgba);
                enc.set_depth(png::BitDepth::Eight);
                enc.write_header()?.write_image_data(&img.rgba)?;
                writeln!(
                    list,
                    "{} {:?} {}x{} format 0x{:02x}",
                    e.index, e.name, e.width, e.height, e.format
                )?;
            }
            Err(err) => writeln!(
                list,
                "{} {:?} {}x{} format 0x{:02x} UNSUPPORTED ({err})",
                e.index, e.name, e.width, e.height, e.format
            )?,
        }
    }
    fs::write(out.join(format!("{pack}.txt")), list)?;
    if assemble {
        let imgs: Vec<&Image> = entries
            .iter()
            .filter_map(|e| e.image.as_ref().ok())
            .collect();
        let (w, h, rgba, grid) = assemble_images(&imgs);
        if grid {
            eprintln!("{pack}: all positions are zero, using a grid fallback");
        }
        let file = out.join(format!("{pack}_assembled.png"));
        let mut enc = png::Encoder::new(BufWriter::new(fs::File::create(file)?), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()?.write_image_data(&rgba)?;
    }
    Ok(())
}

/// Compose images at their (x, y) header positions; returns (width, height, RGBA, used_grid).
fn assemble_images(imgs: &[&Image]) -> (u32, u32, Vec<u8>, bool) {
    let grid = imgs.len() > 1 && imgs.iter().all(|i| i.x == 0 && i.y == 0);
    let pos: Vec<(u32, u32)> = if grid {
        let cw = imgs.iter().map(|i| i.width).max().unwrap_or(1);
        let ch = imgs.iter().map(|i| i.height).max().unwrap_or(1);
        let cols = (imgs.len() as f64).sqrt().ceil() as u32;
        (0..imgs.len() as u32)
            .map(|k| ((k % cols) * cw, (k / cols) * ch))
            .collect()
    } else {
        imgs.iter().map(|i| (i.x, i.y)).collect()
    };
    let w = imgs
        .iter()
        .zip(&pos)
        .map(|(i, p)| p.0 + i.width)
        .max()
        .unwrap_or(1);
    let h = imgs
        .iter()
        .zip(&pos)
        .map(|(i, p)| p.1 + i.height)
        .max()
        .unwrap_or(1);
    let mut canvas = vec![0u8; (w * h * 4) as usize];
    for (img, &(px, py)) in imgs.iter().zip(&pos) {
        for row in 0..img.height {
            let src = (row * img.width * 4) as usize;
            let dst = (((py + row) * w + px) * 4) as usize;
            let n = (img.width * 4) as usize;
            canvas[dst..dst + n].copy_from_slice(&img.rgba[src..src + n]);
        }
    }
    (w, h, canvas, grid)
}
