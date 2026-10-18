//! Test modules that misbehave on purpose, linked in through [`crate::ModuleCode::of`]: the
//! engine's tests use them to check its guards. Not for games.

use specula_sdk::{Cx, Module, ModuleInfo, SdkError, Value};
use specula_sys::fpenv::{self, Rounding};

/// Changes the floating-point rounding mode to toward-zero in every tick, makes a request with
/// it changed, and returns with it changed again, as a module doing "fast maths" would. Its
/// `fp_probe` hook answers the bits of `1/3` it computes itself (`0x3eaaaaab` to nearest), then
/// leaves the mode changed too.
#[derive(Default)]
pub struct FlipRounding;

fn flip() {
    if let Some(bits) = fpenv::rounding_bits(Rounding::TowardZero) {
        // SAFETY: deliberately misbehaving test module; the engine restores the environment at the
        // next request or when the call returns, which is what the tests check.
        unsafe { fpenv::set_control(bits) };
    }
}

impl Module for FlipRounding {
    fn describe(&self) -> ModuleInfo {
        ModuleInfo {
            name: "fliprounding",
            version: "1",
            description: "test module: changes the rounding mode",
            hooks: vec![specula_sdk::HookDef { name: "fp_probe", doc: "1/3 in this module's environment", args: "" }],
            ..ModuleInfo::default()
        }
    }

    fn tick(&self, cx: &mut Cx) -> Result<(), SdkError> {
        flip();
        // The engine answers this request in the default environment.
        cx.set_state("ticks", cx.tick())?;
        flip();
        Ok(())
    }

    fn hook(&self, _cx: &mut Cx, _name: &str, _args: &Value) -> Result<Value, SdkError> {
        let third = std::hint::black_box(std::hint::black_box(1.0f32) / std::hint::black_box(3.0f32));
        flip();
        Ok(Value::Int(i64::from(third.to_bits())))
    }
}
