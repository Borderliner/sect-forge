use super::*;
use crate::relationships::{Person, Relationship, Roster, Tie};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct RelationshipEditor {
    // The absence of a result means the background read is still running.
    rosters: BTreeMap<(bool, bool), Option<Result<Roster, String>>>,
    pair: [Option<Person>; 2],
    side: usize,
    tie: Tie,
    error: Option<String>,
}
impl RelationshipEditor {
    pub(super) fn clear_selection(&mut self) {
        self.pair = [Some(Person::Founder), None];
        self.side = 1;
        self.error = None;
    }
}
impl Editor {
    pub(super) fn load_relationships(&mut self, cx: &mut Context<Self>) {
        if self.founder_tab != FounderTab::Relationships {
            return;
        }
        let Some(game) = self.library.as_ref().map(|l| l.game.clone()) else {
            return;
        };
        let base = (self.config.trom_compat, self.config.expanded_compat);
        if self.relationship_editor.rosters.contains_key(&base) {
            return;
        }
        self.relationship_editor.rosters.insert(base, None);
        if self.relationship_editor.pair == [None, None] {
            self.relationship_editor.clear_selection();
        }
        let paths = if base.0 {
            trom::packs(&self.workshop).map(Vec::from)
        } else if base.1 {
            expanded::packs(&self.workshop).map(Vec::from)
        } else {
            Ok(Vec::new())
        };
        let source = game.clone();
        let work = cx.background_executor().spawn(async move {
            paths
                .and_then(|p| Roster::read(&source, &p))
                .map_err(|e| format!("{e:#}"))
        });
        cx.spawn(async move |entity, cx| {
            let result = work.await;
            entity
                .update(cx, |s, cx| {
                    if s.library
                        .as_ref()
                        .is_some_and(|l| Arc::ptr_eq(&l.game, &game))
                    {
                        s.relationship_editor.rosters.insert(base, Some(result));
                        cx.notify();
                    }
                })
                .ok();
        })
        .detach();
    }

    fn person_name(&self, p: &Person, roster: &Roster) -> String {
        match p {
            Person::Founder => format!("{} {}", self.config.surname, self.config.forename),
            Person::Officer(i) => self
                .config
                .officers
                .get(*i)
                .map(|o| format!("{} {}", o.surname, o.forename))
                .unwrap_or_else(|| "Missing officer".into()),
            Person::Historical(k) => roster
                .people
                .get(k)
                .map(|h| h.name.clone())
                .unwrap_or_else(|| format!("{k} (unavailable)")),
        }
    }

