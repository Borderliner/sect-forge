use gpui::{AnyElement, Context, ObjectFit, SharedString, div, prelude::*, px, rgb};

use crate::config::{Variant, WARLORDS, Warlord, WarlordFamily};

use super::{
    Editor, GOLD, LINE, MUTED, PAPER, body, caption, col, colour, control, picture, row_ui,
};

impl Editor {
    pub(super) fn select_variant(&mut self, variant: Variant, cx: &mut Context<Self>) {
        if self.config.variant == variant {
            return;
        }
        self.config.variant = variant;
        if variant == Variant::Warlord && self.config.warlord.is_none() {
            self.config.sect_mode = false;
        }
        if let Some(entry) = self.config.warlord() {
            self.playstyle_family = entry.family;
        }
        self.edited(cx);
    }

    fn select_warlord(&mut self, entry: &Warlord, cx: &mut Context<Self>) {
        self.config.warlord = Some(entry.faction.clone());
        self.edited(cx);
    }

    pub(super) fn warlord_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.config.warlord();
        let family = self.playstyle_family;
        let mut entries = WARLORDS
            .iter()
            .filter(|entry| entry.family == family)
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| &entry.name);
        col()
            .gap_3()
            .child(
                row_ui()
                    .justify_between()
                    .items_center()
                    .gap_2()
                    .child(caption("190 CE playstyles"))
                    .child(
                        caption(
                            selected
                                .map_or("Choose a warlord", |entry| entry.name.as_str())
                                .to_owned(),
                        )
                        .text_color(rgb(GOLD)),
                    ),
            )
            .child(
                row_ui().gap_1().flex_wrap().children(
                    [
                        WarlordFamily::Han,
                        WarlordFamily::Bandit,
                        WarlordFamily::YellowTurban,
                        WarlordFamily::Nanman,
                    ]
                    .map(|category| {
                        control(
                            format!("playstyle-{}", category.label()),
                            family == category,
                        )
                        .child(category.label())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.playstyle_family = category;
                            cx.notify();
                        }))
                        .on_key_down(cx.listener(
                            move |this, event: &gpui::KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                    this.playstyle_family = category;
                                    cx.notify();
                                }
                            },
                        ))
                    }),
                ),
            )
            .child(col().gap_1().children(entries.into_iter().map(|entry| {
                let current = selected.is_some_and(|it| it.faction == entry.faction);
                let path = format!("ui/characters/{0}/stills/mini/{0}.png", entry.portrait);
                control(SharedString::from(entry.faction.clone()), current)
                    .w_full()
                    .min_h(px(48.))
                    .h_auto()
                    .py_1()
                    .px_2()
                    .gap_2()
                    .justify_start()
                    .child(
                        div()
                            .size(px(36.))
                            .flex_shrink_0()
                            .rounded(px(4.))
                            .overflow_hidden()
                            .bg(rgb(colour(&entry.primary, LINE)))
                            .child(picture(self.art(&path), "♙", ObjectFit::Cover)),
                    )
                    .child(
                        col()
                            .flex_1()
                            .min_w_0()
                            .items_start()
                            .child(div().text_color(rgb(PAPER)).child(entry.name.clone()))
                            .child(caption(entry.focus.clone()).text_color(rgb(MUTED)))
                            .when(current, |text| text.child(body(entry.description.clone()))),
                    )
                    .when(current, |row| row.child(caption("✓").text_color(rgb(GOLD))))
                    .on_click(cx.listener(move |this, _, _, cx| this.select_warlord(entry, cx)))
                    .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            this.select_warlord(entry, cx);
                        }
                    }))
            })))
            .into_any_element()
    }
}
