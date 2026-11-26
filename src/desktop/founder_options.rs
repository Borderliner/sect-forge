use super::{
    Attribute, Button, CINNABAR, CLASSES, Career, Catalog, CatalogView, Context, Div, Editor,
    Entity, GOLD, Gender, Input, InputEvent, InputState, LINE, PAPER, SharedString, Stateful,
    Window, body, caption, col, control, dot, matches_text, name_generator, notice, panel, px,
    relative, rgb, row_ui,
};
use crate::{
    config::{MAX_ATTRIBUTE, MAX_OFFICERS, MIN_ATTRIBUTE, Officer},
    portraits::words,
};
use gpui::prelude::*;
use gpui_component::{Disableable, Selectable, Sizable, button::ButtonVariants};

pub(super) struct OfficerDraft {
    names: [Entity<InputState>; 2],
    age: Entity<InputState>,
    subtype: &'static str,
    female: bool,
    editing: Option<usize>,
    error: Option<String>,
}

impl OfficerDraft {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Editor>) -> Self {
        Self {
            age: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Random")
                    .validate(|s, _| s.len() <= 2 && s.bytes().all(|b| b.is_ascii_digit()))
            }),
            names: ["Family name", "Given name"].map(|label| {
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder(label)
                        .validate(|value, _| value.chars().count() <= 64)
                })
            }),
            subtype: CLASSES[0].0,
            female: false,
            editing: None,
            error: None,
        }
    }

    pub(super) fn reset(&mut self, window: &mut Window, cx: &mut Context<Editor>) {
        self.editing = None;
        self.error = None;
        self.age
            .update(cx, |input, cx| input.set_value("", window, cx));
        for input in &self.names {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
    }
}

