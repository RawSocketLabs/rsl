#[repr(C)]
#[derive(Clone, Copy, deps::bytemuck::Pod, deps::bytemuck::Zeroable)]
#[bytemuck(crate = "deps::bytemuck")]
struct Pair {
    first: i16,
    second: i16,
}

#[test]
fn facade_qualified_derives_enable_slice_casting() {
    let mut pairs = [Pair {
        first: 0x0102,
        second: 0x0304,
    }];

    let scalars: &mut [i16] = deps::bytemuck::cast_slice_mut(&mut pairs);

    assert_eq!(scalars, &[0x0102, 0x0304]);
}

#[cfg(feature = "num")]
#[test]
fn pod_feature_marks_complex_i16_as_plain_data() {
    fn assert_pod<T: deps::bytemuck::Pod>() {}

    assert_pod::<deps::num_complex::Complex<i16>>();
}
