//! The floating-point environment guard (docs/design/DETERMINISM.md, rule 6).
//!
//! The CPU's floating-point control register decides how every `f32`/`f64` operation rounds and
//! whether tiny values flush to zero. Rust (LLVM) assumes the default: round to nearest, ties to
//! even, subnormals kept, every exception masked. Any native code in the process can change it
//! for its thread: a driver, an overlay DLL, a game module that wants faster maths. The
//! simulation would then compute different bits on this run than on another, which is a desync
//! nobody can see in the code. So the engine checks the register where foreign code may have run
//! (at startup, as each tick begins, around each module call) and puts it back.
//!
//! | Target | Register | Control bits checked | Default |
//! |---|---|---|---|
//! | x86-64 (and x86 with SSE) | MXCSR | rounding (bits 13-14), FTZ (15), DAZ (6), exception masks (7-12) | `0x1f80` |
//! | AArch64 | FPCR | RMode (22-23), FZ (24), DN (25), AHP (26), FZ16 (19), trap enables (8-12, 15), and FEAT_AFP's FIZ (0), AH (1), NEP (2) | `0` |
//! | anything else | none | nothing: [`SUPPORTED`] is `false` and every check reports nothing | |
//!
//! Status flags (MXCSR bits 0-5: an inexact result happened, and so on) are sticky records,
//! not modes, and are ignored. The x87 control word is not checked: Rust does `f32`/`f64` maths in
//! SSE registers on these targets, so x87 precision can't change simulation results.
//!
//! Reading and restoring are plain register moves (a few cycles), cheap enough for every tick
//! and every module call.

/// Whether this target has a register the guard checks.
pub const SUPPORTED: bool = imp::SUPPORTED;
/// The register's name (`mxcsr`, `fpcr`, or `none`).
pub const REGISTER: &str = imp::REGISTER;
/// The control bits checked (status flags excluded).
pub const CONTROL_MASK: u64 = imp::CONTROL_MASK;
/// The control bits Rust code assumes.
pub const EXPECTED: u64 = imp::EXPECTED;

/// The register's whole value (`None` where [`SUPPORTED`] is false).
pub fn read() -> Option<u64> {
    imp::read()
}

/// The register's control bits (`read() & CONTROL_MASK`).
pub fn control() -> Option<u64> {
    read().map(|r| r & CONTROL_MASK)
}

/// Checks the control bits and, if anything changed them, restores the default (keeping the
/// status flags as they are). Returns what was found, or `None` when all was well (always `None`
/// where the target has no supported register).
pub fn check_and_restore() -> Option<Drift> {
    let raw = imp::read()?;
    if raw & CONTROL_MASK == EXPECTED {
        return None;
    }
    // SAFETY: the value written is the default environment Rust code is compiled for, with this
    // thread's own status flags kept.
    unsafe { imp::write((raw & !CONTROL_MASK) | EXPECTED) };
    Some(Drift { found: raw & CONTROL_MASK })
}

/// Sets the register's control bits to `bits` (masked by [`CONTROL_MASK`]); a no-op where the
/// target has no supported register. For tests that play a misbehaving library.
///
/// # Safety
///
/// Rust code is compiled assuming the default environment: until [`check_and_restore`] puts it
/// back, float operations on this thread may round differently from what the compiler assumed
/// when it folded constants, and unmasked exceptions trap (the process ends). Call no float code
/// you rely on in between.
pub unsafe fn set_control(bits: u64) {
    if let Some(raw) = imp::read() {
        // SAFETY: the caller takes responsibility (see above).
        unsafe { imp::write((raw & !CONTROL_MASK) | (bits & CONTROL_MASK)) };
    }
}

/// Control bits for a rounding mode, for [`set_control`] (`None` where unsupported).
pub fn rounding_bits(mode: Rounding) -> Option<u64> {
    imp::SUPPORTED.then(|| (EXPECTED & !imp::RC_MASK) | (imp::rc_bits(mode) << imp::RC_SHIFT))
}

