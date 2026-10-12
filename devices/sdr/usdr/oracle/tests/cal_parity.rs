//! rsl-usdr's calibration mathematics against libusdr's, bit for bit and call for call.

use num_complex::Complex32;
use rsl_usdr::cal::fit;
use rsl_usdr::cal::search::{self, Axis, Limits, Objective2d, Search, Stage};
use rsl_usdr_oracle::cal::{self as c, CFloat};

/// A seeded xorshift generator: reproducible inputs without a dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A float in [-1, 1).
    fn unit(&mut self) -> f32 {
        let bits = u32::try_from(self.next() >> 40).unwrap();
        f32::from_bits(0x3f80_0000 | (bits >> 1)) * 2.0 - 3.0
    }
}

fn to_c(samples: &[Complex32]) -> Vec<CFloat> {
    samples
        .iter()
        .map(|s| CFloat { i: s.re, q: s.im })
        .collect()
}

fn to_rust(samples: &[CFloat]) -> Vec<Complex32> {
    samples.iter().map(|s| Complex32::new(s.i, s.q)).collect()
}

/// Bitwise equality, so NaNs and signed zeros count.
fn same(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits()
}

fn assert_samples_eq(rust: &[Complex32], c: &[CFloat], what: &str) {
    assert_eq!(rust.len(), c.len(), "{what}: length");
    for (n, (r, c)) in rust.iter().zip(c).enumerate() {
        assert!(
            same(r.re, c.i) && same(r.im, c.q),
            "{what}: sample {n}: rsl-usdr {r:?}, libusdr {c:?}"
        );
    }
}

/// A noisy tone: what a calibration capture looks like.
fn noisy_tone(rng: &mut Rng, len: usize, amp: f32, period: f32, phase: f32) -> Vec<Complex32> {
    let mut tone = vec![Complex32::new(0.0, 0.0); len];
    fit::generate(&mut tone, amp, period, phase);
    for sample in &mut tone {
        sample.re += rng.unit() * 0.05;
        sample.im += rng.unit() * 0.05;
    }
    tone
}

#[test]
fn twelve_bit_dumps_unpack_identically() {
    let mut rng = Rng(0x1234_5678_9abc_def0);
    let mut words: Vec<u32> = (0..512)
        .map(|_| u32::try_from(rng.next() >> 32).unwrap())
        .collect();
    words.extend([0, 0x7ff, 0x800, 0xfff, 0x7ff_800, 0xffff_ffff]);
    let mut rust = vec![Complex32::new(0.0, 0.0); words.len()];
    fit::from_dump12(&words, &mut rust);
    assert_samples_eq(&rust, &c::dump12(&words), "dump12");
}

#[test]
fn tones_generate_identically() {
    for (amp, period, phase) in [
        (1.0, 32.0, 0.0),
        (0.5, 7.3, 1.25),
        (0.999, 384.0, -3.0),
        (0.25, 8.0, 6.2),
    ] {
        let mut rust = vec![Complex32::new(0.0, 0.0); 1000];
        fit::generate(&mut rust, amp, period, phase);
        assert_samples_eq(
            &rust,
            &c::generate(1000, amp, period, phase),
            &format!("tone {amp} {period} {phase}"),
        );
    }
}

#[test]
fn statistics_and_dc_removal_match_including_empty_input() {
    let mut rng = Rng(42);
    for len in [0, 1, 2, 255, 4096] {
        let samples: Vec<Complex32> = (0..len)
            .map(|_| Complex32::new(rng.unit() + 0.1, rng.unit() - 0.2))
            .collect();
        let (mean, std) = fit::magnitude_mean_std(&samples);
        let (c_mean, c_std) = c::mean_std(&to_c(&samples));
        assert!(same(mean, c_mean) && same(std, c_std), "mean/std at {len}");

        let mut rust = samples.clone();
        let mut c_samples = to_c(&samples);
        let dc = fit::remove_dc(&mut rust);
        let c_dc = c::remove_dc(&mut c_samples);
        assert!(same(dc.re, c_dc.i) && same(dc.im, c_dc.q), "dc at {len}");
        assert_samples_eq(&rust, &c_samples, &format!("centred at {len}"));
    }
}

#[test]
fn fits_and_errors_match() {
    let mut rng = Rng(7);
    for (amp, period, phase) in [(0.5, 32.0, 1.0), (0.9, 100.0, 4.0), (0.3, 12.5, 0.2)] {
        let samples = noisy_tone(&mut rng, 1024, amp, period, phase);
        let c_samples = to_c(&samples);

        let rust = fit::best_phase(&samples, amp, period);
        let (mse, ph) = c::best_fit(&c_samples, amp, period);
        assert!(
            same(rust.error, mse) && same(rust.phase, ph),
            "best fit {amp} {period} {phase}: rsl-usdr {rust:?}, libusdr ({mse}, {ph})"
        );

        let rust = fit::best_phase_and_period(&samples, amp);
        let (mse, ph, per) = c::best_fit_phase_period(&c_samples, amp);
        assert!(
            same(rust.error, mse) && same(rust.phase, ph) && same(rust.period, per),
            "phase/period fit {amp}: rsl-usdr {rust:?}, libusdr ({mse}, {ph}, {per})"
        );

        for trial in [phase, phase + 0.5, 0.0] {
            let rust = fit::fit_error(&samples, amp, period, trial);
            let c = c::fit_error(&c_samples, amp, period, trial);
            assert!(
                same(rust, c),
                "fit error {trial}: rsl-usdr {rust}, libusdr {c}"
            );
        }

        let rust = fit::max_component_error(&samples, amp, period, phase);
        assert!(same(rust, c::max_amp_error(&c_samples, amp, period, phase)));
    }
    assert_samples_eq(&to_rust(&c::dump12(&[])), &[], "empty");
}

