//! The plans, one file each. A new plan is a file here and one line in [`PLANS`].

pub mod box_creature;

use crate::plan::Plan;

/// Every plan, by name.
pub static PLANS: &[&dyn Plan] = &[
    &box_creature::PLAN,
];
