use std::sync::Arc;

use fuzzy::{StringMatchCandidate, match_strings};
use gpui::{
    App, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, Task, WeakEntity, Window,
    prelude::*,
};
use picker::{Picker, PickerDelegate};
use pijul::{Channel, PijulStore};
use ui::{HighlightedLabel, ListItem, ListItemSpacing, prelude::*};
use util::ResultExt as _;
use workspace::{ModalView, Workspace};

pub fn register(workspace: &mut Workspace) {
    workspace.register_action(open);
}

fn open(
    workspace: &mut Workspace,
    _: &zed_actions::pijul::ChannelList,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(pijul_store) = PijulStore::try_global(cx) else {
        return;
    };

    workspace.toggle_modal(window, cx, |window, cx| {
        let delegate = ChannelPickerDelegate::new(cx.entity().downgrade(), pijul_store, cx);
        ChannelPicker::new(delegate, window, cx)
    });
}

pub struct ChannelPicker {
    picker: Entity<Picker<ChannelPickerDelegate>>,
}

impl ChannelPicker {
    pub fn new(
        delegate: ChannelPickerDelegate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx));
        Self { picker }
    }
}

impl ModalView for ChannelPicker {}

impl EventEmitter<DismissEvent> for ChannelPicker {}

impl Focusable for ChannelPicker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for ChannelPicker {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().w(rems(34.)).child(self.picker.clone())
    }
}

#[derive(Debug, Clone)]
struct ChannelEntry {
    channel: Channel,
    positions: Vec<usize>,
}

pub struct ChannelPickerDelegate {
    picker: WeakEntity<ChannelPicker>,
    matches: Vec<ChannelEntry>,
    all_channels: Vec<Channel>,
    selected_index: usize,
}

impl ChannelPickerDelegate {
    fn new(
        picker: WeakEntity<ChannelPicker>,
        pijul_store: Entity<PijulStore>,
        cx: &mut Context<ChannelPicker>,
    ) -> Self {
        let channels = pijul_store.read(cx).repository().list_channels();

        Self {
            picker,
            matches: Vec::new(),
            all_channels: channels,
            selected_index: 0,
        }
    }
}

impl PickerDelegate for ChannelPickerDelegate {
    type ListItem = ListItem;

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> Arc<str> {
        "Select Channel…".into()
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(
        &mut self,
        ix: usize,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) {
        self.selected_index = ix;
    }

    fn update_matches(
        &mut self,
        query: String,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let background = cx.background_executor().clone();
        let all_channels = self.all_channels.clone();

        cx.spawn_in(window, async move |this, cx| {
            let matches = if query.is_empty() {
                all_channels
                    .into_iter()
                    .map(|channel| ChannelEntry {
                        channel,
                        positions: Vec::new(),
                    })
                    .collect()
            } else {
                let candidates = all_channels
                    .iter()
                    .enumerate()
                    .map(|(ix, channel)| StringMatchCandidate::new(ix, &channel.name))
                    .collect::<Vec<_>>();
                match_strings(
                    &candidates,
                    &query,
                    false,
                    true,
                    100,
                    &Default::default(),
                    background,
                )
                .await
                .into_iter()
                .map(|mat| ChannelEntry {
                    channel: all_channels[mat.candidate_id].clone(),
                    positions: mat.positions,
                })
                .collect()
            };

            this.update(cx, |this, _cx| {
                this.delegate.matches = matches;
            })
            .log_err();
        })
    }

    fn confirm(&mut self, _secondary: bool, _window: &mut Window, _cx: &mut Context<Picker<Self>>) {
        //
    }

    fn dismissed(&mut self, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
        self.picker
            .update(cx, |_, cx| cx.emit(DismissEvent))
            .log_err();
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let entry = &self.matches[ix];

        Some(
            ListItem::new(ix)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(selected)
                .child(HighlightedLabel::new(
                    entry.channel.name.clone(),
                    entry.positions.clone(),
                )),
        )
    }
}
