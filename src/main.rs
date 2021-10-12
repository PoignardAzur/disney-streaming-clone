// Copyright 2019 The Druid Authors.

// On Windows platform, don't show a console when opening the app.
#![windows_subsystem = "windows"]
#![allow(unused_imports)]

use smallvec::{smallvec, SmallVec};
use tracing::{error, trace, trace_span, warn, Span};

use widget_cruncher::promise::PromiseToken;
use widget_cruncher::shell::keyboard_types::Key;
use widget_cruncher::widget::prelude::*;
use widget_cruncher::widget::{
    AsWidgetPod, ClipBox, FillStrat, Flex, Image, Label, SizedBox, Spinner, WebImage, WidgetId,
    WidgetPod,
};
use widget_cruncher::{AppLauncher, Color, Command, Point, Rect, Selector, Target, WindowDesc};

const CHANGE_SELECTED_ITEM: Selector<(usize, usize)> = Selector::new("change_selected_item");
const REQUEST_FOCUS: Selector = Selector::new("request_focus");

struct RootWidget {
    pub children_promise: PromiseToken<Vec<ContentSetMetadata>>,
    pub children: WidgetPod<ClipBox<Flex>>,
    pub selected_item: (usize, usize),
}

struct ContentSetMetadata {
    pub title: String,
    pub ref_id: String,
}

struct ContentSet {
    pub row: usize,
    pub data: ContentSetMetadata,
    pub children_promise: PromiseToken<Vec<String>>,
    pub children: WidgetPod<Flex>,
}

struct Thumbnail {
    pub row: usize,
    pub column: usize,
    pub inner: WidgetPod<WebImage>,
    pub selected: bool,
    pub selected_progress: u32,
}

// --- METHODS ---

impl RootWidget {
    pub fn new() -> Self {
        let placeholder = Spinner::new();
        let column = Flex::column().with_child(placeholder);
        let clipbox = ClipBox::new(column).constrain_horizontal(true);
        Self {
            children_promise: PromiseToken::empty(),
            children: WidgetPod::new(clipbox),
            selected_item: (0, 0),
        }
    }
}

impl ContentSet {
    pub fn new(row: usize, data: ContentSetMetadata) -> Self {
        let title_label = Label::new(data.title.clone());
        let placeholder = Spinner::new();
        Self {
            row,
            data,
            children_promise: PromiseToken::empty(),
            children: WidgetPod::new(
                Flex::column()
                    .with_child(title_label)
                    .with_child(placeholder),
            ),
        }
    }
}

impl Thumbnail {
    pub fn new(row: usize, column: usize, thumbnail_url: String) -> Self {
        let image = WebImage::new(thumbnail_url);
        Self {
            row,
            column,
            inner: WidgetPod::new(image),
            selected: false,
            selected_progress: 0,
        }
    }
}

fn load_collection(url: &str) -> Result<Vec<ContentSetMetadata>, reqwest::Error> {
    let json: serde_json::Value = reqwest::blocking::get(url)?.json()?;
    let containers = json["data"]["StandardCollection"]["containers"].clone();
    let container_items = containers
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|container| {
            let title = container["set"]["text"]["title"]["full"]["set"]["default"]["content"]
                .as_str()?
                .to_string();
            let ref_id = container["set"]["refId"].as_str()?.to_string();
            Some(ContentSetMetadata { title, ref_id })
        })
        .collect::<Vec<_>>();
    Ok(container_items)
}

fn load_content_set(url: &str) -> Result<Vec<String>, reqwest::Error> {
    let json: serde_json::Value = reqwest::blocking::get(url)?.json()?;
    let items = json["data"]["CuratedSet"]["items"].clone();
    let items_tiles = items
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|item| {
            let tileset = item["image"]["tile"].clone();
            // Just take the first suggested tile.
            let tile = tileset.as_object().unwrap().values().next()?;
            let tile_url = tile["program"]["default"]["url"].as_str()?.to_string();

            Some(tile_url)
        })
        .collect::<Vec<_>>();
    Ok(items_tiles)
}

// --- TRAIT IMPLS ---

