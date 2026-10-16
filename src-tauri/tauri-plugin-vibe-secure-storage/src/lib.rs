use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime, State,
};

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;
mod error;
mod models;

#[cfg(desktop)]
use desktop::SecureStorage;
#[cfg(mobile)]
use mobile::SecureStorage;

pub use error::{Error, Result};

pub trait SecureStorageExt<R: Runtime> {
    fn secure_storage(&self) -> State<'_, SecureStorage<R>>;
}

impl<R: Runtime, T: Manager<R>> SecureStorageExt<R> for T {
    fn secure_storage(&self) -> State<'_, SecureStorage<R>> {
        self.state::<SecureStorage<R>>()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("vibe-secure-storage")
        .setup(|app, api| {
            #[cfg(mobile)]
            let storage = mobile::init(app, api)?;
            #[cfg(desktop)]
            let storage = desktop::init(app, api)?;
            app.manage(storage);
            Ok(())
        })
        .build()
}
