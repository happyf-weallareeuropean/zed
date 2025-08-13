use std::path::Path;
use std::sync::Arc;

use gpui::{App, Entity, Global, prelude::*};

use crate::{PijulRepository, RealPijulRepository};

/// Note: We won't ultimately be storing the pijul store in a global, we're just doing this for exploration purposes.
struct GlobalPijulStore(Entity<PijulStore>);

impl Global for GlobalPijulStore {}

pub struct PijulStore {
    repository: Arc<dyn PijulRepository>,
}

impl PijulStore {
    pub fn init_global(cx: &mut App) {
        let Some(repository) = RealPijulRepository::new(Path::new(".")).ok() else {
            return;
        };

        let repository = Arc::new(repository);
        let pijul_store = cx.new(|cx| PijulStore::new(repository, cx));

        cx.set_global(GlobalPijulStore(pijul_store));
    }

    pub fn try_global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalPijulStore>()
            .map(|global| global.0.clone())
    }

    pub fn new(repository: Arc<dyn PijulRepository>, _cx: &mut Context<Self>) -> Self {
        Self { repository }
    }

    pub fn repository(&self) -> &Arc<dyn PijulRepository> {
        &self.repository
    }
}
