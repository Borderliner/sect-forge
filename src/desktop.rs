use std::{
    borrow::Cow,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::{Context as _, Result};
use gpui::{
    Animation, AnimationExt as _, AnyElement, App, Application, AssetSource, Bounds, Context, Div,
    Entity, Hsla, ObjectFit, PathPromptOptions, Pixels, Point, Render, Rgba, ScrollHandle,
    ScrollWheelEvent, SharedString, Stateful, StyledImage as _, Subscription, TitlebarOptions,
    Window, WindowBounds, WindowOptions, div, ease_in_out, img, linear_color_stop, linear_gradient,
    point, prelude::*, px, relative, rgb, rgba, size,
};
use gpui_component::{
    Disableable, Root, Selectable, Sizable, Theme, ThemeMode,
    button::Button,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
    input::{Input, InputEvent, InputState},
    scroll::{Scrollbar, ScrollbarAxis, ScrollbarShow},
};
use rpfm_lib::files::{Container, pack::Pack};

use crate::{
    config::{
        CLASSES, Config, HEPU, MAX_FOUNDER_AGE, MIN_FOUNDER_AGE, SETTLEMENTS, SLOTS, Settlement,
        Variant, WARLORDS, output_dir,
    },
    export,
    game::{self, EquipmentCatalog, EquipmentStats, Game, MAP_IMAGE, Trait, TraitCategory},
    portraits::{self, Portrait},
    trom,
};

mod catalog;
mod game_font;
mod name_generator;
use catalog::{Catalog, CatalogPage, RARITIES, TRAIT_CATEGORIES, TraitView};
use game_font::GameHeadingFont;
use name_generator::Gender;

const INK: u32 = 0x101210;
const RAIL: u32 = 0x151814;
const CARD: u32 = 0x1b201a;
const LIFT: u32 = 0x262d23;
const LINE: u32 = 0x373f33;
const PAPER: u32 = 0xf0e8d6;
const MUTED: u32 = 0xb2b4a4;
const GOLD: u32 = 0xe2bd79;
const JADE: u32 = 0xa5bf91;
const CINNABAR: u32 = 0xeb967e;
const RAIL_WIDTH: f32 = 208.;
const PAGE_PAD: f32 = 36.;

#[derive(Clone, Default)]
struct Artwork {
    packs: Arc<RwLock<HashMap<usize, Arc<Pack>>>>,
    next: Arc<AtomicUsize>,
}

impl Artwork {
    fn register(&self, pack: Arc<Pack>) -> usize {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.packs.write().unwrap().insert(id, pack);
        id
    }

    fn discard(&self, id: usize) {
        self.packs.write().unwrap().remove(&id);
    }
}

impl AssetSource for Artwork {
    fn load(&self, key: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if key == "sect-forge.png" {
            return Ok(Some(Cow::Borrowed(include_bytes!("data/sect-forge.png"))));
        }
        let Some((id, path)) = key
            .strip_prefix("forge-art/")
            .and_then(|rest| rest.split_once('/'))
        else {
            return Ok(None);
        };
        let Some(pack) = id
            .parse::<usize>()
            .ok()
            .and_then(|id| self.packs.read().unwrap().get(&id).cloned())
        else {
            return Ok(None);
        };
        Ok(Some(Cow::Owned(game::raw(&pack, path)?)))
    }

    fn list(&self, _: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

struct Library {
    game: Arc<Game>,
    art_id: usize,
    portraits: Vec<Portrait>,
    traits: Vec<Trait>,
    trait_error: Option<String>,
    equipment: Result<EquipmentCatalog, String>,
    trom_equipment: Option<Result<EquipmentCatalog, String>>,
    trom_art: Option<[usize; 2]>,
}

#[derive(Clone)]
struct Source {
    path: PathBuf,
    pack: Arc<Pack>,
    art_id: usize,
    portraits: Vec<Portrait>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Identity,
    Founder,
    Settlement,
    Review,
}

impl Page {
    const ALL: [(Self, &'static str, &'static str); 4] = [
        (Self::Identity, "Identity", "◇"),
        (Self::Founder, "Founder", "♙"),
        (Self::Settlement, "Starting settlement", "⌖"),
        (Self::Review, "Review & export", "↗"),
    ];
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FounderTab {
    Identity,
    Portrait,
    Traits,
    Loadout,
}

#[derive(Clone, Copy)]
enum NameField {
    Faction = 0,
    Family = 1,
    Given = 2,
}

impl NameField {
    fn label(self) -> &'static str {
        match self {
            Self::Faction => "Faction name",
            Self::Family => "Family name",
            Self::Given => "Given name",
        }
    }
}

struct Editor {
    config: Config,
    game_path: Option<PathBuf>,
    artwork: Artwork,
    heading_font: Option<GameHeadingFont>,
    library: Option<Library>,
    source: Option<Arc<Source>>,
    selected_source: Option<Arc<Source>>,
    workshop: Vec<PathBuf>,
    show_workshop: bool,
    source_loading: bool,
    source_error: Option<String>,
    loading: bool,
    error: Option<String>,
    output: PathBuf,
    exported: Option<PathBuf>,
    exporting: bool,
    export_status: Option<String>,
    page: Page,
    founder_tab: FounderTab,
    slot: &'static str,
    equipment_rarity: Option<&'static str>,
    trait_category: Option<TraitCategory>,
    trait_view: TraitView,
    portrait_filter: &'static str,
    city_filter: &'static str,
    results_page: usize,
    workshop_page: usize,
    workspace_scroll: ScrollHandle,
    scale_factor: f32,
    zoom: f32,
    map_scroll: ScrollHandle,
    warlords: bool,
    inspecting: Option<String>,
    motion: usize,
    fields: [Entity<InputState>; 5],
    colour_pickers: [Entity<ColorPickerState>; 2],
    founder_age: Entity<InputState>,
    search: Entity<InputState>,
    pack_search: Entity<InputState>,
    subscriptions: Vec<Subscription>,
}

impl Editor {
    fn heading(&self, text: impl Into<SharedString>, size: f32) -> Div {
        let text = text.into();
        let height = px(size * 1.16);
        let element = row_ui()
            .min_w_0()
            .max_w_full()
            .h(height)
            .min_h(height)
            .items_center()
            .overflow_hidden();
        let pixel_em = (size * self.scale_factor).round() as u16;
        if let Some(heading) = self
            .heading_font
            .as_ref()
            .and_then(|font| font.heading(text.as_ref(), pixel_em))
        {
            return element.child(
                img(heading.image)
                    .w(px(size * heading.width_em))
                    .h(px(size * heading.height_em))
                    .max_w_full()
                    .object_fit(ObjectFit::Contain),
            );
        }
        element.child(
            div()
                .min_w_0()
                .font_weight(gpui::FontWeight::BOLD)
                .text_size(px(size))
                .line_height(height)
                .truncate()
                .child(text),
        )
    }

    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        artwork: Artwork,
        game_path: Option<PathBuf>,
        heading_font: Option<GameHeadingFont>,
    ) -> Self {
        let defaults = Config::default();
        let config = Config {
            sect_name: name_generator::faction_name(
                defaults.variant,
                Some(Gender::Male),
                &defaults.sect_name,
            ),
            art_set: "3k_main_art_set_historical_zhao_yun_general".into(),
            ..defaults
        };
        let fields = [
            config.sect_name.as_str(),
            config.surname.as_str(),
            config.forename.as_str(),
            config.primary_colour.as_str(),
            config.secondary_colour.as_str(),
        ]
        .map(|value| cx.new(|cx| InputState::new(window, cx).default_value(value.to_owned())));
        let colour_pickers = [&config.primary_colour, &config.secondary_colour].map(|hex| {
            cx.new(|cx| ColorPickerState::new(window, cx).default_value(rgb(colour(hex, LINE))))
        });
        let founder_age = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(config.founder_age.to_string())
                .validate(|text, _| {
                    text.len() <= 2 && text.bytes().all(|byte| byte.is_ascii_digit())
                })
        });
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search by name"));
        let pack_search = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Filter packs by name or Workshop ID")
        });
        let mut subscriptions = Vec::new();
        for (index, input) in fields.iter().enumerate() {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                move |editor, input, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change) {
                        let value = input.read(cx).value().to_string();
                        if index >= 3 {
                            let hex = value.trim().trim_start_matches('#');
                            if hex.len() == 6
                                && let Ok(colour) = u32::from_str_radix(hex, 16)
                            {
                                editor.colour_pickers[index - 3].update(cx, |picker, cx| {
                                    picker.set_value(rgb(colour), window, cx);
                                });
                            }
                        }
                        match index {
                            0 if editor.config.sect_name != value => {
                                editor.config.sect_name = value;
                                editor.config.faction_key = None;
                            }
                            0 => {}
                            1 => editor.config.surname = value,
                            2 => editor.config.forename = value,
                            3 => editor.config.primary_colour = value,
                            _ => editor.config.secondary_colour = value,
                        }
                        editor.exported = None;
                        cx.notify();
                    }
                },
            ));
        }
        for (index, picker) in colour_pickers.iter().enumerate() {
            subscriptions.push(cx.subscribe_in(
                picker,
                window,
                move |editor, _, event: &ColorPickerEvent, window, cx| {
                    if let ColorPickerEvent::Change(Some(colour)) = event {
                        let Rgba { r, g, b, .. } = Rgba::from(*colour);
                        let [r, g, b] = [r, g, b].map(|channel| (channel * 255.).round() as u8);
                        let hex = format!("{r:02X}{g:02X}{b:02X}");
                        editor.fields[index + 3].update(cx, |input, cx| {
                            input.set_value(hex, window, cx);
                        });
                    }
                },
            ));
        }
        subscriptions.push(cx.subscribe_in(
            &founder_age,
            window,
            |editor, input, event: &InputEvent, window, cx| {
                let value = input.read(cx).value();
                let parsed = value.parse::<u8>().ok();
                let range = MIN_FOUNDER_AGE..=MAX_FOUNDER_AGE;
                let age = match event {
                    InputEvent::Change => parsed.filter(|age| range.contains(age)),
                    InputEvent::Blur | InputEvent::PressEnter { .. } => {
                        let age = parsed
                            .unwrap_or(editor.config.founder_age)
                            .clamp(MIN_FOUNDER_AGE, MAX_FOUNDER_AGE);
                        let text = age.to_string();
                        if value.as_ref() != text {
                            input.update(cx, |input, cx| input.set_value(text, window, cx));
                        }
                        Some(age)
                    }
                    InputEvent::Focus => None,
                };
                if let Some(age) = age
                    && age != editor.config.founder_age
                {
                    editor.config.founder_age = age;
                    editor.edited(cx);
                }
            },
        ));
        subscriptions.push(cx.subscribe(&search, |editor, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                editor.results_page = 0;
                cx.notify();
            }
        }));
        subscriptions.push(cx.subscribe(&pack_search, |editor, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                editor.workshop_page = 0;
                cx.notify();
            }
        }));
        let mut editor = Self {
            config,
            game_path,
            artwork,
            heading_font,
            library: None,
            source: None,
            selected_source: None,
            workshop: Vec::new(),
            show_workshop: false,
            source_loading: false,
            source_error: None,
            loading: false,
            error: None,
            output: output_dir(),
            exported: None,
            exporting: false,
            export_status: None,
            page: Page::Identity,
            founder_tab: FounderTab::Identity,
            slot: "weapon",
            equipment_rarity: None,
            trait_category: None,
            trait_view: TraitView::List,
            portrait_filter: "All",
            city_filter: "All",
            results_page: 0,
            workshop_page: 0,
            workspace_scroll: ScrollHandle::new(),
            scale_factor: window.scale_factor(),
            zoom: 1.,
            map_scroll: ScrollHandle::new(),
            warlords: true,
            inspecting: None,
            motion: 0,
            fields,
            colour_pickers,
            founder_age,
            search,
            pack_search,
            subscriptions,
        };
        editor.load_game(cx);
        editor
    }

    fn load_game(&mut self, cx: &mut Context<Self>) {
        if self.loading || self.exporting || self.source_loading {
            return;
        }
        self.loading = true;
        self.error = None;
        let chosen = self.game_path.clone();
        let artwork = self.artwork.clone();
        let work = cx.background_executor().spawn(async move {
            let path = chosen.context("Choose the folder where Three Kingdoms is installed.")?;
            let game = Arc::new(Game::open(&path)?);
            let portraits = portraits::catalog(&game.pack, None)?;
            let (traits, trait_error) = match game.traits() {
                Ok(traits) => (traits, None),
                Err(error) => (Vec::new(), Some(format!("{error:#}"))),
            };
            let workshop = game::workshop_packs();
            let equipment = game.equipment(None).map_err(|error| format!("{error:#}"));
            let mut trom_art = None;
            let trom_equipment = trom::packs(&workshop).ok().map(|[part_one, part_two]| {
                (|| -> Result<_> {
                    let a = Arc::new(game::open_pack(&part_one)?);
                    let b = Arc::new(game::open_pack(&part_two)?);
                    let catalog = game.equipment(Some([&a, &b]))?;
                    trom_art = Some([artwork.register(a), artwork.register(b)]);
                    Ok(catalog)
                })()
                .map_err(|error| format!("{error:#}"))
            });
            let art_id = artwork.register(game.pack.clone());
            Ok::<_, anyhow::Error>((
                path,
                Library {
                    game,
                    art_id,
                    portraits,
                    traits,
                    trait_error,
                    equipment,
                    trom_equipment,
                    trom_art,
                },
                workshop,
            ))
        });
        cx.spawn(async move |entity, cx| {
            let result = work.await;
            entity
                .update(cx, |editor, cx| {
                    editor.loading = false;
                    match result {
                        Ok((path, library, workshop)) => {
                            editor.game_path = Some(path);
                            if let Some(old) = editor.library.replace(library) {
                                editor.artwork.discard(old.art_id);
                                for id in old.trom_art.into_iter().flatten() {
                                    editor.artwork.discard(id);
                                }
                            }
                            editor.workshop = workshop;
                            if let Some(path) = editor.config.portrait_pack.clone() {
                                editor.load_source(path, cx);
                            }
                        }
                        Err(error) => editor.error = Some(format!("{error:#}")),
                    }
                    cx.notify();
                })
                .ok();
        })
        .detach();
        cx.notify();
    }

    fn load_source(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(game) = self.library.as_ref().map(|data| data.game.clone()) else {
            return;
        };
        if self.source_loading || self.loading {
            return;
        }
        self.source_loading = true;
        self.source_error = None;
        let artwork = self.artwork.clone();
        let work = cx.background_executor().spawn(async move {
            let pack = Arc::new(game::open_pack(&path)?);
            let portraits = portraits::catalog(&game.pack, Some(&pack))?;
            let art_id = artwork.register(pack.clone());
            Ok::<_, anyhow::Error>(Source {
                path,
                pack,
                art_id,
                portraits,
            })
        });
        cx.spawn(async move |entity, cx| {
            let result = work.await;
            entity
                .update(cx, |editor, cx| {
                    editor.source_loading = false;
                    match result {
                        Ok(source) => {
                            let source = Arc::new(source);
                            if editor.config.portrait_pack.as_ref() == Some(&source.path) {
                                editor.select_source(Some(source.clone()));
                            }
                            editor.browse_source(Some(source));
                            editor.results_page = 0;
                            editor.show_workshop = false;
                            editor.inspecting = None;
                            editor.motion += 1;
                        }
                        Err(error) => editor.source_error = Some(format!("{error:#}")),
                    }
                    cx.notify();
                })
                .ok();
        })
        .detach();
        cx.notify();
    }

    fn pick_folder(&mut self, destination: bool, cx: &mut Context<Self>) {
        let title = if destination {
            "Choose the export folder"
        } else {
            "Choose the Three Kingdoms installation"
        };
        let choice = choose_path(cx, title, true);
        cx.spawn(async move |entity, cx| {
            let result = choice.await;
            entity
                .update(cx, |editor, cx| {
                    match result {
                        Ok(Some(path)) => {
                            if destination {
                                editor.output = path;
                            } else {
                                if editor.game_path.as_ref() != Some(&path)
                                    || editor.heading_font.is_none()
                                {
                                    editor.heading_font = GameHeadingFont::load(&path);
                                }
                                editor.game_path = Some(path);
                                editor.load_game(cx);
                            }
                        }
                        Ok(None) => {}
                        Err(error) => editor.export_status = Some(format!("{error:#}")),
                    }
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }

    fn open_pack_dialog(&mut self, cx: &mut Context<Self>) {
        let choice = choose_path(cx, "Open a Three Kingdoms portrait pack", false);
        cx.spawn(async move |entity, cx| {
            let result = choice.await;
            entity
                .update(cx, |editor, cx| {
                    match result {
                        Ok(Some(path))
                            if path
                                .extension()
                                .is_some_and(|ext| ext.eq_ignore_ascii_case("pack")) =>
                        {
                            editor.load_source(path, cx)
                        }
                        Ok(Some(_)) => editor.source_error = Some("Choose a .pack file.".into()),
                        Ok(None) => {}
                        Err(error) => editor.source_error = Some(format!("{error:#}")),
                    }
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }

    fn change_page(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        if self.page != page {
            self.page = page;
            self.clear_search(window, cx);
            self.motion += 1;
            cx.notify();
        }
    }

    fn clear_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.results_page = 0;
        self.inspecting = None;
        self.workspace_scroll.set_offset(point(px(0.), px(0.)));
        self.search
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    fn open_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let choice = choose_path(cx, "Open a Sect Forge project", false);
        let executor = cx.background_executor().clone();
        cx.spawn_in(window, async move |entity, cx| {
            let result: Result<Option<Config>> = async {
                let Some(path) = choice.await? else {
                    return Ok(None);
                };
                anyhow::ensure!(
                    path.extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("json")),
                    "Choose a .json project."
                );
                executor
                    .spawn(async move {
                        let config: Config = serde_json::from_slice(&fs::read(&path)?)
                            .with_context(|| format!("Read {}", path.display()))?;
                        Ok(Some(config.validated()?))
                    })
                    .await
            }
            .await;
            entity
                .update_in(cx, |editor, window, cx| {
                    match result {
                        Ok(Some(config)) => {
                            let source = config.portrait_pack.clone();
                            for (input, value) in editor.fields.iter().zip([
                                &config.sect_name,
                                &config.surname,
                                &config.forename,
                                &config.primary_colour,
                                &config.secondary_colour,
                            ]) {
                                input.update(cx, |input, cx| {
                                    input.set_value(value.to_owned(), window, cx)
                                });
                            }
                            editor.founder_age.update(cx, |input, cx| {
                                input.set_value(config.founder_age.to_string(), window, cx);
                            });
                            editor.config = config;
                            editor.select_source(None);
                            if let Some(path) = source {
                                editor.load_source(path, cx);
                            }
                            editor.exported = None;
                            editor.export_status = None;
                        }
                        Ok(None) => {}
                        Err(error) => editor.export_status = Some(format!("{error:#}")),
                    }
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }

    fn save_project(&self, cx: &mut Context<Self>) {
        let config = self.config.clone();
        let directory = if self.output.is_dir() {
            self.output.clone()
        } else {
            std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir())
        };
        let choice = cx.prompt_for_new_path(&directory, Some(&format!("{}.json", config.key())));
        let executor = cx.background_executor().clone();
        cx.spawn(async move |entity, cx| {
            let result: Result<Option<PathBuf>> = async {
                let Some(mut path) = choice.await?? else {
                    return Ok(None);
                };
                if path.extension().is_none() {
                    path.set_extension("json");
                }
                executor
                    .spawn(async move {
                        let data = serde_json::to_vec_pretty(&config.validated()?)?;
                        fs::write(&path, data)
                            .with_context(|| format!("Save {}", path.display()))?;
                        Ok(Some(path))
                    })
                    .await
            }
            .await;
            entity
                .update(cx, |editor, cx| {
                    match result {
                        Ok(Some(path)) => {
                            editor.export_status = Some(format!("Saved {}", filename(&path)))
                        }
                        Ok(None) => {}
                        Err(error) => editor.export_status = Some(format!("{error:#}")),
                    }
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }

    fn export(&mut self, cx: &mut Context<Self>) {
        if self.exporting {
            return;
        }
        let Some(game) = self.library.as_ref().map(|data| data.game.clone()) else {
            return;
        };
        let config = self.config.clone();
        let output = self.output.clone();
        let (send, receive) = async_channel::unbounded();
        self.exporting = true;
        self.exported = None;
        self.export_status = Some("Preparing the campaign…".into());
        let work = cx.background_executor().spawn(async move {
            export::build(&game, config, &output, |message| {
                let _ = send.try_send(message.to_owned());
            })
        });
        cx.spawn(async move |entity, cx| {
            while let Ok(message) = receive.recv().await {
                if entity
                    .update(cx, |editor, cx| {
                        editor.export_status = Some(message);
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
            }
            let result = work.await;
            entity
                .update(cx, |editor, cx| {
                    editor.exporting = false;
                    match result {
                        Ok(path) => {
                            editor.export_status = Some(format!("Exported {}", filename(&path)));
                            editor.exported = Some(path);
                        }
                        Err(error) => editor.export_status = Some(format!("{error:#}")),
                    }
                    cx.notify();
                })
                .ok();
        })
        .detach();
        cx.notify();
    }

    fn source_portraits(&self) -> &[Portrait] {
        if let Some(source) = &self.source {
            &source.portraits
        } else {
            self.library.as_ref().map_or(&[], |data| &data.portraits)
        }
    }

    fn browse_source(&mut self, source: Option<Arc<Source>>) {
        let previous = std::mem::replace(&mut self.source, source);
        self.discard_unused_source(previous);
    }

    fn select_source(&mut self, source: Option<Arc<Source>>) {
        let previous = std::mem::replace(&mut self.selected_source, source);
        self.discard_unused_source(previous);
    }

    fn discard_unused_source(&self, source: Option<Arc<Source>>) {
        if let Some(source) = source
            && [&self.source, &self.selected_source]
                .into_iter()
                .flatten()
                .all(|current| current.art_id != source.art_id)
        {
            self.artwork.discard(source.art_id);
        }
    }

    fn art(&self, path: &str) -> Option<String> {
        let data = self.library.as_ref()?;
        data.game.pack.file(path, true)?;
        Some(asset(data.art_id, path))
    }

    fn equipment_catalog(&self) -> Option<&EquipmentCatalog> {
        let data = self.library.as_ref()?;
        if self.config.trom_compat {
            data.trom_equipment
                .as_ref()
                .and_then(|catalog| catalog.as_ref().ok())
        } else {
            data.equipment.as_ref().ok()
        }
    }

    fn remove_incompatible_equipment(&mut self) {
        let element = self.config.element();
        let Some(catalog) = self.equipment_catalog() else {
            return;
        };
        let remove: Vec<_> = self
            .config
            .starting_ancillaries
            .iter()
            .filter(|(_, key)| !catalog.get(*key).is_some_and(|item| item.fits(element)))
            .map(|(slot, _)| slot.clone())
            .collect();
        for slot in remove {
            self.config.starting_ancillaries.remove(&slot);
        }
    }

    fn portrait_art(&self, portrait: &Portrait, source: Option<&Source>) -> Option<String> {
        if let Some(source) = source {
            for path in portrait.mod_paths.iter().chain(&portrait.game_paths) {
                if source.pack.file(path, true).is_some() {
                    return Some(asset(source.art_id, path));
                }
            }
        }
        portrait.game_paths.iter().find_map(|path| self.art(path))
    }

    fn selected_portrait(&self) -> Option<(&Portrait, Option<&Source>)> {
        let (portraits, source) = if let Some(path) = &self.config.portrait_pack {
            let source = self
                .selected_source
                .as_deref()
                .filter(|source| &source.path == path)?;
            (&source.portraits, Some(source))
        } else {
            (&self.library.as_ref()?.portraits, None)
        };
        let portrait = portraits
            .iter()
            .find(|item| item.key == self.config.art_set)?;
        Some((portrait, source))
    }

    fn selected_art(&self) -> Option<String> {
        let (portrait, source) = self.selected_portrait()?;
        self.portrait_art(portrait, source)
    }

    fn founder_gender(&self) -> Option<Gender> {
        let (portrait, _) = self.selected_portrait()?;
        Gender::from_label(portrait.gender)
    }

    fn choose_founder(
        &mut self,
        key: &str,
        source: Option<Arc<Source>>,
        gender: Option<Gender>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pack = source.as_ref().map(|source| source.path.clone());
        if self.config.art_set == key && self.config.portrait_pack == pack {
            cx.notify();
            return;
        }
        let previous_gender = self.founder_gender();
        self.config.art_set = key.into();
        self.config.portrait_pack = pack;
        self.select_source(source);
        if let Some(gender) = gender {
            if gender == Gender::Female || previous_gender == Some(Gender::Female) {
                let name = name_generator::given_name(gender, &self.config.forename);
                self.set_name(NameField::Given, name, window, cx);
            }
            if let Cow::Owned(title) = name_generator::faction_title(&self.config.sect_name, gender)
                && title.chars().count() <= 64
            {
                self.set_name(NameField::Faction, title, window, cx);
            }
        }
        self.edited(cx);
    }

    fn edited(&mut self, cx: &mut Context<Self>) {
        self.exported = None;
        self.export_status = None;
        cx.notify();
    }

    fn search_box(&self) -> impl IntoElement {
        Input::new(&self.search)
            .prefix(div().text_color(rgb(GOLD)).child("⌕"))
            .w_full()
    }

    fn colour_field(&self, index: usize, label: &'static str) -> Div {
        col()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(caption(label))
            .child(
                row_ui()
                    .items_center()
                    .gap_1()
                    .child(ColorPicker::new(&self.colour_pickers[index]).cursor_pointer())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&self.fields[index + 3]).w_full()),
                    ),
            )
    }

    fn name_field(&self, field: NameField, cx: &mut Context<Self>) -> Div {
        let label = field.label();
        let needs_portrait = matches!(field, NameField::Given) && self.founder_gender().is_none();
        col()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(caption(label))
            .child(
                row_ui()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&self.fields[field as usize]).w_full()),
                    )
                    .child(
                        Button::new(("reroll-name", field as usize))
                            .outline()
                            .compact()
                            .label("Reroll")
                            .disabled(needs_portrait)
                            .tooltip(if needs_portrait {
                                "Choose a founder portrait to match the given name".into()
                            } else {
                                format!("Randomize {}", label.to_ascii_lowercase())
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.reroll_name(field, window, cx)
                            })),
                    ),
            )
    }

    fn reroll_name(&mut self, field: NameField, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.fields[field as usize].read(cx).value();
        let gender = self.founder_gender();
        let next = match field {
            NameField::Faction => {
                name_generator::faction_name(self.config.variant, gender, &current)
            }
            NameField::Family => name_generator::family_name(&current).into(),
            NameField::Given => {
                let Some(gender) = gender else { return };
                name_generator::given_name(gender, &current).into()
            }
        };
        self.set_name(field, next, window, cx);
        self.edited(cx);
    }

    fn set_name(
        &self,
        field: NameField,
        value: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.fields[field as usize].update(cx, |input, cx| input.set_value(value, window, cx));
    }

    fn rail(&self, cx: &mut Context<Self>) -> Div {
        let status = if self.loading {
            "Reading game data"
        } else if self.library.is_some() {
            "Game connected"
        } else {
            "Game setup required"
        };
        let colour = if self.library.is_some() { JADE } else { GOLD };
        col()
            .w(px(RAIL_WIDTH))
            .flex_shrink_0()
            .h_full()
            .bg(rgb(RAIL))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                col()
                    .px_6()
                    .pt_6()
                    .pb_7()
                    .gap_1()
                    .child(
                        row_ui()
                            .items_center()
                            .gap_3()
                            .child(img("sect-forge.png").size(px(32.)).flex_shrink_0())
                            .child(self.heading("Sect Forge", 24.)),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child("TOTAL WAR · THREE KINGDOMS"),
                    ),
            )
            .child(
                col()
                    .gap_1()
                    .px_3()
                    .children(Page::ALL.map(|(page, title, glyph)| {
                        let selected = page == self.page;
                        control(format!("nav-{title}"), selected)
                            .h(px(43.))
                            .w_full()
                            .justify_start()
                            .gap_3()
                            .px_3()
                            .child(
                                div()
                                    .w(px(16.))
                                    .text_color(rgb(if selected { GOLD } else { MUTED }))
                                    .child(glyph),
                            )
                            .child(title)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.change_page(page, window, cx)
                            }))
                    })),
            )
            .child(div().flex_1())
            .child(
                col()
                    .gap_2()
                    .p_4()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(
                        row_ui()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .text_size(px(12.))
                            .text_color(rgb(colour))
                            .child(
                                row_ui()
                                    .items_center()
                                    .gap_2()
                                    .child(dot(colour, 7.))
                                    .child(status),
                            )
                            .child(caption(concat!("v", env!("CARGO_PKG_VERSION")))),
                    )
                    .child(
                        control("game-folder", false)
                            .justify_start()
                            .child("Game folder…")
                            .on_click(cx.listener(|this, _, _, cx| this.pick_folder(false, cx))),
                    ),
            )
    }

    fn header(&self, title: &'static str, description: &'static str) -> Div {
        col()
            .gap_1()
            .child(self.heading(title, 40.))
            .child(body(description))
    }

    fn identity(&self, cx: &mut Context<Self>) -> AnyElement {
        col().gap_6()
            .child(self.header("Faction identity", "Choose your colours and the kind of campaign you want to play."))
            .child(row_ui().gap_6().items_start()
                .child(col().flex_1().min_w_0().gap_5()
                    .child(panel().gap_4().child(self.heading("Banner", 30.))
                        .child(self.name_field(NameField::Faction, cx))
                        .child(row_ui().gap_4().child(self.colour_field(0, "Primary colour")).child(self.colour_field(1, "Secondary colour")))
                        .child(row_ui().gap_2().items_center().flex_wrap()
                            .children([("A34434","D3AD6C"), ("326151","D2BA78"), ("34496D","D0D7DD"), ("644F73","CEADD4"), ("3E422C","B4AF75")].map(|(primary, secondary)| {
                                control(format!("palette-{primary}"), self.config.primary_colour == primary && self.config.secondary_colour == secondary)
                                    .gap_0().px_2().child(dot(colour(primary, LINE), 16.)).child(dot(colour(secondary, LINE), 12.))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        for (index, colour) in [(3, primary), (4, secondary)] {
                                            this.fields[index].update(cx, |input, cx| input.set_value(colour, window, cx));
                                        }
                                        this.edited(cx);
                                    }))
                            }))))
                    .child(panel().gap_4().child(self.heading("Campaign rules", 30.))
                        .child(row_ui().gap_3().children([Variant::Sect, Variant::Cult].map(|variant| {
                            let selected = self.config.variant == variant;
                            control(format!("variant-{}", variant.label()), selected).flex_1().min_w_0().h_auto().px_3().py_3()
                                .flex_col().items_start().gap_1()
                                .child(self.heading(variant.label(), 26.))
                                .child(div().text_size(px(12.)).font_weight(gpui::FontWeight::NORMAL).text_color(rgb(MUTED))
                                    .child(match variant {
                                        Variant::Sect => "Found chapters, raise donations, and back a patron.",
                                        Variant::Cult => "Take raid contracts and expand the Bandit Network.",
                                    }))
                                .on_click(cx.listener(move |this, _, _, cx| { this.config.variant = variant; this.edited(cx); }))
                        })))
                        .child(self.toggle("sect-mode", "Sect Mode", "Hold one commandery and research paid alternatives to imported building materials.", self.config.sect_mode,
                            |this| this.config.sect_mode = !this.config.sect_mode, cx))))
                .child(self.banner()))
            .into_any_element()
    }

    fn banner(&self) -> Div {
        let primary = colour(&self.config.primary_colour, 0x783d30);
        let secondary = colour(&self.config.secondary_colour, GOLD);
        let place = self
            .config
            .settlement()
            .map_or("Hepu", |site| site.name.as_str())
            .to_owned();
        let title = if self.config.sect_name.trim().is_empty() {
            "Your faction"
        } else {
            &self.config.sect_name
        };
        col()
            .w(relative(0.40))
            .min_w(px(250.))
            .flex_shrink_0()
            .rounded(px(8.))
            .overflow_hidden()
            .border_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(316.))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .bg(linear_gradient(
                        155.,
                        linear_color_stop(rgb(primary), 0.),
                        linear_color_stop(rgb(CARD), 1.),
                    ))
                    .when_some(self.art(MAP_IMAGE), |element, art| {
                        element.child(
                            img(art)
                                .absolute()
                                .inset_0()
                                .size_full()
                                .object_fit(ObjectFit::Cover)
                                .opacity(0.12),
                        )
                    })
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .border_1()
                                    .border_color(rgb(secondary))
                                    .rounded_full()
                                    .size(px(214.))
                                    .opacity(0.45),
                            ),
                    )
                    .when_some(self.selected_art(), |element, art| {
                        element.child(
                            img(art)
                                .absolute()
                                .bottom(px(0.))
                                .left(relative(0.06))
                                .w(relative(0.88))
                                .h(relative(0.94))
                                .object_fit(ObjectFit::Contain),
                        )
                    })
                    .child(div().absolute().inset_0().bg(linear_gradient(
                        180.,
                        linear_color_stop(rgba(0x10121000), 0.42),
                        linear_color_stop(rgba(0x101210f2), 1.),
                    )))
                    .child(
                        col()
                            .absolute()
                            .bottom_0()
                            .w_full()
                            .p_5()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(0xe9d3a4))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child("190 CE · CAMPAIGN PREVIEW"),
                            )
                            .child(self.heading(title.to_owned(), 38.)),
                    ),
            )
            .child(
                row_ui()
                    .h(px(4.))
                    .child(div().flex_1().bg(rgb(primary)))
                    .child(div().w(relative(0.25)).bg(rgb(secondary))),
            )
            .child(
                col()
                    .bg(rgb(CARD))
                    .p_5()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(14.))
                            .child(format!("{} {}", self.config.surname, self.config.forename)),
                    )
                    .child(body(format!("{place}  ·  {}", self.config.variant.label()))),
            )
    }

    fn toggle(
        &self,
        id: &'static str,
        label: &'static str,
        description: &'static str,
        enabled: bool,
        change: impl Fn(&mut Editor) + 'static,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        control(id, false)
            .w_full()
            .h_auto()
            .py_3()
            .px_3()
            .justify_between()
            .gap_3()
            .child(
                col()
                    .flex_1()
                    .min_w_0()
                    .items_start()
                    .gap_1()
                    .child(label)
                    .child(body(description).w_full()),
            )
            .child(
                row_ui()
                    .items_center()
                    .gap_2()
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(rgb(if enabled { JADE } else { MUTED }))
                    .child(if enabled { "On" } else { "Off" })
                    .child(
                        row_ui()
                            .w(px(33.))
                            .h(px(19.))
                            .px(px(3.))
                            .items_center()
                            .when(enabled, |element| element.justify_end())
                            .bg(rgb(if enabled { 0x4c663e } else { 0x343b31 }))
                            .rounded_full()
                            .child(dot(if enabled { 0xd1e5c2 } else { 0x989c90 }, 13.)),
                    ),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                change(this);
                this.edited(cx);
            }))
    }

    fn founder(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let tabs = [
            (FounderTab::Identity, "Personal details"),
            (FounderTab::Portrait, "Portrait"),
            (FounderTab::Traits, "Traits"),
            (FounderTab::Loadout, "Equipment"),
        ];
        col()
            .gap_5()
            .child(self.header(
                "Founding leader",
                "Set your founder’s age, class, appearance, traits and starting equipment.",
            ))
            .child(
                row_ui()
                    .gap_2()
                    .flex_wrap()
                    .pt_1()
                    .pb_3()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .children(tabs.map(|(tab, title)| {
                        let text = if tab == FounderTab::Traits {
                            format!("Traits · {}", self.config.starting_traits.len())
                        } else {
                            title.into()
                        };
                        control(title, self.founder_tab == tab)
                            .child(text)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.founder_tab = tab;
                                this.clear_search(window, cx);
                                this.motion += 1;
                                cx.notify();
                            }))
                    })),
            )
            .child(match self.founder_tab {
                FounderTab::Identity => self.founder_identity(cx).into_any_element(),
                FounderTab::Portrait => self.portraits(cx),
                FounderTab::Traits => self.traits(window, cx),
                FounderTab::Loadout => self.equipment(cx),
            })
            .into_any_element()
    }

    fn founder_identity(&self, cx: &mut Context<Self>) -> Div {
        panel()
            .gap_5()
            .child(
                row_ui()
                    .gap_4()
                    .child(self.name_field(NameField::Family, cx))
                    .child(self.name_field(NameField::Given, cx)),
            )
            .child(
                col()
                    .gap_2()
                    .child(caption(format!(
                        "Starting age · {MIN_FOUNDER_AGE}–{MAX_FOUNDER_AGE}"
                    )))
                    .child(
                        row_ui()
                            .id("founder-age")
                            .items_center()
                            .gap_1()
                            .w(px(152.))
                            .on_key_down(cx.listener(
                                |this, event: &gpui::KeyDownEvent, window, cx| {
                                    let step = match event.keystroke.key.as_str() {
                                        "up" => 1,
                                        "down" => -1,
                                        _ => return,
                                    };
                                    this.step_founder_age(step, window, cx);
                                    cx.stop_propagation();
                                },
                            ))
                            .child(self.age_step_button(-1, cx))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(Input::new(&self.founder_age).w_full()),
                            )
                            .child(self.age_step_button(1, cx)),
                    ),
            )
            .child(
                col().gap_2().child(caption("Character class")).child(
                    row_ui()
                        .gap_2()
                        .flex_wrap()
                        .children(CLASSES.map(|(class, name, _, tone)| {
                            control(class, self.config.subtype == class)
                                .gap_2()
                                .child(dot(tone, 8.))
                                .child(name)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.config.subtype = class.into();
                                    this.remove_incompatible_equipment();
                                    this.results_page = 0;
                                    this.edited(cx);
                                }))
                        })),
                ),
            )
    }

    fn age_step_button(&self, step: i8, cx: &mut Context<Self>) -> Button {
        let (id, tooltip, sign, limit) = if step < 0 {
            ("younger", "Decrease age", "−", MIN_FOUNDER_AGE)
        } else {
            ("older", "Increase age", "+", MAX_FOUNDER_AGE)
        };
        let disabled = self.config.founder_age == limit;
        Button::new(id)
            .w(px(36.))
            .disabled(disabled)
            .tooltip(tooltip)
            .child(
                div()
                    .text_size(px(20.))
                    .line_height(relative(1.))
                    .text_color(rgb(if disabled { MUTED } else { PAPER }))
                    .child(sign),
            )
            .on_click(
                cx.listener(move |this, _, window, cx| this.step_founder_age(step, window, cx)),
            )
    }

    fn step_founder_age(&mut self, step: i8, window: &mut Window, cx: &mut Context<Self>) {
        let age = self
            .config
            .founder_age
            .saturating_add_signed(step)
            .clamp(MIN_FOUNDER_AGE, MAX_FOUNDER_AGE);
        self.founder_age
            .update(cx, |input, cx| input.set_value(age.to_string(), window, cx));
    }

    fn portraits(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(_) = &self.library else {
            return self.game_notice(cx);
        };
        let source_label = self
            .source
            .as_ref()
            .map_or("Vanilla game", |source| filename(&source.path));
        let query = self.query(cx);
        let matches: Vec<_> = self
            .source_portraits()
            .iter()
            .filter(|item| {
                let gender = self.portrait_filter == "All" || self.portrait_filter == item.gender;
                gender
                    && (query.is_empty()
                        || matches_text(
                            &query,
                            &[&item.label, &item.search, &item.key, item.gender],
                        ))
            })
            .collect();
        let page = self.catalog_page(Catalog::Portraits, matches.len());
        let selected = self
            .source_portraits()
            .iter()
            .find(|item| self.inspecting.as_deref() == Some(item.key.as_str()))
            .or_else(|| {
                self.source_portraits()
                    .iter()
                    .find(|item| self.is_current_portrait(item))
            });
        let portraits = matches[page.range()]
            .iter()
            .map(|item| {
                let key = item.key.clone();
                let label = item.label.clone();
                let current = self.is_current_portrait(item);
                let inspecting = self.inspecting.as_ref() == Some(&key);
                let from = self.source.clone();
                let playable = item.can_export;
                let gender = Gender::from_label(item.gender);
                control(format!("portrait-{key}"), current || inspecting)
                    .w(px(138.))
                    .h(px(160.))
                    .p_0()
                    .flex_col()
                    .justify_start()
                    .gap_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .w_full()
                            .h(px(110.))
                            .bg(rgb(INK))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(picture(
                                self.portrait_art(item, self.source.as_deref()),
                                "◇",
                                ObjectFit::Contain,
                            )),
                    )
                    .child(
                        col()
                            .w_full()
                            .px_2()
                            .py_1()
                            .gap_0()
                            .items_start()
                            .child(div().w_full().truncate().text_size(px(12.)).child(label))
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(if current { GOLD } else { MUTED }))
                                    .child(if current {
                                        "Selected"
                                    } else if item.can_export {
                                        item.gender
                                    } else {
                                        "Preview only"
                                    }),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.inspecting = Some(key.clone());
                        if playable {
                            this.choose_founder(&key, from.clone(), gender, window, cx);
                        } else {
                            cx.notify();
                        }
                    }))
            })
            .collect::<Vec<_>>();
        col()
            .gap_4()
            .child(
                row_ui()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        col()
                            .min_w_0()
                            .gap_1()
                            .child(caption("Portrait source"))
                            .child(div().truncate().child(source_label.to_owned())),
                    )
                    .child(
                        row_ui()
                            .gap_2()
                            .flex_shrink_0()
                            .child(
                                control("vanilla", self.source.is_none())
                                    .child("Vanilla")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.browse_source(None);
                                        this.show_workshop = false;
                                        this.source_error = None;
                                        this.clear_search(window, cx);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                control("workshop", self.show_workshop)
                                    .child("Workshop")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.show_workshop = !this.show_workshop;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                control("open-pack", false).child("Open pack…").on_click(
                                    cx.listener(|this, _, _, cx| this.open_pack_dialog(cx)),
                                ),
                            ),
                    ),
            )
            .when(self.show_workshop, |element| {
                element.child(self.workshop(cx))
            })
            .when(self.source_loading, |element| {
                element.child(body("Reading portraits from the selected pack…"))
            })
            .when_some(self.source_error.as_ref(), |element, error| {
                element.child(notice(error.clone(), CINNABAR))
            })
            .child(
                row_ui()
                    .gap_2()
                    .items_center()
                    .child(div().flex_1().min_w_0().child(self.search_box()))
                    .children(["All", "Male", "Female", "Unknown"].map(|filter| {
                        control(
                            format!("portrait-filter-{filter}"),
                            self.portrait_filter == filter,
                        )
                        .child(filter)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.portrait_filter = filter;
                            this.results_page = 0;
                            cx.notify();
                        }))
                    })),
            )
            .child(self.pagination(Catalog::Portraits, page, false, cx))
            .child(
                row_ui()
                    .items_start()
                    .gap_4()
                    .child(
                        col()
                            .flex_1()
                            .min_w_0()
                            .gap_3()
                            .child(row_ui().gap_2().flex_wrap().children(portraits))
                            .when(matches.is_empty(), |element| {
                                element.child(notice("No portraits match this search.", MUTED))
                            })
                            .when(page.pages > 1, |list| {
                                list.child(self.pagination(Catalog::Portraits, page, true, cx))
                            }),
                    )
                    .when_some(selected, |element, item| {
                        element.child(
                            panel()
                                .w(px(215.))
                                .flex_shrink_0()
                                .p_0()
                                .overflow_hidden()
                                .child(div().h(px(214.)).w_full().bg(rgb(INK)).child(picture(
                                    self.portrait_art(item, self.source.as_deref()),
                                    "◇",
                                    ObjectFit::Contain,
                                )))
                                .child(
                                    col()
                                        .p_4()
                                        .gap_2()
                                        .child(self.heading(item.label.clone(), 25.))
                                        .child(body(item.gender))
                                        .child(body(if item.can_export {
                                            "Available for your founder."
                                        } else {
                                            "This artwork has no compatible campaign portrait set."
                                        })),
                                ),
                        )
                    }),
            )
            .into_any_element()
    }

    fn is_current_portrait(&self, item: &Portrait) -> bool {
        item.key == self.config.art_set
            && self.source.as_ref().map(|source| &source.path) == self.config.portrait_pack.as_ref()
    }

    fn workshop(&self, cx: &mut Context<Self>) -> Div {
        let query = self.pack_search.read(cx).value().to_lowercase();
        let matches: Vec<_> = self
            .workshop
            .iter()
            .filter(|path| {
                query.is_empty()
                    || matches_text(
                        &query,
                        &[
                            filename(path),
                            path.parent().map(filename).unwrap_or_default(),
                        ],
                    )
            })
            .collect();
        let page = self.catalog_page(Catalog::Workshop, matches.len());
        panel()
            .p_3()
            .gap_2()
            .child(Input::new(&self.pack_search).w_full())
            .child(self.pagination(Catalog::Workshop, page, false, cx))
            .child(
                col()
                    .id(("installed-workshop-packs", page.index))
                    .max_h(px(204.))
                    .overflow_y_scroll()
                    // GPUI's scroll listener lets wheel events reach the page underneath.
                    .occlude()
                    .gap_1()
                    .children(matches[page.range()].iter().map(|&path| {
                        let selected = self
                            .source
                            .as_ref()
                            .is_some_and(|source| &source.path == path);
                        let chosen = path.clone();
                        control(format!("workshop-{}", path.display()), selected)
                            .w_full()
                            .flex_shrink_0()
                            .gap_3()
                            .justify_start()
                            .child(div().flex_1().truncate().child(filename(path).to_owned()))
                            .child(body(
                                path.parent().map(filename).unwrap_or_default().to_owned(),
                            ))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.load_source(chosen.clone(), cx)
                            }))
                    })),
            )
            .when(matches.is_empty(), |element| {
                element.child(body(if self.workshop.is_empty() {
                    "No installed Three Kingdoms Workshop packs were found."
                } else {
                    "No Workshop packs match this search."
                }))
            })
    }

    fn traits(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(data) = &self.library else {
            return self.game_notice(cx);
        };
        if let Some(error) = &data.trait_error {
            return notice(error.clone(), CINNABAR).into_any_element();
        }
        let query = self.query(cx);
        let entries = data
            .traits
            .iter()
            .filter(|item| {
                self.trait_category
                    .is_none_or(|category| item.category == Some(category))
            })
            .filter(|item| {
                let mut fields = vec![
                    item.title.as_str(),
                    item.description.as_str(),
                    item.key.as_str(),
                ];
                fields.extend(item.effects.iter().map(String::as_str));
                matches_text(&query, &fields)
            })
            .collect::<Vec<_>>();
        let page = self.catalog_page(Catalog::Traits, entries.len());
        let columns = if window.viewport_size().width >= px(1200.) {
            3
        } else {
            2
        };
        let picks = self
            .config
            .starting_traits
            .iter()
            .filter_map(|key| data.traits.iter().find(|item| item.key == *key))
            .map(|item| {
                let key = item.key.clone();
                control(format!("remove-{key}"), true)
                    .gap_2()
                    .child(div().size(px(22.)).child(picture(
                        item.icon.as_deref().and_then(|path| self.art(path)),
                        "◇",
                        ObjectFit::Contain,
                    )))
                    .child(item.title.clone())
                    .child("×")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.config.starting_traits.retain(|item| item != &key);
                        this.edited(cx);
                    }))
            });
        col().gap_4()
            .child(panel().p_4().gap_2()
                .child(row_ui().justify_between().items_center().gap_3()
                    .child(self.heading("Starting traits", 27.))
                    .child(caption(format!("{} / 3 selected", self.config.starting_traits.len()))))
                .child(body("Choose up to three traits, including one physical trait. Leave empty to keep the campaign defaults."))
                .child(row_ui().gap_2().flex_wrap().children(picks)))
            .child(row_ui().gap_2().items_center()
                .child(div().flex_1().min_w_0().child(self.search_box()))
                .children(TraitView::ALL.map(|(view, label)| {
                    control(format!("trait-view-{label}"), self.trait_view == view)
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.trait_view = view;
                            this.results_page = 0;
                            cx.notify();
                        }))
                })))
            .child(row_ui().gap_2().flex_wrap()
                .children(std::iter::once((None, "All", MUTED))
                    .chain(TRAIT_CATEGORIES.map(|(category, label, tone)| (Some(category), label, tone)))
                    .map(|(category, label, tone)| {
                        control(format!("trait-category-{label}"), self.trait_category == category)
                            .h(px(30.)).gap_2()
                            .when(category.is_some(), |filter| filter.child(dot(tone, 7.)))
                            .child(label)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.trait_category = category;
                                this.results_page = 0;
                                cx.notify();
                            }))
                    })))
            .child(self.pagination(Catalog::Traits, page, false, cx))
            .child(col().gap_3()
                .when(self.trait_view == TraitView::Grid, |grid| grid.grid().grid_cols(columns))
                .children(entries[page.range()].iter().map(|item| self.trait_card(item, cx))))
            .when(entries.is_empty(), |element| element.child(notice("No matching traits.", MUTED)))
            .when(page.pages > 1, |list| list.child(self.pagination(Catalog::Traits, page, true, cx)))
            .into_any_element()
    }

    fn trait_card(&self, item: &Trait, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = self.config.starting_traits.contains(&item.key);
        let can_add = self.config.starting_traits.len() < 3
            && (!item.physical
                || !self
                    .config
                    .starting_traits
                    .iter()
                    .any(|key| key.contains("_physical_")));
        let key = item.key.clone();
        let kind = if item.physical {
            "Physical"
        } else {
            "Personality"
        };
        let category = TRAIT_CATEGORIES
            .iter()
            .find(|(category, _, _)| Some(*category) == item.category);
        let icon = item.icon.as_deref().and_then(|path| self.art(path));
        let grid = self.trait_view == TraitView::Grid;
        let marker = if selected {
            "✓"
        } else if can_add {
            "+"
        } else {
            ""
        };
        control(format!("trait-{key}"), selected)
            .w_full()
            .h_auto()
            .p_3()
            .items_start()
            .justify_start()
            .gap_3()
            .when(grid, |card| card.flex_col())
            .when(!selected && !can_add, |card| card.opacity(0.62))
            .child(
                row_ui()
                    .justify_between()
                    .when(grid, |header| header.w_full())
                    .child(div().size(px(38.)).flex_shrink_0().child(picture(
                        icon,
                        "◇",
                        ObjectFit::Contain,
                    )))
                    .when(grid, |header| {
                        header.child(div().text_color(rgb(GOLD)).text_size(px(16.)).child(marker))
                    }),
            )
            .child(
                col()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .when(grid, |content| content.w_full())
                    .child(
                        row_ui()
                            .items_center()
                            .gap_2()
                            .flex_wrap()
                            .child(item.title.clone())
                            .child(caption(kind))
                            .when_some(category, |heading, (_, label, tone)| {
                                heading.child(caption(*label).text_color(rgb(*tone)))
                            }),
                    )
                    .when(!item.description.is_empty(), |content| {
                        content.child(body(item.description.clone()))
                    })
                    .child(
                        col()
                            .gap_1()
                            .font_weight(gpui::FontWeight::NORMAL)
                            .text_size(px(12.))
                            .text_color(rgb(PAPER))
                            .children(item.effects.iter().map(|effect| {
                                row_ui()
                                    .gap_2()
                                    .items_start()
                                    .child(div().flex_shrink_0().text_color(rgb(GOLD)).child("•"))
                                    .child(div().flex_1().min_w_0().child(effect.clone()))
                            }))
                            .when(item.effects.is_empty(), |effects| {
                                effects.child(caption("No listed gameplay modifiers"))
                            }),
                    ),
            )
            .when(!grid, |card| {
                card.child(
                    div()
                        .flex_shrink_0()
                        .text_color(rgb(GOLD))
                        .text_size(px(16.))
                        .child(marker),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                if selected {
                    this.config.starting_traits.retain(|item| item != &key);
                } else if can_add {
                    this.config.starting_traits.push(key.clone());
                } else {
                    return;
                }
                this.edited(cx);
            }))
    }

    fn equipment(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(data) = &self.library else {
            return self.game_notice(cx);
        };
        let query = self.query(cx);
        let slot = self.slot;
        let selected = self.config.starting_ancillaries.get(slot);
        let catalog = if self.config.trom_compat {
            data.trom_equipment.as_ref().map_or_else(
                || Err("Install both TROM packs to browse TROM equipment."),
                |catalog| catalog.as_ref().map_err(String::as_str),
            )
        } else {
            data.equipment.as_ref().map_err(String::as_str)
        };
        let mut entries = catalog
            .iter()
            .flat_map(|items| items.iter())
            .filter(|(key, item)| {
                item.slot == slot
                    && self
                        .equipment_rarity
                        .is_none_or(|rarity| item.rarity == rarity)
                    && matches_text(&query, &[&item.title, key, &item.rarity, &item.description])
            })
            .collect::<Vec<_>>();
        let element = self.config.element();
        entries.sort_by_cached_key(|(_, item)| (!item.fits(element), item.title.to_lowercase()));
        let page = self.catalog_page(Catalog::Equipment, entries.len());
        col()
            .gap_4()
            .child(
                row_ui()
                    .gap_2()
                    .flex_wrap()
                    .children(SLOTS.map(|(id, name)| {
                        control(format!("slot-{id}"), slot == id)
                            .gap_2()
                            .child(name)
                            .when(
                                self.config.starting_ancillaries.contains_key(id),
                                |element| element.child(dot(JADE, 6.)),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.slot = id;
                                this.clear_search(window, cx);
                                cx.notify();
                            }))
                    })),
            )
            .child(
                row_ui()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(self.search_box()))
                    .child(
                        control("gear-default", selected.is_none())
                            .child("Campaign default")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.config.starting_ancillaries.remove(slot);
                                this.edited(cx);
                            })),
                    ),
            )
            .child(
                row_ui()
                    .gap_2()
                    .items_center()
                    .flex_wrap()
                    .child(caption("Rarity"))
                    .children(
                        std::iter::once((None, "All", MUTED))
                            .chain(RARITIES.map(|(key, title, tone)| (Some(key), title, tone)))
                            .map(|(rarity, title, tone)| {
                                control(format!("rarity-{title}"), self.equipment_rarity == rarity)
                                    .h(px(30.))
                                    .gap_2()
                                    .when(rarity.is_some(), |filter| filter.child(dot(tone, 7.)))
                                    .child(title)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.equipment_rarity = rarity;
                                        this.results_page = 0;
                                        cx.notify();
                                    }))
                            }),
                    ),
            )
            .when_some(catalog.err(), |list, error| {
                list.child(notice(error.to_owned(), GOLD))
            })
            .child(self.pagination(Catalog::Equipment, page, false, cx))
            .child(
                col()
                    .gap_2()
                    .children(entries[page.range()].iter().map(|(key, item)| {
                        let current = selected == Some(*key);
                        let key = (*key).clone();
                        let unavailable = item.unavailable.or_else(|| {
                            (!item.fits(element)).then_some("Requires a different founder class")
                        });
                        let icon = match (item.icon_source, data.trom_art) {
                            (1, Some([id, _])) | (2, Some([_, id])) => Some(asset(id, &item.icon)),
                            _ => self.art(&item.icon),
                        };
                        let (rarity, tone) = RARITIES
                            .iter()
                            .find(|(raw, _, _)| *raw == item.rarity)
                            .map_or(("Unknown", MUTED), |(_, label, tone)| (*label, *tone));
                        control(format!("equipment-{key}"), current)
                            .when(unavailable.is_some(), |row| {
                                row.tab_index(-1).cursor_default()
                            })
                            .w_full()
                            .h_auto()
                            .min_h(px(62.))
                            .py_2()
                            .items_start()
                            .justify_start()
                            .gap_4()
                            .child(div().size(px(44.)).flex_shrink_0().child(picture(
                                icon,
                                "◇",
                                ObjectFit::Contain,
                            )))
                            .child(
                                col()
                                    .flex_1()
                                    .min_w_0()
                                    .items_start()
                                    .gap_1()
                                    .child(
                                        row_ui()
                                            .gap_2()
                                            .items_center()
                                            .flex_wrap()
                                            .child(item.title.clone())
                                            .child(caption(rarity).text_color(rgb(tone))),
                                    )
                                    .child(equipment_stat_lines(&item.stats))
                                    .when(!item.description.is_empty(), |element| {
                                        element.child(body(item.description.clone()).w_full())
                                    })
                                    .when_some(unavailable, |element, reason| {
                                        element.child(caption(reason).text_color(rgb(MUTED)))
                                    }),
                            )
                            .when(unavailable.is_none(), |row| {
                                row.child(
                                    div()
                                        .flex_shrink_0()
                                        .text_color(rgb(GOLD))
                                        .text_size(px(16.))
                                        .child(if current { "✓" } else { "+" }),
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.config
                                            .starting_ancillaries
                                            .insert(slot.into(), key.clone());
                                        this.edited(cx);
                                    },
                                ))
                            })
                    })),
            )
            .when(entries.is_empty(), |element| {
                element.child(notice("No equipment matches the current filters.", MUTED))
            })
            .when(page.pages > 1, |list| {
                list.child(self.pagination(Catalog::Equipment, page, true, cx))
            })
            .into_any_element()
    }

    fn catalog_page(&self, catalog: Catalog, length: usize) -> CatalogPage {
        let page = if catalog == Catalog::Workshop {
            self.workshop_page
        } else {
            self.results_page
        };
        CatalogPage::new(page, catalog.page_size(self.trait_view), length)
    }

    fn pagination(
        &self,
        catalog: Catalog,
        page: CatalogPage,
        footer: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let activate = |index| {
            cx.listener(move |this, _: &gpui::ClickEvent, _, cx| {
                if catalog == Catalog::Workshop {
                    this.workshop_page = index;
                } else {
                    this.results_page = index;
                    if footer {
                        this.workspace_scroll.set_offset(point(px(0.), px(0.)));
                    }
                }
                cx.notify();
            })
        };
        row_ui()
            .id((catalog.label(), usize::from(footer)))
            .items_center()
            .justify_between()
            .flex_wrap()
            .gap_2()
            .child(body(page.summary(catalog.label())))
            .when(page.pages > 1, |element| {
                element.child(
                    row_ui()
                        .items_center()
                        .flex_wrap()
                        .gap_1()
                        .child(
                            Button::new("prev")
                                .outline()
                                .small()
                                .label("‹")
                                .tooltip("Previous page")
                                .disabled(page.index == 0)
                                .on_click(activate(page.index.saturating_sub(1))),
                        )
                        .children(page.buttons().into_iter().map(|button| {
                            match button {
                                Some(index) => Button::new(("page", index))
                                    .outline()
                                    .small()
                                    .min_w(px(28.))
                                    .compact()
                                    .label((index + 1).to_string())
                                    .selected(index == page.index)
                                    .tooltip(format!("Page {}", index + 1))
                                    .on_click(activate(index))
                                    .into_any_element(),
                                None => caption("…").px_1().into_any_element(),
                            }
                        }))
                        .child(
                            Button::new("next")
                                .outline()
                                .small()
                                .label("›")
                                .tooltip("Next page")
                                .disabled(page.index + 1 >= page.pages)
                                .on_click(activate(page.index + 1)),
                        ),
                )
            })
    }

    fn zoom_map(&mut self, zoom: f32, cursor: Option<Point<Pixels>>, cx: &mut Context<Self>) {
        let zoom = zoom.clamp(1., 2.5);
        if zoom == self.zoom {
            return;
        }
        let bounds = self.map_scroll.bounds();
        let anchor = cursor.unwrap_or_else(|| bounds.center()) - bounds.origin;
        let offset = self.map_scroll.offset() - anchor;
        let scale = zoom / self.zoom;
        self.map_scroll
            .set_offset(anchor + point(offset.x * scale, offset.y * scale));
        self.zoom = zoom;
        cx.notify();
    }

    fn wheel_map(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(px(24.)).y;
        if delta != px(0.) && !event.modifiers.shift {
            let scale = 1.15_f32.powf(f32::from(delta) / 72.);
            self.zoom_map(self.zoom * scale, Some(event.position), cx);
            cx.stop_propagation();
        }
    }

    fn settlement(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let query = self.query(cx);
        let sites: Vec<_> = SETTLEMENTS
            .iter()
            .filter(|site| {
                let owner = match self.city_filter {
                    "Unoccupied" => !site.han && site.key != HEPU,
                    "Han Empire" => site.han,
                    _ => true,
                };
                owner
                    && matches_text(
                        &query,
                        &[
                            &site.name,
                            &site.key,
                            site.owner_label(),
                            if site.county { "County" } else { "City" },
                        ],
                    )
            })
            .collect();
        let site = self.config.settlement().unwrap();
        let viewport = window.viewport_size();
        let fit_height = (f32::from(viewport.height) - 342.).clamp(290., 600.);
        let available_width =
            (f32::from(viewport.width) - RAIL_WIDTH - PAGE_PAD * 2. - 256.).max(400.);
        let fit = available_width.min(fit_height * 892. / 702.5);
        let width = fit * self.zoom;
        let height = width * 702.5 / 892.;
        let mut map = div()
            .relative()
            .w(px(width))
            .h(px(height))
            .flex_shrink_0()
            .bg(rgb(0x151d16));
        if let Some(art) = self.art(MAP_IMAGE) {
            map = map.child(
                img(art)
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Fill)
                    .opacity(0.78),
            );
        }
        for candidate in &sites {
            let current = candidate.key == self.config.start_region;
            if current {
                continue;
            }
            map = map.child(self.site_marker(candidate, width, height, false, cx));
        }
        if self.warlords {
            for warlord in WARLORDS.iter() {
                let primary = colour(&warlord.primary, LINE);
                let secondary = colour(&warlord.secondary, GOLD);
                let path = format!(
                    "ui/characters/{}/stills/mini/{}.png",
                    warlord.portrait, warlord.portrait
                );
                map = map.child(
                    div()
                        .id(SharedString::from(format!("warlord-{}", warlord.name)))
                        .group("warlord")
                        .absolute()
                        .left(px(f32::from(warlord.x) * width / 892. - 12.))
                        .top(px((702.5 - f32::from(warlord.y)) * height / 702.5 - 12.))
                        .child(
                            div()
                                .size(px(24.))
                                .rounded_full()
                                .overflow_hidden()
                                .border_1()
                                .border_color(rgb(secondary))
                                .bg(rgb(primary))
                                .child(picture(self.art(&path), "♙", ObjectFit::Cover)),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(px(24.))
                                .top(px(0.))
                                .hidden()
                                .group_hover("warlord", |style| style.visible())
                                .whitespace_nowrap()
                                .bg(rgb(INK))
                                .border_1()
                                .border_color(rgb(secondary))
                                .rounded(px(3.))
                                .px_2()
                                .text_size(px(11.))
                                .child(warlord.name.clone()),
                        ),
                );
            }
            map = map.child(
                div()
                    .absolute()
                    .left(px(389. * width / 892. - 4.))
                    .top(px((702.5 - 193.) * height / 702.5 - 4.))
                    .child(dot(CINNABAR, 8.)),
            );
        }
        map = map.child(self.site_marker(site, width, height, true, cx));
        col()
            .gap_4()
            .child(self.header(
                "Starting settlement",
                "Choose a city or county on the 190 CE campaign map.",
            ))
            .child(
                row_ui()
                    .gap_3()
                    .items_center()
                    .justify_between()
                    .child(
                        row_ui()
                            .gap_2()
                            .children(["All", "Unoccupied", "Han Empire"].map(|filter| {
                                control(format!("city-{filter}"), filter == self.city_filter)
                                    .child(filter)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.city_filter = filter;
                                        cx.notify();
                                    }))
                            })),
                    )
                    .child(
                        row_ui()
                            .gap_2()
                            .child(
                                control("warlords", self.warlords)
                                    .child("Warlords")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.warlords = !this.warlords;
                                        cx.notify();
                                    })),
                            )
                            .child(control("zoom-out", false).child("−").on_click(cx.listener(
                                |this, _, _, cx| this.zoom_map(this.zoom - 0.25, None, cx),
                            )))
                            .child(
                                control("zoom-reset", false)
                                    .child(format!("{:.0}%", self.zoom * 100.))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.zoom_map(1., None, cx);
                                    })),
                            )
                            .child(control("zoom-in", false).child("+").on_click(cx.listener(
                                |this, _, _, cx| this.zoom_map(this.zoom + 0.25, None, cx),
                            ))),
                    ),
            )
            .child(
                row_ui()
                    .gap_3()
                    .items_start()
                    .child(
                        col()
                            .flex_1()
                            .min_w_0()
                            .gap_2()
                            .child(
                                div()
                                    .id("settlement-map")
                                    .relative()
                                    .w_full()
                                    .h(px(fit * 702.5 / 892. + 2.))
                                    .occlude()
                                    .rounded(px(6.))
                                    .border_1()
                                    .border_color(rgb(LINE))
                                    .child(
                                        div()
                                            .id("map-viewport")
                                            .size_full()
                                            .overflow_scroll()
                                            .track_scroll(&self.map_scroll)
                                            .child(
                                                // Make the hitbox cover the map and any empty viewport space.
                                                div()
                                                    .w(px(width))
                                                    .h(px(height))
                                                    .min_w(relative(1.))
                                                    .min_h(relative(1.))
                                                    .on_scroll_wheel(cx.listener(Self::wheel_map))
                                                    .child(map),
                                            ),
                                    )
                                    .child(
                                        div().absolute().inset_0().child(
                                            Scrollbar::new(&self.map_scroll)
                                                .id("map-scrollbars")
                                                .axis(ScrollbarAxis::Both)
                                                .scrollbar_show(ScrollbarShow::Always),
                                        ),
                                    ),
                            )
                            .child(
                                row_ui()
                                    .gap_3()
                                    .flex_wrap()
                                    .child(legend(JADE, "Unoccupied"))
                                    .child(legend(GOLD, "Han Empire"))
                                    .child(legend(CINNABAR, "Guangxin · Han capital"))
                                    .child(legend(PAPER, "Selected"))
                                    .child(body("Scroll to zoom · Use scrollbars to pan")),
                            ),
                    )
                    .child(
                        col()
                            .w(px(242.))
                            .flex_shrink_0()
                            .gap_2()
                            .child(self.search_box())
                            .child(
                                panel()
                                    .p_3()
                                    .gap_1()
                                    .child(caption("Selected settlement"))
                                    .child(self.heading(site.name.clone(), 31.))
                                    .child(body(format!(
                                        "{} · {}",
                                        site.owner_label(),
                                        if site.county { "County" } else { "City" }
                                    ))),
                            )
                            .child(caption(format!("{} available", sites.len())))
                            .child(
                                col()
                                    .id("settlement-results")
                                    .h(px((fit * 702.5 / 892. - 135.).clamp(190., 465.)))
                                    .overflow_y_scroll()
                                    .occlude()
                                    .gap_1()
                                    .children(sites.iter().map(|site| {
                                        let key = site.key.clone();
                                        control(
                                            format!("settlement-{key}"),
                                            key == self.config.start_region,
                                        )
                                        .w_full()
                                        .h(px(48.))
                                        .flex_shrink_0()
                                        .justify_start()
                                        .gap_2()
                                        .child(dot(site_colour(site), 7.))
                                        .child(
                                            col()
                                                .flex_1()
                                                .min_w_0()
                                                .items_start()
                                                .gap_0()
                                                .child(site.name.clone())
                                                .child(
                                                    div()
                                                        .text_size(px(11.))
                                                        .text_color(rgb(MUTED))
                                                        .child(site.owner_label()),
                                                ),
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.config.start_region = key.clone();
                                                this.edited(cx);
                                            }),
                                        )
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn site_marker(
        &self,
        site: &Settlement,
        width: f32,
        height: f32,
        current: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let key = site.key.clone();
        let colour = if current { PAPER } else { site_colour(site) };
        let radius = if current { 15. } else { 9. };
        div()
            .id(SharedString::from(format!("marker-{key}")))
            .group("site-marker")
            .absolute()
            .cursor_pointer()
            .left(px(f32::from(site.x) * width / 892. - radius / 2.))
            .top(px(
                (702.5 - f32::from(site.y)) * height / 702.5 - radius / 2.
            ))
            .child(
                div()
                    .size(px(radius))
                    .rounded_full()
                    .border_1()
                    .border_color(rgb(INK))
                    .bg(rgb(colour)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(radius + 2.))
                    .top(px(-3.))
                    .whitespace_nowrap()
                    .text_size(px(11.))
                    .px_1()
                    .bg(rgb(INK))
                    .rounded(px(2.))
                    .when(!current, |element| {
                        element
                            .hidden()
                            .group_hover("site-marker", |style| style.visible())
                    })
                    .child(site.name.clone()),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.config.start_region = key.clone();
                this.edited(cx);
            }))
    }

    fn review(&self, cx: &mut Context<Self>) -> AnyElement {
        let site = self.config.settlement().unwrap();
        let class = CLASSES
            .iter()
            .find(|class| class.0 == self.config.subtype)
            .unwrap_or(&CLASSES[0]);
        let packs = trom::packs(&self.workshop);
        let compatibility = if packs.is_ok() {
            "TROM 5.2 Parts 1 and 2 detected."
        } else {
            "Requires both TROM 5.2 Workshop packs."
        };
        col().gap_5()
            .child(self.header("Review & export", "Build a mod pack for a new 190 CE Romance or Records campaign."))
            .child(row_ui().gap_5().items_start()
                .child(col().flex_1().min_w_0().gap_4()
                    .child(panel().gap_3()
                        .child(self.heading("Campaign", 28.))
                        .child(summary_line("Faction", self.config.sect_name.clone()))
                        .child(summary_line("Founder", format!("{} {} · {} · Age {}", self.config.surname, self.config.forename, class.1, self.config.founder_age)))
                        .child(summary_line("Settlement", format!("{} · {}", site.name, site.owner_label())))
                        .child(summary_line("Victory", self.config.variant.objective().to_owned()))
                        .child(summary_line("Territory", if self.config.sect_mode { "One commandery" } else { "Unlimited" }.into())))
                    .child(panel().gap_3()
                        .child(self.heading("Pack settings", 28.))
                        .child(self.toggle("trom", "TROM compatibility", compatibility, self.config.trom_compat,
                            |this| {
                                this.config.trom_compat = !this.config.trom_compat;
                                this.remove_incompatible_equipment();
                            }, cx))
                        .child(col().gap_2().child(caption("Output folder"))
                            .child(row_ui().gap_2().items_center()
                                .child(div().flex_1().min_w_0().text_size(px(12.)).text_color(rgb(MUTED)).child(self.output.display().to_string()))
                                .child(control("output-folder", false).flex_shrink_0().child("Change…")
                                    .on_click(cx.listener(|this, _, _, cx| this.pick_folder(true, cx))))))
                        .child(summary_line("Filename", self.config.filename()))
                        .when_some(self.config.portrait_pack.as_ref(), |element, path| {
                            element.child(summary_line("Portrait dependency", filename(path).to_owned()))
                        })
                        .when(self.config.trom_compat, |element| element.child(body("Enable the exported pack above both TROM parts in your mod manager.")))
                        .when_some(self.exported.as_ref(), |element, path| {
                            let path = path.clone();
                            element.child(control("reveal-export", true).justify_start().child("Show exported pack ↗")
                                .on_click(move |_, _, cx| cx.reveal_path(&path)))
                        })))
                .child(self.banner()))
            .into_any_element()
    }

    fn query(&self, cx: &App) -> String {
        self.search.read(cx).value().trim().to_lowercase()
    }

    fn game_notice(&self, cx: &mut Context<Self>) -> AnyElement {
        let description = if self.loading {
            "Reading portraits, traits, and artwork from your installed game…"
        } else {
            "Choose the Three Kingdoms installation to use the game’s artwork and export a pack."
        };
        col()
            .gap_3()
            .child(notice(description, MUTED))
            .when_some(self.error.as_ref(), |element, error| {
                element.child(notice(error.clone(), CINNABAR))
            })
            .when(!self.loading, |element| {
                element.child(
                    control("connect", false)
                        .child("Choose game folder…")
                        .on_click(cx.listener(|this, _, _, cx| this.pick_folder(false, cx))),
                )
            })
            .into_any_element()
    }

    fn footer(&self, cx: &mut Context<Self>) -> Div {
        let validation = self
            .config
            .clone()
            .validated()
            .err()
            .map(|error| error.to_string())
            .or_else(|| {
                self.config
                    .trom_compat
                    .then(|| {
                        trom::packs(&self.workshop)
                            .err()
                            .map(|error| error.to_string())
                    })
                    .flatten()
            });
        let message = self
            .export_status
            .clone()
            .or_else(|| self.error.clone())
            .or_else(|| validation.clone())
            .unwrap_or_else(|| {
                if self.loading {
                    "Indexing your game files…".into()
                } else if self.page == Page::Review {
                    "Enable the exported pack before starting a new campaign.".into()
                } else {
                    format!(
                        "{}  ·  {}",
                        self.config.variant.label(),
                        self.config.settlement().unwrap().name
                    )
                }
            });
        let can_export = validation.is_none()
            && self.library.is_some()
            && self.error.is_none()
            && !self.loading
            && !self.exporting;
        row_ui()
            .flex_shrink_0()
            .min_h(px(64.))
            .px(px(PAGE_PAD))
            .py_2()
            .bg(rgb(RAIL))
            .border_t_1()
            .border_color(rgb(LINE))
            .items_center()
            .gap_4()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.))
                    .text_color(rgb(if self.exported.is_some() { JADE } else { MUTED }))
                    .child(message),
            )
            .child(
                control_style(
                    "footer-primary",
                    false,
                    self.page != Page::Review || can_export,
                )
                .h(px(39.))
                .flex_shrink_0()
                .px_4()
                .bg(rgb(if self.page != Page::Review || can_export {
                    GOLD
                } else {
                    LINE
                }))
                .border_color(rgb(if self.page != Page::Review || can_export {
                    GOLD
                } else {
                    LINE
                }))
                .text_color(rgb(if self.page != Page::Review || can_export {
                    INK
                } else {
                    MUTED
                }))
                .child(match self.page {
                    Page::Identity => "Edit founder →",
                    Page::Founder => "Choose settlement →",
                    Page::Settlement => "Review pack →",
                    Page::Review if self.exporting => "Building pack…",
                    Page::Review => "Export mod pack ↗",
                })
                .on_click(cx.listener(
                    move |this, _, window, cx| match this.page {
                        Page::Identity => this.change_page(Page::Founder, window, cx),
                        Page::Founder => this.change_page(Page::Settlement, window, cx),
                        Page::Settlement => this.change_page(Page::Review, window, cx),
                        Page::Review if can_export => this.export(cx),
                        Page::Review => {}
                    },
                )),
            )
    }
}

