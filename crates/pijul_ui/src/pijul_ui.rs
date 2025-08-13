mod channel_picker;

use gpui::App;
use pijul::PijulStore;
use workspace::Workspace;

pub fn init(cx: &mut App) {
    PijulStore::init_global(cx);

    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        channel_picker::register(workspace);
    })
    .detach();
}
