use symphonia::core::audio::{ChannelLabel, Channels, Position};

const K: f32 = std::f32::consts::FRAC_1_SQRT_2;

pub fn gains(ch: &Channels) -> Vec<(f32, f32)> {
    let pos: Vec<Option<Position>> = match ch {
        Channels::Positioned(p) => p.iter().map(Some).collect(),
        Channels::Custom(list) => list
            .iter()
            .map(|c| match c {
                ChannelLabel::Positioned(p) => Some(*p),
                _ => None,
            })
            .collect(),
        _ => vec![None; ch.count()],
    };
    let mut g: Vec<(f32, f32)> = pos.iter().enumerate().map(|(i, p)| gain(*p, i)).collect();
    let (sl, sr) = g.iter().fold((0.0f32, 0.0f32), |(a, b), (x, y)| (a + x, b + y));
    let k = 1.0 / sl.max(sr).max(1.0);
    for x in &mut g {
        x.0 *= k;
        x.1 *= k;
    }
    g
}

fn gain(p: Option<Position>, i: usize) -> (f32, f32) {
    use Position as P;
    let Some(p) = p else {
        return match i {
            0 => (1.0, 0.0),
            1 => (0.0, 1.0),
            _ => (K, K),
        };
    };
    if p.intersects(P::LFE1 | P::LFE2) {
        (0.0, 0.0)
    } else if p.intersects(P::FRONT_LEFT | P::FRONT_LEFT_CENTER | P::FRONT_LEFT_WIDE) {
        (1.0, 0.0)
    } else if p.intersects(P::FRONT_RIGHT | P::FRONT_RIGHT_CENTER | P::FRONT_RIGHT_WIDE) {
        (0.0, 1.0)
    } else if p.intersects(P::SIDE_LEFT | P::REAR_LEFT | P::TOP_FRONT_LEFT | P::TOP_REAR_LEFT) {
        (K, 0.0)
    } else if p.intersects(P::SIDE_RIGHT | P::REAR_RIGHT | P::TOP_FRONT_RIGHT | P::TOP_REAR_RIGHT) {
        (0.0, K)
    } else {
        (K, K)
    }
}

pub fn downmix(planes: &[Vec<f32>], gains: &[(f32, f32)], l: &mut Vec<f32>, r: &mut Vec<f32>) {
    let n = planes[0].len();
    l.clear();
    l.resize(n, 0.0);
    r.clear();
    r.resize(n, 0.0);
    for (p, &(gl, gr)) in planes.iter().zip(gains) {
        if gl == 0.0 && gr == 0.0 {
            continue;
        }
        for ((a, b), s) in l.iter_mut().zip(r.iter_mut()).zip(p) {
            *a += s * gl;
            *b += s * gr;
        }
    }
}