/// IEEE 754 rounding directions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rounding {
    Nearest,
    Down,
    Up,
    TowardZero,
}

impl Rounding {
    pub fn name(self) -> &'static str {
        match self {
            Rounding::Nearest => "nearest",
            Rounding::Down => "down",
            Rounding::Up => "up",
            Rounding::TowardZero => "toward_zero",
        }
    }
}

/// Control bits found different from the default (see [`check_and_restore`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drift {
    /// The control bits found ([`CONTROL_MASK`] applied).
    pub found: u64,
}

impl Drift {
    pub fn expected(&self) -> u64 {
        EXPECTED
    }

    /// The rounding mode found.
    pub fn rounding(&self) -> Rounding {
        imp::rounding_of(self.found)
    }

    /// Results too small for a normal float became zero (x86 FTZ, ARM FZ).
    pub fn flush_to_zero(&self) -> bool {
        imp::flush_to_zero(self.found)
    }

    /// Subnormal inputs were read as zero (x86 DAZ; ARM FEAT_AFP FIZ).
    pub fn denormals_are_zero(&self) -> bool {
        imp::denormals_are_zero(self.found)
    }

    /// Some float exception would trap instead of giving NaN or infinity.
    pub fn exceptions_unmasked(&self) -> bool {
        imp::exceptions_unmasked(self.found)
    }

    /// One line for logs: `mxcsr 0x7f80 (expected 0x1f80): rounding toward_zero`.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.rounding() != Rounding::Nearest {
            parts.push(format!("rounding {}", self.rounding().name()));
        }
        if self.flush_to_zero() {
            parts.push("flush-to-zero on".to_owned());
        }
        if self.denormals_are_zero() {
            parts.push("denormals-are-zero on".to_owned());
        }
        if self.exceptions_unmasked() {
            parts.push("float exceptions unmasked".to_owned());
        }
        if parts.is_empty() {
            parts.push("other control bits".to_owned());
        }
        format!("{REGISTER} {:#x} (expected {EXPECTED:#x}): {}", self.found, parts.join(", "))
    }
}

#[cfg(any(target_arch = "x86_64", all(target_arch = "x86", target_feature = "sse")))]
mod imp {
    use super::Rounding;

    pub const SUPPORTED: bool = true;
    pub const REGISTER: &str = "mxcsr";
    /// DAZ (6), exception masks (7-12), rounding (13-14), FTZ (15).
    pub const CONTROL_MASK: u64 = 0xffc0;
    pub const EXPECTED: u64 = 0x1f80;
    pub const RC_SHIFT: u32 = 13;
    pub const RC_MASK: u64 = 0b11 << RC_SHIFT;

    pub fn rc_bits(mode: Rounding) -> u64 {
        match mode {
            Rounding::Nearest => 0,
            Rounding::Down => 1,
            Rounding::Up => 2,
            Rounding::TowardZero => 3,
        }
    }

    pub fn rounding_of(bits: u64) -> Rounding {
        match (bits & RC_MASK) >> RC_SHIFT {
            0 => Rounding::Nearest,
            1 => Rounding::Down,
            2 => Rounding::Up,
            _ => Rounding::TowardZero,
        }
    }

    pub fn flush_to_zero(bits: u64) -> bool {
        bits & (1 << 15) != 0
    }

    pub fn denormals_are_zero(bits: u64) -> bool {
        bits & (1 << 6) != 0
    }

    pub fn exceptions_unmasked(bits: u64) -> bool {
        bits & 0x1f80 != 0x1f80
    }

    pub fn read() -> Option<u64> {
        let mut csr: u32 = 0;
        // SAFETY: `stmxcsr` stores the 32-bit MXCSR to the given address, which is our own
        // local; it changes no registers or flags. SSE is part of the target (x86-64 always,
        // x86 only with the `sse` feature, see the cfg).
        unsafe { core::arch::asm!("stmxcsr [{}]", in(reg) &mut csr, options(nostack, preserves_flags)) };
        Some(u64::from(csr))
    }

