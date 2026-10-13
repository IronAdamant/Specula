//! A tiny built-in TrueType font: every printable ASCII character is a solid box (capitals and
//! digits 0.7 em tall, lower case 0.5 em), 0.5 em advance, 1 em line height, units per em 1000.
//!
//! It exists so layout, `ui_check` and preview tests give the same answers on every machine,
//! including CI images with no fonts. Its metrics are close to real UI fonts (Segoe UI: cap
//! height 0.70 em, x-height 0.50 em), so legibility numbers carry over.

/// The font file bytes.
pub fn box_font() -> Vec<u8> {
    fn be16(v: &mut Vec<u8>, x: u16) {
        v.extend_from_slice(&x.to_be_bytes());
    }
    fn be32(v: &mut Vec<u8>, x: u32) {
        v.extend_from_slice(&x.to_be_bytes());
    }
    // Glyph 0 (.notdef) and glyph 1 (space) are empty; glyph c - 31 is ASCII c for 33..=126.
    let first = 32u16;
    let last = 126u16;
    let num_glyphs = (last - first + 2) as usize;
    let mut glyf = Vec::new();
    let mut loca = vec![0u32, 0u32];
    for c in first..=last {
        if c > first {
            let top: i16 = if (b'a' as u16..=b'z' as u16).contains(&c) { 500 } else { 700 };
            let pts = [(60i16, 0i16), (60, top), (440, top), (440, 0)];
            be16(&mut glyf, 1);
            for v in [60i16, 0, 440, top] {
                be16(&mut glyf, v as u16);
            }
            be16(&mut glyf, 3); // end point of contour 0
            be16(&mut glyf, 0); // no instructions
            glyf.extend_from_slice(&[1, 1, 1, 1]); // on-curve, 16-bit deltas
            let mut last = 0i16;
            for p in pts {
                be16(&mut glyf, (p.0 - last) as u16);
                last = p.0;
            }
            last = 0;
            for p in pts {
                be16(&mut glyf, (p.1 - last) as u16);
                last = p.1;
            }
            while glyf.len() % 4 != 0 {
                glyf.push(0);
            }
        }
        loca.push(glyf.len() as u32);
    }
    let mut loca_b = Vec::new();
    for o in &loca {
        be32(&mut loca_b, *o);
    }

    let mut head = vec![0u8; 54];
    head[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    head[12..16].copy_from_slice(&0x5F0F_3CF5u32.to_be_bytes());
    head[18..20].copy_from_slice(&1000u16.to_be_bytes());
    head[36..38].copy_from_slice(&0i16.to_be_bytes());
    head[38..40].copy_from_slice(&0i16.to_be_bytes());
    head[40..42].copy_from_slice(&500i16.to_be_bytes());
    head[42..44].copy_from_slice(&700i16.to_be_bytes());
    head[50..52].copy_from_slice(&1i16.to_be_bytes());

    let mut hhea = vec![0u8; 36];
    hhea[4..6].copy_from_slice(&800i16.to_be_bytes());
    hhea[6..8].copy_from_slice(&(-200i16).to_be_bytes());
    hhea[8..10].copy_from_slice(&0i16.to_be_bytes());
    hhea[34..36].copy_from_slice(&1u16.to_be_bytes());

    let mut maxp = vec![0u8; 6];
    maxp[0..4].copy_from_slice(&0x0000_5000u32.to_be_bytes());
    maxp[4..6].copy_from_slice(&(num_glyphs as u16).to_be_bytes());

    let mut hmtx = Vec::new();
    be16(&mut hmtx, 500);
    be16(&mut hmtx, 60);
    for _ in 1..num_glyphs {
        be16(&mut hmtx, 60);
    }

    // cmap format 4: 32..=126 -> c - 31, then the 0xFFFF terminator.
    let segs: [(u16, u16, i16); 2] = [(first, last, -31), (0xffff, 0xffff, 1)];
    let mut sub = Vec::new();
    be16(&mut sub, 4);
    be16(&mut sub, (16 + segs.len() * 8) as u16);
    be16(&mut sub, 0);
    be16(&mut sub, (segs.len() * 2) as u16);
    be16(&mut sub, 4);
    be16(&mut sub, 1);
    be16(&mut sub, 0);
    for s in &segs {
        be16(&mut sub, s.1);
    }
    be16(&mut sub, 0);
    for s in &segs {
        be16(&mut sub, s.0);
    }
    for s in &segs {
        be16(&mut sub, s.2 as u16);
    }
    for _ in &segs {
        be16(&mut sub, 0);
    }
    let mut cmap = Vec::new();
    be16(&mut cmap, 0);
    be16(&mut cmap, 1);
    be16(&mut cmap, 3);
    be16(&mut cmap, 1);
    be32(&mut cmap, 12);
    cmap.extend_from_slice(&sub);

    let family: Vec<u8> = "Specula Box".encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
    let mut name = Vec::new();
    be16(&mut name, 0);
    be16(&mut name, 1);
    be16(&mut name, 6 + 12);
    for x in [3u16, 1, 0x409, 1, family.len() as u16, 0] {
        be16(&mut name, x);
    }
    name.extend_from_slice(&family);

    let tables: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"cmap", cmap),
        (b"glyf", glyf),
        (b"head", head),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
        (b"loca", loca_b),
        (b"maxp", maxp),
        (b"name", name),
    ];
    let mut out = Vec::new();
    be32(&mut out, 0x0001_0000);
    be16(&mut out, tables.len() as u16);
    be16(&mut out, 0);
    be16(&mut out, 0);
    be16(&mut out, 0);
    let mut offset = 12 + tables.len() * 16;
    let mut bodies = Vec::new();
    for (tag, body) in &tables {
        out.extend_from_slice(*tag);
        be32(&mut out, 0);
        be32(&mut out, offset as u32);
        be32(&mut out, body.len() as u32);
        let mut b = body.clone();
        while b.len() % 4 != 0 {
            b.push(0);
        }
        offset += b.len();
        bodies.push(b);
    }
    for b in bodies {
        out.extend_from_slice(&b);
    }
    out
}

/// The box font, parsed.
pub fn box_font_parsed() -> specula_text::Font {
    specula_text::Font::from_bytes(box_font()).expect("the built-in box font parses")
}

#[cfg(test)]
mod tests {
    use super::*;
    use specula_text::{layout, TextStyle};

    #[test]
    fn box_font_metrics() {
        let f = box_font_parsed();
        assert_eq!(f.units_per_em(), 1000);
        assert_eq!(f.family_name(), "Specula Box");
        assert_eq!(f.cap_height(), Some(700));
        assert!(f.glyph_index('A').is_some() && f.glyph_index('~').is_some());
        assert_eq!(f.glyph_index('é'), None);
        let l = layout(&f, "Dust 150", &TextStyle::new(20.0));
        // 8 characters × 0.5 em × 20 px.
        assert_eq!(l.lines[0].width, 80.0);
        assert_eq!(l.height, 20.0);
    }
}