    pub(super) fn relationships(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = &self.relationship_editor;
        let base = (self.config.trom_compat, self.config.expanded_compat);
        let roster = match state.rosters.get(&base).and_then(Option::as_ref) {
            Some(Ok(r)) => r,
            Some(Err(e)) => return notice(e.clone(), CINNABAR).into_any_element(),
            None => {
                return body("Reading characters from the selected 190 campaign…")
                    .into_any_element();
            }
        };
        let query = self.query(cx);
        let mut people = vec![(
            Person::Founder,
            self.person_name(&Person::Founder, roster),
            "Founder".to_owned(),
        )];
        for (i, _) in self.config.officers.iter().enumerate() {
            let p = Person::Officer(i);
            people.push((
                p.clone(),
                self.person_name(&p, roster),
                "Starting officer".into(),
            ));
        }
        let start = people.len();
        for (key, h) in &roster.people {
            let age = h.age.map(|a| format!(" · age {a}")).unwrap_or_default();
            people.push((
                Person::Historical(key.clone()),
                h.name.clone(),
                format!("{}{age}", h.faction),
            ));
        }
        people[start..].sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
        people.retain(|(_, name, desc)| {
            query.is_empty()
                || name.to_lowercase().contains(&query)
                || desc.to_lowercase().contains(&query)
        });
        let page = self.catalog_page(Catalog::Characters, people.len());
        let mut editor = panel().gap_3().child(caption("New relationship")).child(
            row_ui().gap_2().flex_wrap().children(
                [
                    (Tie::Spouse, "Spouse"),
                    (Tie::Parent, "Parent → child"),
                    (Tie::Friend, "Friends"),
                    (Tie::Rival, "Rivals"),
                ]
                .map(|(tie, label)| {
                    control(label, state.tie == tie)
                        .child(label)
                        .on_click(cx.listener(move |s, _, _, cx| {
                            s.relationship_editor.tie = tie;
                            s.relationship_editor.error = None;
                            cx.notify();
                        }))
                }),
            ),
        );
        let mut pair = row_ui().gap_2().items_start();
        for i in 0..2 {
            let person = state.pair[i].as_ref();
            let title = if state.tie == Tie::Parent {
                ["Parent", "Child"][i]
            } else {
                ["First character", "Second character"][i]
            };
            let name = person
                .map(|p| self.person_name(p, roster))
                .unwrap_or_else(|| "Choose a character…".into());
            let family = person
                .and_then(|p| {
                    if let Person::Historical(k) = p {
                        Some(roster.family_info(k))
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            pair = pair.child(
                col()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(caption(title))
                    .child(
                        control(format!("relationship-side-{i}"), state.side == i)
                            .justify_start()
                            .child(name)
                            .on_click(cx.listener(move |s, _, window, cx| {
                                s.relationship_editor.side = i;
                                s.relationship_editor.error = None;
                                s.clear_search(window, cx);
                                cx.notify();
                            })),
                    )
                    .when(!family.is_empty(), |c| c.child(body(family))),
            );
        }
        editor = editor
            .when(matches!(state.tie, Tie::Friend | Tie::Rival), |c| {
                c.child(body("This sets the pair’s relationship on turn one of a new campaign. Later events can change it."))
            })
            .child(pair)
            .child(
                Button::new("add-relationship")
                    .label("Add relationship")
                    .disabled(state.pair.iter().any(Option::is_none))
                    .on_click(cx.listener(move |s, _, _, cx| {
                        let [Some(first), Some(second)] = s.relationship_editor.pair.clone() else {
                            return;
                        };
                        let mut links = s.config.relationships.clone();
                        links.push(Relationship {
                            first,
                            second,
                            tie: s.relationship_editor.tie,
                        });
                        let Some(Some(Ok(roster))) = s.relationship_editor.rosters.get(&base)
                        else {
                            return;
                        };
                        match s
                            .founder_gender()
                            .context("Wait for the founder portrait to load.")
                            .and_then(|g| roster.validate(&s.config, g == Gender::Male, &links))
                        {
                            Ok(()) => {
                                s.config.relationships = links;
                                s.relationship_editor.pair[1] = None;
                                s.relationship_editor.side = 1;
                                s.relationship_editor.error = None;
                                s.edited(cx);
                            }
                            Err(e) => {
                                s.relationship_editor.error = Some(e.to_string());
                                cx.notify();
                            }
                        }
                    })),
            )
            .when_some(state.error.clone(), |c, e| c.child(notice(e, CINNABAR)));
        let mut view = col().gap_3().child(editor);
        if !self.config.relationships.is_empty() {
            view = view.child(
                panel()
                    .gap_2()
                    .child(caption("Starting relationships"))
                    .children(self.config.relationships.iter().enumerate().map(|(i, r)| {
                        row_ui()
                            .gap_2()
                            .items_center()
                            .child(div().flex_1().child(body(format!(
                                "{} {} {}",
                                self.person_name(&r.first, roster),
                                r.tie.verb(),
                                self.person_name(&r.second, roster)
                            ))))
                            .child(
                                control(format!("remove-relationship-{i}"), false)
                                    .child("Remove")
                                    .on_click(cx.listener(move |s, _, _, cx| {
                                        s.config.relationships.remove(i);
                                        s.relationship_editor.error = None;
                                        s.edited(cx);
                                    })),
                            )
                    })),
            );
        }
        view.child(
            panel()
                .gap_2()
                .child(caption(if state.side == 0 {
                    "Choose the first character"
                } else {
                    "Choose the second character"
                }))
                .child(self.search_box())
                .child(self.pagination(Catalog::Characters, page, false, cx))
                .children(
                    people[page.range()]
                        .iter()
                        .enumerate()
                        .map(|(i, (p, name, desc))| {
                            let p = p.clone();
                            control(
                                format!("link-candidate-{i}"),
                                state.pair[state.side].as_ref() == Some(&p),
                            )
                            .justify_between()
                            .child(name.clone())
                            .child(body(desc.clone()))
                            .on_click(cx.listener(
                                move |s, _, window, cx| {
                                    let state = &mut s.relationship_editor;
                                    state.pair[state.side] = Some(p.clone());
                                    state.error = None;
                                    if state.side == 0 {
                                        state.side = 1;
                                        s.clear_search(window, cx);
                                    }
                                    cx.notify();
                                },
                            ))
                        }),
                ),
        )
        .into_any_element()
    }
}
