use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime, State,
};

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;
mod error;

#[cfg(desktop)]
use desktop::VibeShare;
#[cfg(mobile)]
use mobile::VibeShare;

pub use error::{Error, Result};

pub trait VibeShareExt<R: Runtime> {
    fn vibe_share(&self) -> State<'_, VibeShare<R>>;
}

impl<R: Runtime, T: Manager<R>> VibeShareExt<R> for T {
    fn vibe_share(&self) -> State<'_, VibeShare<R>> {
        self.state::<VibeShare<R>>()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("vibe-share")
        .setup(|app, api| {
            #[cfg(mobile)]
            let share = mobile::init(app, api)?;
            #[cfg(desktop)]
            let share = desktop::init(app, api)?;
            app.manage(share);
            Ok(())
        })
        .build()
}
