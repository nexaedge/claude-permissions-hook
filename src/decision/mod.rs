use crate::protocol::{HookInput, HookOutput};

/// Evaluate a hook input and return a permission decision.
///
/// Currently maps permission modes to decisions:
/// - `bypassPermissions` → allow
/// - `dontAsk` → deny
/// - all others → ask
pub fn evaluate(_input: &HookInput) -> HookOutput {
    todo!("Implement permission mode evaluation in step 03")
}
