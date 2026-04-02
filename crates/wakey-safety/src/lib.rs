// Every action passes through Cedar policy evaluation before execution.
// Denied actions return feedback; the cortex tries a different approach.
pub mod engine;
pub mod policy;