fn equipment_stat_lines(stats: &EquipmentStats) -> Div {
    fn line(label: Option<&str>, stats: &[String], tone: u32) -> Div {
        row_ui()
            .gap_1()
            .items_center()
            .flex_wrap()
            .when_some(label, |row, label| {
                row.child(caption(label.to_owned()).mr_1())
            })
            .children(stats.iter().map(|stat| {
                div()
                    .px_2()
                    .py(px(2.))
                    .rounded(px(4.))
                    .bg(rgb(INK))
                    .text_size(px(12.))
                    .text_color(rgb(tone))
                    .child(stat.clone())
            }))
    }

    col()
        .gap_1()
        .when(!stats.battle.is_empty(), |content| {
            content.child(line(
                stats.records.as_ref().map(|_| "Romance"),
                &stats.battle,
                PAPER,
            ))
        })
        .when_some(
            stats.records.as_ref().filter(|stats| !stats.is_empty()),
            |content, records| content.child(line(Some("Records"), records, PAPER)),
        )
        .when(!stats.effects.is_empty(), |content| {
            content.child(line(None, &stats.effects, JADE))
        })
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _subscriptions = &self.subscriptions;
        self.scale_factor = window.scale_factor();
        let content = match self.page {
            Page::Identity => self.identity(cx),
            Page::Founder => self.founder(window, cx),
            Page::Settlement => self.settlement(window, cx),
            Page::Review => self.review(cx),
        };
        row_ui()
            .tab_group()
            .size_full()
            .bg(rgb(INK))
            .text_color(rgb(PAPER))
            .font_family("DM Sans")
            .text_size(px(14.))
            .child(self.rail(cx))
            .child(
                col()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(
                        row_ui()
                            .h(px(54.))
                            .flex_shrink_0()
                            .px(px(PAGE_PAD))
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(rgb(LINE))
                            .child(caption("190 CE · RISE OF THE WARLORDS"))
                            .child(
                                row_ui()
                                    .gap_2()
                                    .child(
                                        control("open-project", false)
                                            .child("Open project…")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.open_project(window, cx)
                                            })),
                                    )
                                    .child(
                                        control("save-project", false)
                                            .child("Save project…")
                                            .on_click(
                                                cx.listener(|this, _, _, cx| this.save_project(cx)),
                                            ),
                                    ),
                            ),
                    )
                    .child(
                        col()
                            .id(("workspace", self.motion))
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.workspace_scroll)
                            .px(px(PAGE_PAD))
                            .pt_6()
                            .pb_7()
                            .child(div().w_full().child(content).with_animation(
                                ("page-fade", self.motion),
                                Animation::new(Duration::from_millis(170)).with_easing(ease_in_out),
                                |element, progress| element.opacity(0.78 + 0.22 * progress),
                            )),
                    )
                    .child(self.footer(cx)),
            )
    }
}

