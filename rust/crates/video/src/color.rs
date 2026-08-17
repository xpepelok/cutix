#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorSpec {
    pub full_range: bool,
    pub matrix: Matrix,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Matrix {
    Bt601,
    Bt709,
    Bt2020,
}

impl Matrix {
    const fn luma_weights(self) -> (f32, f32) {
        match self {
            Self::Bt601 => (0.299, 0.114),
            Self::Bt709 => (0.2126, 0.0722),
            Self::Bt2020 => (0.2627, 0.0593),
        }
    }

    pub const fn assumed_for_height(height: usize) -> Self {
        if height <= 576 {
            Self::Bt601
        } else {
            Self::Bt709
        }
    }

    fn from_coefficients(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Bt709),
            4 | 5 | 6 | 7 => Some(Self::Bt601),
            9 | 10 => Some(Self::Bt2020),
            _ => None,
        }
    }
}

impl ColorSpec {
    pub const fn assumed_for_height(height: usize) -> Self {
        Self {
            full_range: false,
            matrix: Matrix::assumed_for_height(height),
        }
    }

    fn coefficients(self) -> (f32, f32, f32, f32, f32, f32) {
        let (kr, kb) = self.matrix.luma_weights();
        let kg = 1.0 - kr - kb;
        let (offset, luma_scale, chroma_scale) = if self.full_range {
            (0.0, 1.0, 1.0)
        } else {
            (16.0, 255.0 / 219.0, 255.0 / 224.0)
        };
        (
            offset,
            luma_scale,
            2.0 * (1.0 - kr) * chroma_scale,
            -2.0 * kb * (1.0 - kb) / kg * chroma_scale,
            -2.0 * kr * (1.0 - kr) / kg * chroma_scale,
            2.0 * (1.0 - kb) * chroma_scale,
        )
    }
}

pub fn i420_to_rgba(
    y_plane: &[u8],
    u_plane: &[u8],
    v_plane: &[u8],
    dimensions: (usize, usize),
    strides: (usize, usize, usize),
    spec: ColorSpec,
    target: &mut [u8],
) {
    let (width, height) = dimensions;
    let (offset, luma, vr, ug, vg, ub) = spec.coefficients();

    for row in 0..height {
        let chroma_row = row / 2;
        for column in 0..width {
            let luma_sample = f32::from(y_plane[row * strides.0 + column]);
            let u = f32::from(u_plane[chroma_row * strides.1 + column / 2]) - 128.0;
            let v = f32::from(v_plane[chroma_row * strides.2 + column / 2]) - 128.0;
            let base = (luma_sample - offset) * luma;

            let pixel = (row * width + column) * 4;
            target[pixel] = clamp(vr.mul_add(v, base));
            target[pixel + 1] = clamp(vg.mul_add(v, ug.mul_add(u, base)));
            target[pixel + 2] = clamp(ub.mul_add(u, base));
            target[pixel + 3] = 255;
        }
    }
}

fn clamp(value: f32) -> u8 {
    if value <= 0.0 {
        0
    } else if value >= 255.0 {
        255
    } else {
        (value + 0.5) as u8
    }
}

struct BitReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn bit(&mut self) -> Option<u32> {
        let byte = self.bytes.get(self.position / 8)?;
        let bit = (byte >> (7 - self.position % 8)) & 1;
        self.position += 1;
        Some(u32::from(bit))
    }

    fn bits(&mut self, count: u32) -> Option<u32> {
        let mut value = 0u32;
        for _ in 0..count {
            value = (value << 1) | self.bit()?;
        }
        Some(value)
    }

    fn unsigned_golomb(&mut self) -> Option<u32> {
        let mut leading = 0u32;
        while self.bit()? == 0 {
            leading += 1;
            if leading > 31 {
                return None;
            }
        }
        if leading == 0 {
            return Some(0);
        }
        Some((1u32 << leading) - 1 + self.bits(leading)?)
    }

    fn signed_golomb(&mut self) -> Option<i32> {
        self.unsigned_golomb().map(|value| {
            let magnitude = value.div_ceil(2) as i32;
            if value % 2 == 0 {
                -magnitude
            } else {
                magnitude
            }
        })
    }
}