impl Widget for RootWidget {
    fn on_event(&mut self, ctx: &mut EventCtx, event: &Event, env: &Env) {
        ctx.init();
        match event {
            Event::PromiseResult(result) => {
                if let Some(children) = result.try_get(self.children_promise) {
                    // TODO - Need to find a more idiomatic way to do this.
                    self.children.recurse_pass(
                        "custom_pass",
                        &mut ctx.widget_state,
                        |clipbox, clipbox_state| {
                            clipbox.child.recurse_pass(
                                "custom_pass",
                                clipbox_state,
                                |flex, flex_state| {
                                    flex.clear(flex_state);
                                    for (row, child) in children.into_iter().enumerate() {
                                        flex.add_child(flex_state, ContentSet::new(row, child));
                                    }
                                },
                            );
                        },
                    );

                    ctx.skip_child(&mut self.children);
                    return;
                }
            }
            Event::KeyDown(key_event) => {
                // This is a HUGE cheat.
                match &key_event.key {
                    Key::ArrowDown => {
                        self.selected_item.0 = self.selected_item.0.saturating_add(1);
                    }
                    Key::ArrowLeft => {
                        self.selected_item.1 = self.selected_item.1.saturating_sub(1);
                    }
                    Key::ArrowRight => {
                        self.selected_item.1 = self.selected_item.1.saturating_add(1);
                    }
                    Key::ArrowUp => {
                        self.selected_item.0 = self.selected_item.0.saturating_sub(1);
                    }
                    _ => {}
                }

                ctx.submit_command(CHANGE_SELECTED_ITEM.with(self.selected_item));
            }
            Event::Command(command) if command.is(REQUEST_FOCUS) => {
                ctx.request_focus();
            }
            _ => {}
        }
        self.children.on_event(ctx, event, env)
    }

    fn on_status_change(&mut self, _ctx: &mut LifeCycleCtx, _event: &StatusChange, _env: &Env) {}

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, env: &Env) {
        const COLLECTION_URL: &str = "https://cd-static.bamgrid.com/dp-117731241344/home.json";

        ctx.init();
        match event {
            LifeCycle::BuildFocusChain => {
                ctx.register_for_focus();
                ctx.submit_command(
                    Command::from(REQUEST_FOCUS).to(Target::Widget(ctx.widget_id())),
                );
            }
            LifeCycle::WidgetAdded => {
                self.children_promise =
                    ctx.compute_in_background(move |_| load_collection(COLLECTION_URL).unwrap());
            }
            _ => {}
        }
        self.children.lifecycle(ctx, event, env)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, env: &Env) -> Size {
        let layout = self.children.layout(ctx, bc, env);
        self.children.set_origin(ctx, env, Point::ORIGIN);
        layout
    }

    fn paint(&mut self, ctx: &mut PaintCtx, env: &Env) {
        self.children.paint(ctx, env)
    }

    fn children(&self) -> SmallVec<[&dyn AsWidgetPod; 16]> {
        smallvec![&self.children as &dyn AsWidgetPod]
    }

    fn children_mut(&mut self) -> SmallVec<[&mut dyn AsWidgetPod; 16]> {
        smallvec![&mut self.children as &mut dyn AsWidgetPod]
    }

    fn make_trace_span(&self) -> Span {
        trace_span!("RootWidget")
    }
}

impl Widget for ContentSet {
    fn on_event(&mut self, ctx: &mut EventCtx, event: &Event, env: &Env) {
        ctx.init();
        match event {
            Event::PromiseResult(result) => {
                if let Some(children) = result.try_get(self.children_promise) {
                    let row = self.row;
                    let title = self.data.title.clone();
                    self.children.recurse_pass(
                        "custom_pass",
                        &mut ctx.widget_state,
                        |flex, flex_state| {
                            flex.clear(flex_state);
                            flex.add_child(flex_state, Label::new(title));
                            let mut titles = Flex::row();
                            for (column, child) in children.into_iter().enumerate() {
                                titles = titles.with_child(Thumbnail::new(row, column, child));
                            }
                            flex.add_child(
                                flex_state,
                                ClipBox::new(titles).constrain_vertical(true),
                            );
                        },
                    );

                    ctx.skip_child(&mut self.children);
                    return;
                }
            }
            _ => {}
        }
        self.children.on_event(ctx, event, env)
    }