fn asset(id: usize, path: &str) -> String {
    format!("forge-art/{id}/{path}")
}

fn choose_path(
    cx: &App,
    title: &'static str,
    directories: bool,
) -> impl Future<Output = Result<Option<PathBuf>>> + use<> {
    let selection = cx.prompt_for_paths(PathPromptOptions {
        files: !directories,
        directories,
        multiple: false,
        prompt: Some(title.into()),
    });
    async move { Ok(selection.await??.and_then(|paths| paths.into_iter().next())) }
}
fn filename(path: &Path) -> &str {
    path.file_name()
        .and_then(|part| part.to_str())
        .unwrap_or_default()
}
fn colour(value: &str, fallback: u32) -> u32 {
    u32::from_str_radix(value.trim().trim_start_matches('#'), 16)
        .ok()
        .filter(|n| *n <= 0xffffff)
        .unwrap_or(fallback)
}
fn site_colour(site: &Settlement) -> u32 {
    if site.han {
        GOLD
    } else if site.key == HEPU {
        CINNABAR
    } else {
        JADE
    }
}
fn matches_text(query: &str, fields: &[&str]) -> bool {
    query.split_whitespace().all(|word| {
        fields
            .iter()
            .any(|field| field.to_lowercase().contains(word))
    })
}
fn row_ui() -> Div {
    div().flex()
}
fn col() -> Div {
    row_ui().flex_col()
}
fn panel() -> Div {
    col()
        .p_5()
        .bg(rgb(CARD))
        .border_1()
        .border_color(rgb(LINE))
        .rounded(px(7.))
}
fn body(text: impl Into<SharedString>) -> Div {
    div()
        .font_weight(gpui::FontWeight::NORMAL)
        .text_size(px(13.))
        .text_color(rgb(MUTED))
        .child(text.into())
}
fn caption(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(11.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(MUTED))
        .child(text.into())
}
fn notice(text: impl Into<SharedString>, tone: u32) -> Div {
    panel()
        .p_3()
        .text_size(px(13.))
        .text_color(rgb(tone))
        .child(text.into())
}
fn dot(tone: u32, size: f32) -> Div {
    div()
        .size(px(size))
        .flex_shrink_0()
        .rounded_full()
        .bg(rgb(tone))
}
fn legend(tone: u32, label: &'static str) -> Div {
    row_ui()
        .items_center()
        .gap_2()
        .child(dot(tone, 7.))
        .child(caption(label))
}
fn summary_line(label: &'static str, value: String) -> Div {
    row_ui()
        .py_2()
        .border_b_1()
        .border_color(rgb(LINE))
        .items_start()
        .gap_4()
        .child(div().w(px(91.)).flex_shrink_0().child(caption(label)))
        .child(div().flex_1().min_w_0().text_size(px(13.)).child(value))
}
fn control(id: impl Into<SharedString>, selected: bool) -> Stateful<Div> {
    control_style(id, selected, false)
}

