// ABOUTME: Command module aggregator for the peleka CLI.
// ABOUTME: Re-exports deploy, rollback, and exec command handlers.

mod deploy;
mod exec;
mod rollback;

pub use deploy::deploy;
pub use exec::exec_command;
pub use rollback::rollback;
