use std::time::Duration;

/// Nanoseconds on the clock used by output deadlines and output events: a monotonic clock that
/// keeps counting while the device sleeps (Apple `mach_continuous_time`, Android / Linux
/// `CLOCK_BOOTTIME`, the same clock as `SystemClock.elapsedRealtimeNanos`). Elsewhere it counts
/// from the first call in the process. `None` when the clock cannot be read.
pub fn output_clock_ns() -> Option<u64> {
    u64::try_from(read()?.as_nanos()).ok()
}

#[cfg(target_vendor = "apple")]
fn read() -> Option<Duration> {
    #[repr(C)]
    struct MachTimebaseInfo {
        numer: u32,
        denom: u32,
    }
    unsafe extern "C" {
        fn mach_continuous_time() -> u64;
        fn mach_timebase_info(info: *mut MachTimebaseInfo) -> i32;
    }
    static TIMEBASE: std::sync::OnceLock<Option<(u32, u32)>> = std::sync::OnceLock::new();
    let (numer, denom) = (*TIMEBASE.get_or_init(|| {
        let mut info = MachTimebaseInfo { numer: 0, denom: 0 };
        // SAFETY: `info` has the layout of `mach_timebase_info_data_t` and is writable.
        let result = unsafe { mach_timebase_info(&mut info) };
        (result == 0 && info.numer != 0 && info.denom != 0).then_some((info.numer, info.denom))
    }))?;
    // SAFETY: reads the OS clock without arguments.
    let ticks = unsafe { mach_continuous_time() };
    let nanos = u128::from(ticks) * u128::from(numer) / u128::from(denom);
    Some(Duration::from_nanos(u64::try_from(nanos).ok()?))
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn read() -> Option<Duration> {
    use std::os::raw::{c_int, c_long};
    #[repr(C)]
    struct Timespec {
        tv_sec: c_long,
        tv_nsec: c_long,
    }
    const CLOCK_BOOTTIME: c_int = 7;
    unsafe extern "C" {
        fn clock_gettime(clock: c_int, value: *mut Timespec) -> c_int;
    }
    let mut value = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `value` points to a writable `timespec`.
    if unsafe { clock_gettime(CLOCK_BOOTTIME, &mut value) } != 0 {
        return None;
    }
    let seconds = u64::try_from(value.tv_sec).ok()?;
    let nanos = u32::try_from(value.tv_nsec).ok()?;
    (nanos < 1_000_000_000).then(|| Duration::new(seconds, nanos))
}

#[cfg(not(any(target_vendor = "apple", target_os = "android", target_os = "linux")))]
fn read() -> Option<Duration> {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    Some(START.get_or_init(std::time::Instant::now).elapsed())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_clock_does_not_go_back() {
        let before = super::output_clock_ns().unwrap();
        let after = super::output_clock_ns().unwrap();
        assert!(before <= after);
    }
}