fn control_style(id: impl Into<SharedString>, selected: bool, primary: bool) -> Stateful<Div> {
    div()
        .id(id.into())
        .tab_index(0)
        .flex()
        .items_center()
        .justify_center()
        .h(px(35.))
        .px_3()
        .rounded(px(5.))
        .border_1()
        .border_color(rgb(if selected { 0x806742 } else { LINE }))
        .bg(rgb(if selected { 0x343122 } else { CARD }))
        .text_color(rgb(if selected { GOLD } else { PAPER }))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_size(px(13.))
        .cursor_pointer()
        .hover(move |style| {
            if primary {
                style.bg(rgb(0xf1ce8e)).border_color(rgb(PAPER))
            } else {
                style.bg(rgb(LIFT)).border_color(rgb(0x68735f))
            }
        })
        .focus(|style| style.border_color(rgb(GOLD)))
        .active(|style| style.opacity(0.75))
}
fn picture(path: Option<String>, fallback: &'static str, fit: ObjectFit) -> AnyElement {
    match path {
        Some(path) => img(path).size_full().object_fit(fit).into_any_element(),
        None => row_ui()
            .size_full()
            .items_center()
            .justify_center()
            .text_color(rgb(MUTED))
            .child(fallback)
            .into_any_element(),
    }
}