    /// # Safety
    ///
    /// `value` becomes this thread's MXCSR: see [`super::set_control`].
    pub unsafe fn write(value: u64) {
        let csr = value as u32;
        // SAFETY: `ldmxcsr` loads MXCSR from our local; reserved bits are never set (callers keep
        // the read value's upper bits, which are zero, and mask the rest). The caller vouches for
        // the mode itself.
        unsafe { core::arch::asm!("ldmxcsr [{}]", in(reg) &csr, options(nostack, preserves_flags, readonly)) };
    }
}

#[cfg(target_arch = "aarch64")]
mod imp {
    use super::Rounding;

    pub const SUPPORTED: bool = true;
    pub const REGISTER: &str = "fpcr";
    /// FEAT_AFP's FIZ (0, flush inputs to zero), AH (1, alternate handling) and NEP (2, scalar
    /// results keep the upper vector lanes), trap enables (8-12, 15), FZ16 (19), RMode (22-23),
    /// FZ (24), DN (25), AHP (26). (The AFP bits read as zero on cores without FEAT_AFP.)
    pub const CONTROL_MASK: u64 = 0x07c8_9f07;
    pub const EXPECTED: u64 = 0;
    pub const RC_SHIFT: u32 = 22;
    pub const RC_MASK: u64 = 0b11 << RC_SHIFT;

    pub fn rc_bits(mode: Rounding) -> u64 {
        match mode {
            Rounding::Nearest => 0,
            Rounding::Up => 1,
            Rounding::Down => 2,
            Rounding::TowardZero => 3,
        }
    }

    pub fn rounding_of(bits: u64) -> Rounding {
        match (bits & RC_MASK) >> RC_SHIFT {
            0 => Rounding::Nearest,
            1 => Rounding::Up,
            2 => Rounding::Down,
            _ => Rounding::TowardZero,
        }
    }

    pub fn flush_to_zero(bits: u64) -> bool {
        bits & (1 << 24) != 0
    }

    /// FEAT_AFP's FIZ: subnormal inputs read as zero (like x86 DAZ).
    pub fn denormals_are_zero(bits: u64) -> bool {
        bits & 1 != 0
    }

    pub fn exceptions_unmasked(bits: u64) -> bool {
        bits & 0x9f00 != 0
    }

    pub fn read() -> Option<u64> {
        let fpcr: u64;
        // SAFETY: `mrs` reads FPCR into a general register; no memory, stack or flags involved.
        unsafe { core::arch::asm!("mrs {}, fpcr", out(reg) fpcr, options(nomem, nostack, preserves_flags)) };
        Some(fpcr)
    }

    /// # Safety
    ///
    /// `value` becomes this thread's FPCR: see [`super::set_control`].
    pub unsafe fn write(value: u64) {
        // SAFETY: `msr` writes FPCR from a general register; reserved bits keep the values just
        // read. The caller vouches for the mode itself.
        unsafe { core::arch::asm!("msr fpcr, {}", in(reg) value, options(nomem, nostack, preserves_flags)) };
    }
}

/// Targets without a supported register: the guard checks nothing (and says so through
/// [`SUPPORTED`]). Adding one is a `read`/`write` pair and the bit layout.
#[cfg(not(any(target_arch = "x86_64", all(target_arch = "x86", target_feature = "sse"), target_arch = "aarch64")))]
mod imp {
    use super::Rounding;

    pub const SUPPORTED: bool = false;
    pub const REGISTER: &str = "none";
    pub const CONTROL_MASK: u64 = 0;
    pub const EXPECTED: u64 = 0;
    pub const RC_SHIFT: u32 = 0;
    pub const RC_MASK: u64 = 0;

