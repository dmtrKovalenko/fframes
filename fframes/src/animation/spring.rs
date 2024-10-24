#[derive(Clone, Copy, Debug)]
pub struct SpringRuntime {
    pub(crate) m_zeta: f32,
    pub(crate) w0: f32,
    pub(crate) wd: f32,
    pub(crate) a: f32,
    pub(crate) b: f32,
}

impl SpringRuntime {
    pub fn new(mass: f32, stiffness: f32, damping: f32) -> SpringRuntime {
        let m_zeta = damping / (2.0 * libm::sqrtf(stiffness * mass));
        let m_w0 = libm::sqrtf(stiffness / mass);

        if m_zeta < 1.0 {
            let m_wd = m_w0 * libm::sqrtf(1.0 - m_zeta * m_zeta);
            // Under-damped
            SpringRuntime {
                m_zeta,
                w0: m_w0,
                wd: m_wd,
                a: 1.0,
                b: (m_zeta * m_w0) / m_wd,
            }
        } else {
            // Critically damped (ignoring over-damped case for now).
            SpringRuntime {
                m_zeta,
                w0: m_w0,
                wd: 0.0,
                a: 1.0,
                b: m_w0,
            }
        }
    }

    pub fn solve(&self, t: &f32) -> f32 {
        let progress = if self.m_zeta < 1.0 {
            // Under-damped
            libm::expf(-t * self.m_zeta * self.w0)
                * (self.a * libm::cosf(self.wd * t) + self.b * libm::sinf(self.wd * t))
        } else {
            // Critically damped
            (self.a + self.b * t) * libm::expf(-t * self.w0)
        };

        // Map range from [1..0] to [0..1].
        1.0 - progress
    }

    /// The dumbest way to calculate spring duration taken from animejs https://github.com/juliangarnier/anime/blob/master/src/index.js#L100
    /// Not the best, but it looks there is no formula for precise calculation of spring dumping timing.
    /// Must be called in compile time
    pub fn get_duration(&self) -> f32 {
        let frame_duration = 0.166667; // for 60 fps

        let mut elapsed = 0.0;
        let mut not_animating_frames_count = 0u8;

        loop {
            elapsed += frame_duration;
            if self.solve(&elapsed) == 1.0 {
                not_animating_frames_count += 1;

                if not_animating_frames_count >= 128 {
                    break;
                }
            } else {
                not_animating_frames_count = 0
            }
        }

        elapsed * frame_duration
    }
}