impl Editor {
    pub(super) fn attribute_inputs(
        window: &mut Window,
        cx: &mut Context<Self>,
        subscriptions: &mut Vec<gpui::Subscription>,
    ) -> [Entity<InputState>; 5] {
        Attribute::ALL.map(|attribute| {
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Default")
                    .validate(|value, _| {
                        value.is_empty()
                            || value.bytes().all(|byte| byte.is_ascii_digit())
                                && value
                                    .parse::<u16>()
                                    .is_ok_and(|value| value <= MAX_ATTRIBUTE)
                    })
            });
            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |editor, input, event: &InputEvent, window, cx| {
                    let value = input.read(cx).value();
                    let parsed = value
                        .parse::<u16>()
                        .ok()
                        .filter(|value| *value >= MIN_ATTRIBUTE);
                    match event {
                        InputEvent::Change if value.is_empty() => {
                            editor.config.founder_attributes.remove(&attribute);
                            editor.edited(cx);
                        }
                        InputEvent::Change if parsed.is_some() => {
                            editor
                                .config
                                .founder_attributes
                                .insert(attribute, parsed.unwrap());
                            editor.edited(cx);
                        }
                        InputEvent::Blur | InputEvent::PressEnter { .. }
                            if !value.is_empty() && parsed.is_none() =>
                        {
                            let previous = editor
                                .config
                                .founder_attributes
                                .get(&attribute)
                                .map_or_else(String::new, u16::to_string);
                            input.update(cx, |input, cx| input.set_value(previous, window, cx));
                        }
                        _ => {}
                    }
                },
            ));
            input
        })
    }

    pub(super) fn founder_attributes(&self) -> Div {
        col()
            .gap_2()
            .child(caption("Base attributes"))
            .child(body(format!("Set any value from {MIN_ATTRIBUTE} to {MAX_ATTRIBUTE}. Career, traits and equipment add their bonuses in-game.")))
            .child(row_ui().gap_3().flex_wrap().children(
                self.attribute_inputs.iter().zip(Attribute::ALL).map(|(input, attribute)| {
                    let tone = match attribute {
                        Attribute::Authority => CLASSES[2].3,
                        Attribute::Expertise => CLASSES[3].3,
                        Attribute::Resolve => CLASSES[0].3,
                        Attribute::Instinct => CLASSES[1].3,
                        Attribute::Cunning => CLASSES[4].3,
                    };
                    col().gap_1().flex_1().min_w(px(105.)).child(
                        row_ui().gap_2().items_center().child(dot(tone, 8.)).child(caption(words(attribute.db_key("romance"))))
                    ).child(Input::new(input).w_full())
                }),
            ))
    }

    pub(super) fn available_careers(&self) -> Option<&Result<Vec<Career>, String>> {
        let library = self.library.as_ref()?;
        if self.config.expanded_compat {
            self.expanded_map
                .as_ref()?
                .as_ref()
                .ok()
                .map(|expanded| &expanded.careers)
        } else if self.config.trom_compat {
            library.trom_careers.as_ref()
        } else {
            Some(&library.careers)
        }
    }

    pub(super) fn career_picker(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let selected = self.config.starting_career.as_deref();
        let careers = match self.available_careers() {
            Some(Ok(items)) => items,
            Some(Err(error)) => return notice(error.clone(), CINNABAR).into_any_element(),
            None => {
                return body("Waiting for the selected campaign’s character data.")
                    .into_any_element();
            }
        };
        let (current, tone) = match selected {
            None => ("Class default", PAPER),
            Some(key) => careers
                .iter()
                .find(|career| career.key == key)
                .map_or(("Unavailable in this campaign", CINNABAR), |career| {
                    (career.title.as_str(), GOLD)
                }),
        };
        let query = self.query(cx);
        let entries: Vec<_> = careers
            .iter()
            .filter(|career| matches_text(&query, &[&career.title, &career.key]))
            .collect();
        let page = self.catalog_page(Catalog::Careers, entries.len());
        let grid = self.career_view == CatalogView::Grid;
        let columns = if window.viewport_size().width >= px(1200.) {
            3
        } else {
            2
        };
        col()
            .gap_2()
            .child(
                row_ui()
                    .items_center()
                    .gap_2()
                    .min_h(px(26.))
                    .flex_wrap()
                    .child(caption("In-game background"))
                    .child(body(current.to_owned()).text_color(rgb(tone)))
                    .when(selected.is_some(), |row| {
                        row.child(
                            Button::new("default-career")
                                .ghost()
                                .small()
                                .compact()
                                .label("Use class default")
                                .on_click(cx.listener(|editor, _, _, cx| {
                                    editor.config.starting_career = None;
                                    editor.edited(cx);
                                })),
                        )
                    }),
            )
            .child(self.catalog_search(Catalog::Careers, cx))
            .child(self.pagination(Catalog::Careers, page, false, cx))
            .child(
                col()
                    .gap_2()
                    .when(grid, |results| {
                        results.grid().grid_cols(columns).items_start()
                    })
                    .children(
                        entries[page.range()]
                            .iter()
                            .map(|career| self.career_entry(career, grid, cx)),
                    ),
            )
            .when(entries.is_empty(), |sheet| {
                sheet.child(body("No careers match your search."))
            })
            .when(page.pages > 1, |sheet| {
                sheet.child(self.pagination(Catalog::Careers, page, true, cx))
            })
            .into_any_element()
    }

    fn career_entry(&self, career: &Career, grid: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let key = &career.key;
        let chosen = self.config.starting_career.as_ref() == Some(key);
        let expanded = self.inspecting.as_ref() == Some(key);
        let historical_name = key
            .split_once("_career_historical_")
            .map(|(_, name)| words(name));
        let effects = &career.effects[..career.effects.len().min(2)];
        let remaining = career.effects.len() - effects.len();
        let modifiers = if effects.is_empty() {
            vec!["No listed gameplay modifiers".into()]
        } else if grid {
            effects.to_vec()
        } else {
            vec![effects.join("  •  ")]
        };
        let preview = col()
            .flex_1()
            .min_w_0()
            .when(grid, |preview| preview.min_h(px(32.)))
            .children(modifiers.into_iter().map(|effect| {
                body(effect)
                    .text_size(px(12.))
                    .line_height(px(16.))
                    .truncate()
            }));
        let details_key = key.clone();
        let select_key = key.clone();
        panel()
            .id(SharedString::from(key.clone()))
            .min_w_0()
            .px_3()
            .py_2()
            .gap_2()
            .when(chosen, |entry| entry.border_color(rgb(GOLD)))
            .child(
                row_ui()
                    .min_w_0()
                    .gap_2()
                    .when(grid, |entry| entry.flex_col().gap_1())
                    .when(!grid, |entry| entry.items_center())
                    .child(
                        col()
                            .min_w_0()
                            .when(grid, |name| name.min_h(px(34.)))
                            .when(!grid, |name| name.w(relative(0.28)).flex_shrink_0())
                            .child(
                                body(career.title.clone())
                                    .text_color(rgb(PAPER))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .line_height(px(17.)),
                            )
                            .when_some(historical_name, |name, donor| {
                                name.child(caption(donor).line_height(px(17.)))
                            }),
                    )
                    .when(!expanded, |entry| {
                        entry.child(
                            row_ui()
                                .items_center()
                                .gap_2()
                                .min_w_0()
                                .flex_1()
                                .child(preview)
                                .when(remaining > 0 && !grid, |row| {
                                    row.child(caption(format!("+{remaining} more")).flex_shrink_0())
                                }),
                        )
                    })
                    .when(expanded && !grid, |entry| entry.child(gpui::div().flex_1()))
                    .child(
                        row_ui()
                            .gap_1()
                            .justify_end()
                            .items_center()
                            .flex_shrink_0()
                            .when(grid && !expanded && remaining > 0, |actions| {
                                actions.child(caption(format!("+{remaining} more")).flex_1())
                            })
                            .child(
                                Button::new("career-details")
                                    .ghost()
                                    .small()
                                    .compact()
                                    .label(if expanded { "Hide details" } else { "Details" })
                                    .on_click(cx.listener(move |editor, _, _, cx| {
                                        editor.inspecting = if expanded {
                                            None
                                        } else {
                                            Some(details_key.clone())
                                        };
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("select-career")
                                    .outline()
                                    .small()
                                    .compact()
                                    .selected(chosen)
                                    .label(if chosen { "Selected" } else { "Select" })
                                    .on_click(cx.listener(move |editor, _, _, cx| {
                                        editor.config.starting_career = Some(select_key.clone());
                                        editor.edited(cx);
                                    })),
                            ),
                    ),
            )
            .when(expanded, |entry| {
                entry.child(
                    col()
                        .gap_1()
                        .pt_2()
                        .border_t_1()
                        .border_color(rgb(LINE))
                        .when(!career.description.is_empty(), |details| {
                            details.child(body(career.description.clone()).pb_1())
                        })
                        .children(career.effects.iter().map(|effect| {
                            row_ui()
                                .gap_2()
                                .items_start()
                                .child(caption("•").text_color(rgb(GOLD)).flex_shrink_0())
                                .child(
                                    body(effect.clone())
                                        .flex_1()
                                        .min_w_0()
                                        .text_size(px(12.))
                                        .text_color(rgb(PAPER)),
                                )
                        }))
                        .when(career.effects.is_empty(), |details| {
                            details.child(caption("No listed gameplay modifiers"))
                        }),
                )
            })
    }

    fn save_officer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let [surname, forename] = self
            .officer_draft
            .names
            .each_ref()
            .map(|input| input.read(cx).value().trim().to_owned());
        if surname.is_empty() || forename.is_empty() {
            self.officer_draft.error = Some("Enter a family name and a given name.".into());
            cx.notify();
            return;
        }
        let age = self.officer_draft.age.read(cx).value();
        let age = if age.is_empty() {
            None
        } else if let Some(n) = age.parse::<u8>().ok().filter(|n| (18..=80).contains(n)) {
            Some(n)
        } else {
            self.officer_draft.error = Some("Choose an officer age between 18 and 80.".into());
            cx.notify();
            return;
        };
        let officer = Officer {
            age,
            surname,
            forename,
            subtype: self.officer_draft.subtype.into(),
            female: self.officer_draft.female,
        };
        if let Some(index) = self.officer_draft.editing {
            self.config.officers[index] = officer;
        } else if self.config.officers.len() < MAX_OFFICERS {
            self.config.officers.push(officer);
        }
        self.officer_draft.reset(window, cx);
        self.edited(cx);
    }

    pub(super) fn officers(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let draft = &self.officer_draft;
        let at_limit = draft.editing.is_none() && self.config.officers.len() >= MAX_OFFICERS;
        col().gap_4()
            .child(panel().gap_3().child(caption(if draft.editing.is_some() { "Edit officer" } else { "New officer" }))
                .child(body("Starting officers join your court on the first turn and can be deployed or appointed from there."))
                .child(row_ui().gap_3().children(draft.names.iter().map(|input| col().flex_1().child(Input::new(input)) )))
                .child(row_ui().gap_2().items_center()
                    .child(caption("Starting age (18–80)"))
                    .child(gpui::div().w(px(90.)).child(Input::new(&draft.age))))
                .child(row_ui().gap_2().children([(false, "Male"), (true, "Female")].map(|(female, label)| {
                    control(label, draft.female == female).child(label)
                        .on_click(cx.listener(move |editor, _, _, cx| {
                            editor.officer_draft.female = female;
                            cx.notify();
                        }))
                })).child(control("officer-reroll", false).child("Roll name")
                    .on_click(cx.listener(|editor, _, window, cx| {
                        let gender = if editor.officer_draft.female { Gender::Female } else { Gender::Male };
                        let names = [name_generator::family_name(""), name_generator::given_name(gender, "")];
                        for (input, name) in editor.officer_draft.names.iter().zip(names) {
                            input.update(cx, |input, cx| input.set_value(name, window, cx));
                        }
                    }))))
                .child(row_ui().gap_2().flex_wrap().children(CLASSES.map(|(class, label, _, tone)| {
                    control(class, draft.subtype == class).gap_2().child(dot(tone, 8.)).child(label)
                        .on_click(cx.listener(move |editor, _, _, cx| {
                            editor.officer_draft.subtype = class;
                            cx.notify();
                        }))
                })))
                .when_some(draft.error.as_ref(), |panel, error| panel.child(notice(error.clone(), CINNABAR)))
                .child(row_ui().gap_2().child(Button::new("save-officer")
                    .label(if draft.editing.is_some() { "Save officer" } else { "Add officer" })
                    .disabled(at_limit)
                    .on_click(cx.listener(|editor, _, window, cx| editor.save_officer(window, cx))))
                    .when(draft.editing.is_some(), |row| row.child(control("cancel-officer", false).child("Cancel")
                        .on_click(cx.listener(|editor, _, window, cx| {
                            editor.officer_draft.reset(window, cx);
                            cx.notify();
                        })))))
                .when(at_limit, |panel| panel.child(body(format!("The court already has {MAX_OFFICERS} starting officers.")))))
            .children(self.config.officers.iter().enumerate().map(|(index, officer)| {
                let class = CLASSES.iter().find(|class| class.0 == officer.subtype).unwrap_or(&CLASSES[0]);
                panel().gap_2().child(row_ui().gap_2().items_center()
                    .child(col().flex_1().child(caption(format!("{} {}", officer.surname, officer.forename)))
                        .child(body(class.1)))
                    .child(control(format!("edit-officer-{index}"), draft.editing == Some(index)).child("Edit")
                        .on_click(cx.listener(move |editor, _, window, cx| {
                            let officer = &editor.config.officers[index];
                            editor.officer_draft.editing = Some(index);
                            editor.officer_draft.subtype = class.0;
                            editor.officer_draft.female = officer.female;
                            let age = officer.age.map(|n| n.to_string()).unwrap_or_default();
                            editor.officer_draft.age.update(cx, |input, cx| {
                                input.set_value(age, window, cx);
                            });
                            editor.officer_draft.error = None;
                            for (input, name) in editor.officer_draft.names.iter().zip([&officer.surname, &officer.forename]) {
                                input.update(cx, |input, cx| input.set_value(name.clone(), window, cx));
                            }
                            cx.notify();
                        })))
                    .child(control(format!("remove-officer-{index}"), false).child("Remove")
                        .on_click(cx.listener(move |editor, _, window, cx| {
                            editor.config.officers.remove(index);
                            crate::relationships::remove_officer(&mut editor.config.relationships, index);
                            editor.relationship_editor.clear_selection();
                            match editor.officer_draft.editing {
                                Some(current) if current == index => editor.officer_draft.reset(window, cx),
                                Some(current) if current > index => editor.officer_draft.editing = Some(current - 1),
                                _ => {}
                            }
                            editor.edited(cx);
                        }))))
            }))
            .into_any_element()
    }
}