/// A deterministic, bumpy cost table indexed by parameter.
fn table(seed: u64, len: usize) -> Vec<i32> {
    let mut rng = Rng(seed);
    (0..len)
        .map(|i| {
            let bowl = (i32::try_from(i).unwrap() - 300).pow(2);
            bowl + i32::try_from(rng.next() % 2000).unwrap()
        })
        .collect()
}

#[test]
fn one_dimensional_searches_probe_and_answer_identically() {
    for seed in 1..20 {
        let costs = table(seed, 1024);
        let cost = |x: i32| costs[usize::try_from(x).unwrap()];
        for (start, stop) in [(0, 1023), (100, 900), (5, 6), (0, 0), (700, 200)] {
            let (mut rust_calls, mut c_calls) = (Vec::new(), Vec::new());
            let rust = search::golden_min::<()>(start, stop, |x| {
                rust_calls.push(x);
                Ok(cost(x))
            })
            .unwrap();
            let c = c::golden_min(start, stop, |x| {
                c_calls.push(x);
                cost(x)
            });
            assert_eq!((rust.x, rust.cost), c, "golden {seed} {start}..{stop}");
            assert_eq!(rust_calls, c_calls, "golden probes {seed} {start}..{stop}");

            for extra in [0, 2] {
                let (mut rust_calls, mut c_calls) = (Vec::new(), Vec::new());
                let rust = search::iterate_min::<()>(start, stop, extra, |x| {
                    rust_calls.push(x);
                    Ok(cost(x))
                })
                .unwrap();
                let c = c::iterate_min(start, stop, extra, |x| {
                    c_calls.push(x);
                    cost(x)
                });
                assert_eq!(
                    (rust.x, rust.cost),
                    c,
                    "iterate {seed} {start}..{stop}+{extra}"
                );
                assert_eq!(rust_calls, c_calls, "iterate probes {seed}");
            }
        }
    }
}

/// Records a two-dimensional objective's calls: (axis, value, evaluated).
struct Recorder<'a> {
    calls: Vec<(i32, i32, bool)>,
    cost: &'a dyn Fn(i32, i32) -> i32,
    at: [i32; 2],
}

impl Objective2d for Recorder<'_> {
    type Error = ();

    fn set(&mut self, axis: Axis, value: i32) -> Result<(), ()> {
        let index = usize::from(axis == Axis::Y);
        self.at[index] = value;
        self.calls.push((i32::from(axis == Axis::Y), value, false));
        Ok(())
    }

    fn evaluate(&mut self, axis: Axis, value: i32) -> Result<i32, ()> {
        let index = usize::from(axis == Axis::Y);
        self.at[index] = value;
        self.calls.push((i32::from(axis == Axis::Y), value, true));
        Ok((self.cost)(self.at[0], self.at[1]))
    }
}

#[test]
fn two_dimensional_searches_visit_and_answer_identically() {
    let cost = |x: i32, y: i32| (x - 40).pow(2) + 3 * (y - 70).pow(2) + (x * y) % 7;
    for (golden, extra, stop_when, stages) in [
        (true, 0, 0, 2),
        (false, 0, 0, 2),
        (false, 1, 50, 3),
        (true, 0, i32::MAX, 3),
    ] {
        let stage = Stage {
            search: if golden {
                Search::Golden
            } else {
                Search::Iterate
            },
            limits: [Limits { min: 0, max: 100 }, Limits { min: -10, max: 150 }],
            extra_step: extra,
        };
        let stages_rust = vec![stage; stages];
        let mut recorder = Recorder {
            calls: Vec::new(),
            cost: &cost,
            at: [0, 0],
        };
        let rust = search::best_2d(&stages_rust, stop_when, &mut recorder).unwrap();

        let stages_c = vec![
            c::Stage {
                golden,
                limits: [(0, 100), (-10, 150)],
                extra,
            };
            stages
        ];
        let mut c_calls = Vec::new();
        let mut at = [0, 0];
        let c = c::best_2d(&stages_c, stop_when, |axis, value, evaluate| {
            at[usize::try_from(axis).unwrap()] = value;
            c_calls.push((axis, value, evaluate));
            cost(at[0], at[1])
        });
        assert_eq!((rust.x, rust.y, rust.cost), c, "best 2d {golden} {extra}");
        assert_eq!(recorder.calls, c_calls, "visits {golden} {extra}");
    }
}

#[test]
fn gcds_match() {
    let mut rng = Rng(99);
    for _ in 0..1000 {
        let (a, b) = (rng.next() >> 20, rng.next() >> 30);
        assert_eq!(search::gcd(a, b), c::gcd(a, b), "gcd({a}, {b})");
    }
    for (a, b) in [
        (0, 0),
        (0, 5),
        (5, 0),
        (1 << 40, 1 << 20),
        (26_000_000, 20_000_000),
    ] {
        assert_eq!(search::gcd(a, b), c::gcd(a, b), "gcd({a}, {b})");
    }
}
