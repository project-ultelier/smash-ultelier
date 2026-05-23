#[cfg(feature = "console")]
pub mod common;
#[cfg(feature = "console")]
pub mod console;

#[cfg(feature = "sync-guest")]
pub use ssbusync_guest as sync_guest;

#[cfg(feature = "console")]
#[link(name = "imgui_smash")]
unsafe extern "C" {}

#[cfg(feature = "console")]
pub fn panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let msg = match info.payload().downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => match info.payload().downcast_ref::<String>() {
                Some(s) => &s[..],
                None => "Box<Any>",
            },
        };
        let err_msg = match info.location() {
            Some(location) => format!("ultelier panicked at '{}', {}", msg, location),
            None => format!("ultelier panicked at '{}' (location unavailable)", msg),
        };
        skyline::error::show_error(
            69,
            "ultelier has panicked. Please open the details and send a screenshot to the developer, then close the game.\n\0",
            err_msg.as_str(),
        );
    }));
}

pub fn install() {
    #[cfg(feature = "console")]
    {
        console::install();
        panic_hook();
    }
    #[cfg(feature = "auto-profile-switcher")]
    sync_guest::runtime::install_auto_profile_switcher();
}

#[cfg(feature = "no-dep")]
#[skyline::main(name = "ultelier")]
fn main() {
    install();
}