    pub fn rc_bits(_mode: Rounding) -> u64 {
        0
    }

    pub fn rounding_of(_bits: u64) -> Rounding {
        Rounding::Nearest
    }

    pub fn flush_to_zero(_bits: u64) -> bool {
        false
    }

    pub fn denormals_are_zero(_bits: u64) -> bool {
        false
    }

    pub fn exceptions_unmasked(_bits: u64) -> bool {
        false
    }

    pub fn read() -> Option<u64> {
        None
    }

    /// # Safety
    ///
    /// Nothing to write on this target.
    pub unsafe fn write(_value: u64) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `f` on a fresh thread, so a test's register change can't reach other tests.
    fn on_own_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        std::thread::spawn(f).join().expect("test thread")
    }

    #[test]
    fn the_default_environment_needs_nothing() {
        on_own_thread(|| {
            if !SUPPORTED {
                eprintln!("skipped: no floating-point control register is checked on this target");
                return;
            }
            assert_eq!(control(), Some(EXPECTED), "threads start in the default environment");
            assert_eq!(check_and_restore(), None);
        });
    }

    #[test]
    fn a_changed_rounding_mode_is_found_named_and_restored() {
        on_own_thread(|| {
            if !SUPPORTED {
                eprintln!("skipped: no floating-point control register is checked on this target");
                return;
            }
            for mode in [Rounding::Down, Rounding::Up, Rounding::TowardZero] {
                let bits = rounding_bits(mode).expect("supported");
                // SAFETY: no float code runs before the restore below.
                unsafe { set_control(bits) };
                assert_eq!(control(), Some(bits));
                let d = check_and_restore().expect("drift found");
                assert_eq!(d.rounding(), mode);
                assert!(!d.flush_to_zero() && !d.denormals_are_zero() && !d.exceptions_unmasked());
                assert!(d.describe().contains(mode.name()), "{}", d.describe());
                assert_eq!(control(), Some(EXPECTED), "restored");
            }
        });
    }

    #[test]
    fn rounding_really_changes_results_until_restored() {
        on_own_thread(|| {
            if !SUPPORTED {
                eprintln!("skipped: no floating-point control register is checked on this target");
                return;
            }
            let (a, b) = (std::hint::black_box(1.0f32), std::hint::black_box(3.0f32));
            let nearest = std::hint::black_box(a / b);
            // SAFETY: the only float code until the restore is the division under test.
            unsafe { set_control(rounding_bits(Rounding::TowardZero).expect("supported")) };
            let chopped = std::hint::black_box(std::hint::black_box(a) / std::hint::black_box(b));
            let drift = check_and_restore();
            assert!(drift.is_some());
            // 1/3 to nearest is 0x3eaaaaab (rounded up); toward zero it is one ulp less.
            assert_eq!(nearest.to_bits() - 1, chopped.to_bits(), "1/3 toward zero is one ulp below nearest");
            assert_eq!((a / b).to_bits(), nearest.to_bits(), "and back to nearest after the restore");
        });
    }

    #[cfg(any(target_arch = "x86_64", all(target_arch = "x86", target_feature = "sse")))]
    #[test]
    fn flush_to_zero_and_daz_are_named_and_status_flags_are_ignored() {
        on_own_thread(|| {
            // SAFETY: no float code runs before the restore.
            unsafe { set_control(EXPECTED | (1 << 15) | (1 << 6)) };
            let d = check_and_restore().expect("drift found");
            assert!(d.flush_to_zero() && d.denormals_are_zero());
            assert_eq!(d.rounding(), Rounding::Nearest);
            // A sticky status flag (inexact, bit 5) is not a change of mode.
            let raw = read().expect("supported");
            // SAFETY: writes the default control bits with one status flag set.
            unsafe { imp::write(raw | (1 << 5)) };
            assert_eq!(check_and_restore(), None);
        });
    }
}
