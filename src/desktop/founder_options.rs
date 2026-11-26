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

pub(super) struct OfficerEditor {
    pub(super) index: usize,
    pub(super) names: [Entity<InputState>; 2],
    age: Entity<InputState>,
    attributes: [Entity<InputState>; 5],
    _subscriptions: Vec<gpui::Subscription>,
}
impl Editor {
    pub(super) fn attribute_inputs(
        window: &mut Window,
        cx: &mut Context<Self>,
        subscriptions: &mut Vec<gpui::Subscription>,
        officer: Option<(usize, &Officer)>,
    ) -> [Entity<InputState>; 5] {
        Attribute::ALL.map(|attribute| {
            let value = officer
                .and_then(|(_, o)| o.attributes.get(&attribute))
                .map_or_else(String::new, u16::to_string);
            let index = officer.map(|(i, _)| i);
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(value)
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
                    let attributes = match index {
                        Some(i) if editor.officer_editor.as_ref().is_some_and(|e| e.index == i) => {
                            &mut editor.config.officers[i].attributes
                        }
                        Some(_) => return,
                        None => &mut editor.config.founder_attributes,
                    };
                    let value = input.read(cx).value();
                    let parsed = value
                        .parse::<u16>()
                        .ok()
                        .filter(|value| *value >= MIN_ATTRIBUTE);
                    match event {
                        InputEvent::Change if value.is_empty() => {
                            attributes.remove(&attribute);
                            editor.edited(cx);
                        }
                        InputEvent::Change if parsed.is_some() => {
                            attributes.insert(attribute, parsed.unwrap());
                            editor.edited(cx);
                        }
                        InputEvent::Blur | InputEvent::PressEnter { .. }
                            if !value.is_empty() && parsed.is_none() =>
                        {
                            let previous = attributes
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

    pub(super) fn attribute_fields(&self, inputs: &[Entity<InputState>; 5]) -> Div {
        col()
            .gap_2()
            .child(caption("Base attributes"))
            .child(body(format!("Set any value from {MIN_ATTRIBUTE} to {MAX_ATTRIBUTE}. Career, traits and equipment add their bonuses in-game.")))
            .child(row_ui().gap_3().flex_wrap().children(
                inputs.iter().zip(Attribute::ALL).map(|(input, attribute)| {
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
        let selected = self.character().starting_career.as_deref();
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
                                    editor.character_mut().starting_career = None;
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
        let chosen = self.character().starting_career.as_ref() == Some(key);
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
                                        editor.character_mut().starting_career =
                                            Some(select_key.clone());
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

    pub(super) fn edit_officer(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let o = &self.config.officers[index];
        let mut subscriptions = Vec::new();
        let names = [&o.surname, &o.forename].map(|name| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(name.clone())
                    .validate(|s, _| s.chars().count() <= 64)
            })
        });
        for (n, input) in names.iter().enumerate() {
            subscriptions.push(cx.subscribe(
                input,
                move |editor, input, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change)
                        && editor
                            .officer_editor
                            .as_ref()
                            .is_some_and(|e| e.index == index)
                    {
                        let officer = &mut editor.config.officers[index];
                        let name = if n == 0 {
                            &mut officer.surname
                        } else {
                            &mut officer.forename
                        };
                        *name = input.read(cx).value().to_string();
                        editor.edited(cx);
                    }
                },
            ));
        }
        let age = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Random")
                .default_value(o.age.map_or_else(String::new, |a| a.to_string()))
                .validate(|s, _| s.len() <= 2 && s.bytes().all(|b| b.is_ascii_digit()))
        });
        subscriptions.push(cx.subscribe_in(
            &age,
            window,
            move |editor, input, event: &InputEvent, window, cx| {
                if !editor
                    .officer_editor
                    .as_ref()
                    .is_some_and(|e| e.index == index)
                {
                    return;
                }
                let value = input.read(cx).value();
                let age = value.parse::<u8>().ok().filter(|a| (18..=80).contains(a));
                match event {
                    InputEvent::Change if value.is_empty() || age.is_some() => {
                        editor.config.officers[index].age = age;
                        editor.edited(cx);
                    }
                    InputEvent::Blur | InputEvent::PressEnter { .. }
                        if !value.is_empty() && age.is_none() =>
                    {
                        let previous = editor.config.officers[index]
                            .age
                            .map_or_else(String::new, |a| a.to_string());
                        input.update(cx, |input, cx| input.set_value(previous, window, cx));
                    }
                    _ => {}
                }
            },
        ));
        let attributes = Self::attribute_inputs(window, cx, &mut subscriptions, Some((index, o)));
        self.officer_editor = Some(OfficerEditor {
            index,
            names,
            age,
            attributes,
            _subscriptions: subscriptions,
        });
        self.founder_tab = super::FounderTab::Identity;
        self.restore_portrait_source(cx);
        self.clear_search(window, cx);
        cx.notify();
    }

    pub(super) fn officer_details(&self, editor: &OfficerEditor, cx: &mut Context<Self>) -> Div {
        let index = editor.index;
        let o = &self.config.officers[index];
        panel()
            .gap_4()
            .child(
                row_ui().gap_3().children(
                    editor
                        .names
                        .iter()
                        .zip(["Family name", "Given name"])
                        .enumerate()
                        .map(|(n, (input, label))| {
                            col().flex_1().gap_1().child(caption(label)).child(
                                row_ui()
                                    .gap_2()
                                    .child(gpui::div().flex_1().child(Input::new(input)))
                                    .child(
                                        control(format!("roll-officer-name-{n}"), false)
                                            .child("Roll")
                                            .on_click(cx.listener(move |s, _, window, cx| {
                                                let Some(editor) = &s.officer_editor else {
                                                    return;
                                                };
                                                let o = &s.config.officers[editor.index];
                                                let name = if n == 0 {
                                                    name_generator::family_name(&o.surname)
                                                } else {
                                                    name_generator::given_name(
                                                        if o.female {
                                                            Gender::Female
                                                        } else {
                                                            Gender::Male
                                                        },
                                                        &o.forename,
                                                    )
                                                };
                                                editor.names[n].update(cx, |input, cx| {
                                                    input.set_value(name, window, cx)
                                                });
                                            })),
                                    ),
                            )
                        }),
                ),
            )
            .child(
                row_ui()
                    .gap_3()
                    .items_center()
                    .child(caption("Starting age · 18–80"))
                    .child(gpui::div().w(px(90.)).child(Input::new(&editor.age)))
                    .children([(false, "Male"), (true, "Female")].map(|(female, label)| {
                        control(label, o.female == female)
                            .child(label)
                            .on_click(cx.listener(move |s, _, _, cx| {
                                let o = &mut s.config.officers[index];
                                if o.female != female {
                                    o.female = female;
                                    o.art_set.clear();
                                    o.portrait_pack = None;
                                    s.restore_portrait_source(cx);
                                    s.edited(cx);
                                }
                            }))
                    })),
            )
            .child(self.class_picker(cx))
            .child(self.toggle(
                "legendary-officer",
                "Legendary officer",
                "Boosts the default primary attribute and marks this officer as unique.",
                o.legendary,
                move |s, _| {
                    let o = &mut s.config.officers[index];
                    o.legendary = !o.legendary;
                },
                cx,
            ))
            .child(self.attribute_fields(&editor.attributes))
    }

    pub(super) fn officers(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        col()
            .gap_3()
            .child(
                row_ui()
                    .gap_2()
                    .items_center()
                    .child(
                        col()
                            .flex_1()
                            .child(caption(format!(
                                "Starting officers · {} / {MAX_OFFICERS}",
                                self.config.officers.len()
                            )))
                            .child(body("Starting officers join your court on turn one.")),
                    )
                    .child(
                        Button::new("add-officer")
                            .label("Create officer")
                            .disabled(self.config.officers.len() >= MAX_OFFICERS)
                            .on_click(cx.listener(|s, _, window, cx| {
                                if s.config.officers.len() >= MAX_OFFICERS {
                                    return;
                                }
                                let i = s.config.officers.len();
                                s.config.officers.push(Officer {
                                    surname: name_generator::family_name("").into(),
                                    forename: name_generator::given_name(Gender::Male, "").into(),
                                    female: false,
                                    age: Some(25),
                                    attributes: Default::default(),
                                    legendary: false,
                                    character: Default::default(),
                                });
                                s.edit_officer(i, window, cx);
                                s.edited(cx);
                            })),
                    ),
            )
            .children(self.config.officers.iter().enumerate().map(|(index, o)| {
                let class = CLASSES
                    .iter()
                    .find(|c| c.0 == o.subtype)
                    .unwrap_or(&CLASSES[0]);
                panel().child(
                    row_ui()
                        .gap_3()
                        .items_center()
                        .child(dot(class.3, 9.))
                        .child(
                            col()
                                .flex_1()
                                .child(caption(format!("{} {}", o.surname, o.forename)))
                                .child(body(format!(
                                    "{} · {} traits · {} equipped",
                                    class.1,
                                    o.starting_traits.len(),
                                    o.starting_ancillaries.len()
                                ))),
                        )
                        .child(
                            control(format!("edit-officer-{index}"), false)
                                .child("Edit character")
                                .on_click(cx.listener(move |s, _, window, cx| {
                                    s.edit_officer(index, window, cx)
                                })),
                        )
                        .child(
                            control(format!("remove-officer-{index}"), false)
                                .child("Remove")
                                .on_click(cx.listener(move |s, _, _, cx| {
                                    s.config.officers.remove(index);
                                    crate::relationships::remove_officer(
                                        &mut s.config.relationships,
                                        index,
                                    );
                                    s.relationship_editor.clear_selection();
                                    s.edited(cx);
                                })),
                        ),
                )
            }))
            .into_any_element()
    }
}