fn rbsp(payload: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(payload.len());
    let mut zeros = 0usize;
    for &byte in payload {
        if zeros >= 2 && byte == 0x03 {
            zeros = 0;
            continue;
        }
        if byte == 0 {
            zeros += 1;
        } else {
            zeros = 0;
        }
        output.push(byte);
    }
    output
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpsColor {
    pub full_range: bool,
    pub matrix: Option<Matrix>,
}

pub fn color_spec_from_sps(nal: &[u8]) -> Option<SpsColor> {
    let payload = match nal.first() {
        Some(header) if header & 0x1f == 7 => &nal[1..],
        Some(_) if nal.len() > 4 && nal[0] == 0 => return None,
        _ => nal,
    };
    let bytes = rbsp(payload);
    let mut reader = BitReader::new(&bytes);

    let profile_idc = reader.bits(8)?;
    reader.bits(8)?;
    reader.bits(8)?;
    reader.unsigned_golomb()?;

    if matches!(
        profile_idc,
        100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135
    ) {
        let chroma_format_idc = reader.unsigned_golomb()?;
        if chroma_format_idc == 3 {
            reader.bit()?;
        }
        reader.unsigned_golomb()?;
        reader.unsigned_golomb()?;
        reader.bit()?;
        if reader.bit()? == 1 {
            let lists = if chroma_format_idc == 3 { 12 } else { 8 };
            for index in 0..lists {
                if reader.bit()? == 1 {
                    skip_scaling_list(&mut reader, if index < 6 { 16 } else { 64 })?;
                }
            }
        }
    }

    reader.unsigned_golomb()?;
    let pic_order_cnt_type = reader.unsigned_golomb()?;
    if pic_order_cnt_type == 0 {
        reader.unsigned_golomb()?;
    } else if pic_order_cnt_type == 1 {
        reader.bit()?;
        reader.signed_golomb()?;
        reader.signed_golomb()?;
        let cycle = reader.unsigned_golomb()?;
        for _ in 0..cycle.min(256) {
            reader.signed_golomb()?;
        }
    }

    reader.unsigned_golomb()?;
    reader.bit()?;
    reader.unsigned_golomb()?;
    reader.unsigned_golomb()?;
    if reader.bit()? == 0 {
        reader.bit()?;
    }
    reader.bit()?;
    if reader.bit()? == 1 {
        for _ in 0..4 {
            reader.unsigned_golomb()?;
        }
    }

    if reader.bit()? != 1 {
        return None;
    }

    if reader.bit()? == 1 {
        let aspect_ratio_idc = reader.bits(8)?;
        if aspect_ratio_idc == 255 {
            reader.bits(16)?;
            reader.bits(16)?;
        }
    }
    if reader.bit()? == 1 {
        reader.bit()?;
    }
    if reader.bit()? != 1 {
        return None;
    }

    reader.bits(3)?;
    let full_range = reader.bit()? == 1;
    let matrix = if reader.bit()? == 1 {
        reader.bits(8)?;
        reader.bits(8)?;
        Matrix::from_coefficients(reader.bits(8)? as u8)
    } else {
        None
    };

    Some(SpsColor { full_range, matrix })
}

pub fn resolve(signalled: Option<SpsColor>, height: usize) -> ColorSpec {
    match signalled {
        Some(sps) => ColorSpec {
            full_range: sps.full_range,
            matrix: sps
                .matrix
                .unwrap_or_else(|| Matrix::assumed_for_height(height)),
        },
        None => ColorSpec::assumed_for_height(height),
    }
}

fn skip_scaling_list(reader: &mut BitReader<'_>, size: usize) -> Option<()> {
    let mut last = 8i32;
    let mut next = 8i32;
    for _ in 0..size {
        if next != 0 {
            next = (last + reader.signed_golomb()?).rem_euclid(256);
        }
        last = if next == 0 { last } else { next };
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert(y: u8, u: u8, v: u8, spec: ColorSpec) -> [u8; 3] {
        let mut target = [0u8; 4];
        i420_to_rgba(&[y], &[u], &[v], (1, 1), (1, 1, 1), spec, &mut target);
        [target[0], target[1], target[2]]
    }

    #[test]
    fn studio_range_black_decodes_to_zero() {
        let spec = ColorSpec::assumed_for_height(1080);
        assert_eq!(convert(16, 128, 128, spec), [0, 0, 0]);
    }

    #[test]
    fn studio_range_white_decodes_to_full_scale() {
        let spec = ColorSpec::assumed_for_height(1080);
        assert_eq!(convert(235, 128, 128, spec), [255, 255, 255]);
    }

    #[test]
    fn studio_range_clamps_below_the_black_level() {
        let spec = ColorSpec::assumed_for_height(1080);
        assert_eq!(convert(0, 128, 128, spec), [0, 0, 0]);
    }

    #[test]
    fn full_range_black_is_zero_and_white_is_full_scale() {
        let spec = ColorSpec {
            full_range: true,
            matrix: Matrix::Bt601,
        };
        assert_eq!(convert(0, 128, 128, spec), [0, 0, 0]);
        assert_eq!(convert(255, 128, 128, spec), [255, 255, 255]);
    }

    #[test]
    fn the_assumed_matrix_follows_the_raster_height() {
        assert_eq!(Matrix::assumed_for_height(480), Matrix::Bt601);
        assert_eq!(Matrix::assumed_for_height(576), Matrix::Bt601);
        assert_eq!(Matrix::assumed_for_height(720), Matrix::Bt709);
        assert_eq!(Matrix::assumed_for_height(1080), Matrix::Bt709);
    }

    #[test]
    fn the_two_matrices_differ_on_a_saturated_colour() {
        let bt601 = convert(
            81,
            90,
            240,
            ColorSpec {
                full_range: false,
                matrix: Matrix::Bt601,
            },
        );
        let bt709 = convert(
            81,
            90,
            240,
            ColorSpec {
                full_range: false,
                matrix: Matrix::Bt709,
            },
        );
        assert_ne!(bt601, bt709);
    }

    #[test]
    fn rbsp_removes_emulation_prevention_bytes() {
        assert_eq!(rbsp(&[0x00, 0x00, 0x03, 0x01]), vec![0x00, 0x00, 0x01]);
        assert_eq!(rbsp(&[0x00, 0x01, 0x03]), vec![0x00, 0x01, 0x03]);
    }

    #[test]
    fn golomb_decodes_the_canonical_codes() {
        let mut reader = BitReader::new(&[0b1_010_011_0, 0b0100_0101]);
        assert_eq!(reader.unsigned_golomb(), Some(0));
        assert_eq!(reader.unsigned_golomb(), Some(1));
        assert_eq!(reader.unsigned_golomb(), Some(2));
        assert_eq!(reader.unsigned_golomb(), Some(3));
    }

    #[test]
    fn an_sps_without_vui_reports_nothing() {
        assert_eq!(color_spec_from_sps(&[]), None);
    }
}
