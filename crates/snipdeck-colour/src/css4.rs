//! CSS Color 4 spaces used by the parser and the extra copy formats: Lab and LCH (D50), `OKLab`, and
//! display-P3. Out-of-gamut values are clamped per channel (the browser's fallback), not mapped.

use crate::model::{Rgba8, from_linear, linear, oklab_to_linear, to_oklab};

type M = [[f64; 3]; 3];

const SRGB_TO_XYZ: M = [
    [
        0.412_390_799_265_959_5,
        0.357_584_339_383_878,
        0.180_480_788_401_834_3,
    ],
    [
        0.212_639_005_871_510_36,
        0.715_168_678_767_756,
        0.072_192_315_360_733_71,
    ],
    [
        0.019_330_818_715_591_85,
        0.119_194_779_794_625_99,
        0.950_532_152_249_660_7,
    ],
];
const XYZ_TO_SRGB: M = [
    [
        3.240_969_941_904_522_6,
        -1.537_383_177_570_094,
        -0.498_610_760_293_003_4,
    ],
    [
        -0.969_243_636_280_879_6,
        1.875_967_501_507_720_2,
        0.041_555_057_407_175_59,
    ],
    [
        0.055_630_079_696_993_66,
        -0.203_976_958_888_976_52,
        1.056_971_514_242_878_6,
    ],
];
const D65_TO_D50: M = [
    [
        1.047_929_820_840_548_8,
        0.022_946_793_341_019_088,
        -0.050_192_229_543_135_57,
    ],
    [
        0.029_627_815_688_159_344,
        0.990_434_484_573_249,
        -0.017_073_825_029_385_14,
    ],
    [
        -0.009_243_058_152_591_178,
        0.015_055_144_896_577_895,
        0.751_874_289_958_000_8,
    ],
];
const D50_TO_D65: M = [
    [
        0.955_473_452_704_218_2,
        -0.023_098_536_874_261_423,
        0.063_259_308_661_021_7,
    ],
    [
        -0.028_369_706_963_208_136,
        1.009_995_458_005_822_6,
        0.021_041_398_966_943_008,
    ],
    [
        0.012_314_001_688_319_899,
        -0.020_507_696_433_477_912,
        1.330_365_936_608_075_3,
    ],
];
const P3_TO_SRGB: M = [
    [1.224_940_1, -0.224_940_4, 0.0],
    [-0.042_056_9, 1.042_057_1, 0.0],
    [-0.019_637_6, -0.078_636_1, 1.098_273_5],
];
const SRGB_TO_P3: M = [
    [0.822_461_969, 0.177_538_031, 0.0],
    [0.033_194_199, 0.966_805_801, 0.0],
    [0.017_082_631, 0.072_397_440, 0.910_519_929],
];
const D50: [f64; 3] = [0.964_295_676_429_567_7, 1.0, 0.825_104_602_510_46];

fn mul(m: &M, v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

const EPS: f64 = 216.0 / 24389.0;
const KAPPA: f64 = 24389.0 / 27.0;

/// CIE Lab (D50) to sRGB.
pub(crate) fn from_lab(l: f64, a: f64, b: f64, alpha: u8) -> Rgba8 {
    let fy = (l + 16.0) / 116.0;
    let fx = a / 500.0 + fy;
    let fz = fy - b / 200.0;
    let f = |t: f64| {
        if t.powi(3) > EPS {
            t.powi(3)
        } else {
            (116.0 * t - 16.0) / KAPPA
        }
    };
    let y = if l > KAPPA * EPS {
        fy.powi(3)
    } else {
        l / KAPPA
    };
    let xyz50 = [f(fx) * D50[0], y * D50[1], f(fz) * D50[2]];
    from_linear(mul(&XYZ_TO_SRGB, mul(&D50_TO_D65, xyz50)), alpha)
}

/// sRGB to CIE Lab (D50).
pub(crate) fn to_lab(c: Rgba8) -> (f64, f64, f64) {
    let xyz = mul(&D65_TO_D50, mul(&SRGB_TO_XYZ, linear(c)));
    let f = |t: f64| {
        if t > EPS {
            t.cbrt()
        } else {
            (KAPPA * t + 16.0) / 116.0
        }
    };
    let (fx, fy, fz) = (f(xyz[0] / D50[0]), f(xyz[1] / D50[1]), f(xyz[2] / D50[2]));
    (116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz))
}

/// `OKLab` to sRGB (clamped).
pub(crate) fn from_oklab(l: f64, a: f64, b: f64, alpha: u8) -> Rgba8 {
    from_linear(oklab_to_linear(l, a, b), alpha)
}

/// sRGB to `OKLab`.
pub(crate) fn oklab(c: Rgba8) -> (f64, f64, f64) {
    to_oklab(c)
}

/// Linear display-P3 channels to sRGB (clamped).
pub(crate) fn from_p3_linear(rgb: [f64; 3], alpha: u8) -> Rgba8 {
    from_linear(mul(&P3_TO_SRGB, rgb), alpha)
}

/// Linear display-P3 channels of an sRGB colour.
pub(crate) fn to_p3_linear(c: Rgba8) -> [f64; 3] {
    mul(&SRGB_TO_P3, linear(c))
}

/// The sRGB transfer function applied to one linear channel (also P3's).
pub(crate) fn gamma(x: f64) -> f64 {
    let s = x.signum();
    let x = x.abs();
    s * if x <= 0.003_130_8 {
        x * 12.92
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

/// The inverse of [`gamma`].
pub(crate) fn degamma(x: f64) -> f64 {
    let s = x.signum();
    let x = x.abs();
    s * if x <= 0.040_45 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}