pub fn run(game_path: Option<PathBuf>) -> Result<()> {
    let game_path = game_path.or_else(game::discover_game);
    let heading_font = game_path.as_deref().and_then(GameHeadingFont::load);
    let artwork = Artwork::default();
    Application::new()
        .with_assets(artwork.clone())
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            if let Err(error) = cx
                .text_system()
                .add_fonts(vec![Cow::Borrowed(include_bytes!("data/fonts/DMSans.ttf"))])
            {
                eprintln!("Load Sect Forge fonts: {error:#}");
            }
            Theme::change(ThemeMode::Dark, None, cx);
            let theme = Theme::global_mut(cx);
            theme.font_family = "DM Sans".into();
            theme.background = Hsla::from(rgb(INK));
            theme.foreground = Hsla::from(rgb(PAPER));
            theme.border = Hsla::from(rgb(LINE));
            theme.input = Hsla::from(rgb(LINE));
            theme.ring = Hsla::from(rgb(GOLD));
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1260.), px(840.)),
                    cx,
                ))),
                window_min_size: Some(size(px(1000.), px(690.))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Sect Forge".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            match cx.open_window(options, move |window, cx| {
                let editor = cx.new(|cx| Editor::new(window, cx, artwork, game_path, heading_font));
                cx.new(|cx| Root::new(editor, window, cx))
            }) {
                Ok(_) => cx.activate(true),
                Err(error) => {
                    eprintln!("Open Sect Forge window: {error:#}");
                    cx.quit();
                }
            }
        });
    Ok(())
}
