/// Align `addr` downwards to the nearest multiple of `align`.
///
/// The returned usize is always <= `addr.`
///
/// # Panics
///
/// Panics if `align` is not a power of 2.
pub fn align_down(addr: usize, align: usize) -> usize {
  if !align.is_power_of_two() {
    panic!("align_down: alignment must be a power of 2")
  }
  let units = addr / align;
  units * align
}

/// Align `addr` upwards to the nearest multiple of `align`.
///
/// The returned `usize` is always >= `addr.`
///
/// # Panics
///
/// Panics if `align` is not a power of 2
/// or aligning up overflows the address.
pub fn align_up(addr: usize, align: usize) -> usize {
  if !align.is_power_of_two() {
    panic!("align_down: alignment must be a power of 2")
  }
  let leftover = addr % align;
  if leftover == 0 {
    addr
  } else {
    addr.checked_add(align - leftover).unwrap()
  }
}