    fn on_status_change(&mut self, _ctx: &mut LifeCycleCtx, _event: &StatusChange, _env: &Env) {}

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, env: &Env) {
        let content_set_url = format!(
            "https://cd-static.bamgrid.com/dp-117731241344/sets/{}.json",
            self.data.ref_id
        );

        ctx.init();
        match event {
            LifeCycle::WidgetAdded => {
                self.children_promise =
                    ctx.compute_in_background(move |_| load_content_set(&content_set_url).unwrap());
            }
            _ => {}
        }
        self.children.lifecycle(ctx, event, env)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx, bc: &BoxConstraints, env: &Env) -> Size {
        let layout = self.children.layout(ctx, bc, env);
        self.children.set_origin(ctx, env, Point::ORIGIN);
        layout
    }

    fn paint(&mut self, ctx: &mut PaintCtx, env: &Env) {
        self.children.paint(ctx, env)
    }

    fn children(&self) -> SmallVec<[&dyn AsWidgetPod; 16]> {
        smallvec![&self.children as &dyn AsWidgetPod]
    }

    fn children_mut(&mut self) -> SmallVec<[&mut dyn AsWidgetPod; 16]> {
        smallvec![&mut self.children as &mut dyn AsWidgetPod]
    }

    fn make_trace_span(&self) -> Span {
        trace_span!("ContentSet")
    }
}

impl Widget for Thumbnail {
    fn on_event(&mut self, ctx: &mut EventCtx, event: &Event, env: &Env) {
        ctx.init();
        match event {
            Event::Command(command) => {
                if let Some((row, col)) = command.try_get(CHANGE_SELECTED_ITEM) {
                    if (*row, *col) == (self.row, self.column) {
                        self.selected = true;
                        ctx.request_anim_frame();
                        ctx.request_layout();
                        ctx.request_pan_to_this();
                    } else if self.selected {
                        self.selected = false;
                        ctx.request_anim_frame();
                        ctx.request_layout();
                    }
                }
            }
            // TODO - handle frame interval?
            Event::AnimFrame(_interval) => {
                if self.selected {
                    if self.selected_progress < 5 {
                        self.selected_progress += 1;
                        ctx.request_anim_frame();
                        ctx.request_layout();
                    }
                } else {
                    if self.selected_progress > 0 {
                        self.selected_progress -= 1;
                        ctx.request_anim_frame();
                        ctx.request_layout();
                    }
                }
            }
            _ => {}
        }
        self.inner.on_event(ctx, event, env)
    }

    fn on_status_change(&mut self, _ctx: &mut LifeCycleCtx, _event: &StatusChange, _env: &Env) {}

    fn lifecycle(&mut self, ctx: &mut LifeCycleCtx, event: &LifeCycle, env: &Env) {
        self.inner.lifecycle(ctx, event, env)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx, _bc: &BoxConstraints, env: &Env) -> Size {
        const THUMBNAIL_MAX_SIZE: f64 = 200.0;
        let square_side = THUMBNAIL_MAX_SIZE * (0.90 + (self.selected_progress as f64) / 50.0);
        let child_constraints = BoxConstraints::new(
            Size::new(square_side, square_side),
            Size::new(square_side, square_side),
        );

        let outer_size = Size::new(THUMBNAIL_MAX_SIZE, THUMBNAIL_MAX_SIZE);
        let image_size = self.inner.layout(ctx, &child_constraints, env);
        let origin = (outer_size - image_size) / 2.0;
        self.inner.set_origin(ctx, env, origin.to_vec2().to_point());
        outer_size
    }

    fn paint(&mut self, ctx: &mut PaintCtx, env: &Env) {
        self.inner.paint(ctx, env);

        if self.selected {
            let border_width = 4.0;
            let border_color = Color::WHITE;
            let border_rect = self.inner.layout_rect();
            ctx.stroke(border_rect, &border_color, border_width);
        }
    }

    fn children(&self) -> SmallVec<[&dyn AsWidgetPod; 16]> {
        smallvec![&self.inner as &dyn AsWidgetPod]
    }

    fn children_mut(&mut self) -> SmallVec<[&mut dyn AsWidgetPod; 16]> {
        smallvec![&mut self.inner as &mut dyn AsWidgetPod]
    }

    fn make_trace_span(&self) -> Span {
        trace_span!("Thumbnail")
    }
}

// ---

fn main() {
    let main_window = WindowDesc::new(RootWidget::new()).title("Title list");
    AppLauncher::with_window(main_window)
        .log_to_console()
        .launch()
        .expect("launch failed");
}
