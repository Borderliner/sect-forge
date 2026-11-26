use super::{
    Attribute, Button, CINNABAR, CLASSES, Career, Catalog, CatalogPage, Context, Div, Editor,
    Entity, GOLD, Gender, Input, InputEvent, InputState, JADE, Window, body, caption, col, control,
    dot, name_generator, notice, panel, px, row_ui,
};
use crate::{
    config::{MAX_ATTRIBUTE, MAX_OFFICERS, MIN_ATTRIBUTE, Officer},
    portraits::words,
};
use gpui::prelude::*;
use gpui_component::Disableable;

pub(super) struct OfficerDraft {
    names: [Entity<InputState>; 2],
    subtype: &'static str,
    female: bool,
    editing: Option<usize>,
    error: Option<String>,
}

impl OfficerDraft {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Editor>) -> Self {
        Self {
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

    pub(super) fn career_picker(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let selected = self.config.starting_career.as_deref();
        let mut sheet = col().gap_4().child(
            panel()
                .gap_2()
                .child(caption("In-game background"))
                .child(body(
                    "Choose a career to give the founder its title and effects.",
                ))
                .child(
                    control("default-career", selected.is_none())
                        .child("Class default")
                        .on_click(cx.listener(|editor, _, _, cx| {
                            editor.config.starting_career = None;
                            editor.edited(cx);
                        })),
                ),
        );
        let careers = match self.available_careers() {
            Some(Ok(items)) => items,
            Some(Err(error)) => {
                return sheet
                    .child(notice(error.clone(), CINNABAR))
                    .into_any_element();
            }
            None => {
                return sheet
                    .child(body("Waiting for the selected campaign’s character data."))
                    .into_any_element();
            }
        };
        if let Some(key) = selected {
            let title = careers.iter().find(|item| item.key == key);
            sheet = sheet.child(match title {
                Some(career) => notice(format!("Selected career · {}", career.title), JADE),
                None => notice(
                    "The selected career is unavailable in this campaign.",
                    CINNABAR,
                ),
            });
        }
        let query = self.search.read(cx).value().to_lowercase();
        let filtered: Vec<_> = careers
            .iter()
            .filter(|career| {
                career.title.to_lowercase().contains(&query)
                    || career.key.replace('_', " ").to_lowercase().contains(&query)
            })
            .collect();
        let page = CatalogPage::new(
            self.results_page,
            Catalog::Careers.page_size(self.trait_view),
            filtered.len(),
        );
        sheet
            .child(Input::new(&self.search))
            .child(self.pagination(Catalog::Careers, page, false, cx))
            .children(filtered[page.range()].iter().map(|career| {
                let key = career.key.clone();
                let chosen = selected == Some(key.as_str());
                let historical_name = key
                    .split_once("_career_historical_")
                    .map(|(_, name)| words(name));
                panel()
                    .gap_2()
                    .child(
                        row_ui()
                            .gap_3()
                            .items_center()
                            .child(
                                col()
                                    .flex_1()
                                    .gap_1()
                                    .child(caption(career.title.clone()))
                                    .when_some(historical_name, |column, name| {
                                        column.child(body(name))
                                    }),
                            )
                            .child(
                                control(key.clone(), chosen)
                                    .child(if chosen { "Selected" } else { "Select" })
                                    .on_click(cx.listener(move |editor, _, _, cx| {
                                        editor.config.starting_career = Some(key.clone());
                                        editor.edited(cx);
                                    })),
                            ),
                    )
                    .when(!career.description.is_empty(), |panel| {
                        panel.child(body(career.description.clone()))
                    })
                    .children(
                        career
                            .effects
                            .iter()
                            .map(|effect| notice(effect.clone(), GOLD)),
                    )
            }))
            .child(self.pagination(Catalog::Careers, page, true, cx))
            .into_any_element()
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
        let officer = Officer {
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
                            editor.officer_draft.error = None;
                            for (input, name) in editor.officer_draft.names.iter().zip([&officer.surname, &officer.forename]) {
                                input.update(cx, |input, cx| input.set_value(name.clone(), window, cx));
                            }
                            cx.notify();
                        })))
                    .child(control(format!("remove-officer-{index}"), false).child("Remove")
                        .on_click(cx.listener(move |editor, _, window, cx| {
                            editor.config.officers.remove(index);
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
